use super::{Store, StoreError};
use sqlx::types::Uuid;

impl Store {
    /// Renew only this live execution; a dropped execution future stops heartbeating.
    pub async fn heartbeat_execution(&self, id: Uuid) -> Result<bool, StoreError> {
        let updated = sqlx::query("UPDATE executions SET lease_expires_at = now() + interval '90 seconds' WHERE id=$1 AND lease_owner=$2 AND status='running' AND lease_expires_at > now()")
            .bind(id).bind(self.owner).execute(&self.pool).await?;
        Ok(updated.rows_affected() == 1)
    }

    pub async fn heartbeat_job(&self, id: Uuid) -> Result<bool, StoreError> {
        let updated = sqlx::query("UPDATE jobs SET lease_expires_at = now() + interval '90 seconds' WHERE id=$1 AND lease_owner=$2 AND status='running' AND lease_expires_at > now()")
            .bind(id).bind(self.owner).execute(&self.pool).await?;
        Ok(updated.rows_affected() == 1)
    }

    /// Record an uncertain outcome without replaying an external side effect.
    pub async fn interrupt_execution(&self, id: Uuid) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        let owned = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM executions WHERE id=$1 AND lease_owner=$2 FOR UPDATE",
        )
        .bind(id)
        .bind(self.owner)
        .fetch_optional(&mut *tx)
        .await?;
        if owned.is_some() {
            sqlx::query("UPDATE executions SET status='failed', finished_at=now(), error='execution interrupted; side effects may have occurred; manual review required' WHERE id=$1 AND status IN ('queued','running')")
                .bind(id).execute(&mut *tx).await?;
            sqlx::query("UPDATE execution_steps SET status='failed', finished_at=now(), error='execution interrupted; outcome uncertain', duration_ms=GREATEST(0,(EXTRACT(EPOCH FROM (now()-started_at))*1000)::BIGINT) WHERE execution_id=$1 AND status='running'")
                .bind(id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Terminalize a specific expired execution before returning an idempotent replay response.
    pub async fn recover_expired_execution(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        let expired = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM executions
             WHERE id=$1 AND tenant_id=$2
               AND status IN ('queued', 'running') AND lease_expires_at <= now()
             FOR UPDATE",
        )
        .bind(execution_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(id) = expired {
            sqlx::query(
                "UPDATE executions SET status='failed', finished_at=now(),
                 error='execution lease expired; side effects may have occurred; manual review required'
                 WHERE id=$1 AND status IN ('queued', 'running')",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE execution_steps SET status='failed', finished_at=now(),
                 error='execution lease expired; outcome uncertain',
                 duration_ms=GREATEST(0,(EXTRACT(EPOCH FROM (now()-started_at))*1000)::BIGINT)
                 WHERE execution_id=$1 AND status='running'",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Last-resort cleanup after the bounded drain; never modifies other owners.
    pub async fn interrupt_owned_work(&self) -> Result<(), StoreError> {
        let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM executions WHERE lease_owner=$1 AND status IN ('queued','running','cancelled')")
            .bind(self.owner).fetch_all(&self.pool).await?;
        for id in ids {
            self.interrupt_execution(id).await?;
        }
        sqlx::query("UPDATE jobs SET status='failed', finished_at=now(), error='worker interrupted; outcome uncertain' WHERE lease_owner=$1 AND status='running'")
            .bind(self.owner).execute(&self.pool).await?;
        Ok(())
    }
}
