use chrono::{DateTime, Utc};
use crate::{execution::ExecutionStatus, workflow::WorkflowDefinition};
use serde_json::Value;
use sqlx::{types::{Json, Uuid}, PgPool};

#[derive(Debug)]
pub enum StoreError {
    InvalidTransition {
        from: ExecutionStatus,
        to: ExecutionStatus,
    },
    Database(sqlx::Error),
    NotFound(&'static str),
}

impl From<sqlx::Error> for StoreError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

#[derive(Clone)]
pub struct Store {
    pool: PgPool,
}

impl Store {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_execution(
        &self,
        tenant_id: Uuid,
        workflow_id: Uuid,
        workflow_version_id: Uuid,
        trigger_type: &str,
        trigger_node_id: &str,
        input: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<CreatedExecution, StoreError> {
        let inserted = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO executions (
                tenant_id,
                workflow_id,
                workflow_version_id,
                trigger_type,
                trigger_node_id,
                input,
                idempotency_key
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (workflow_id, idempotency_key)
                WHERE idempotency_key IS NOT NULL
            DO NOTHING
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(workflow_id)
        .bind(workflow_version_id)
        .bind(trigger_type)
        .bind(trigger_node_id)
        .bind(input)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(id) = inserted {
            return Ok(CreatedExecution { id, created: true });
        }

        let idempotency_key =
            idempotency_key.ok_or(StoreError::NotFound("created execution"))?;
        let id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id
            FROM executions
            WHERE workflow_id = $1
              AND idempotency_key = $2
            "#,
        )
        .bind(workflow_id)
        .bind(idempotency_key)
        .fetch_one(&self.pool)
        .await?;

        Ok(CreatedExecution { id, created: false })
    }

    pub async fn transition_execution(
        &self,
        execution_id: Uuid,
        from: ExecutionStatus,
        to: ExecutionStatus,
        output: Option<&Value>,
        error: Option<&str>,
    ) -> Result<bool, StoreError> {
        if !from.can_transition_to(to) {
            return Err(StoreError::InvalidTransition { from, to });
        }

        let result = sqlx::query(
            r#"
            UPDATE executions
            SET status = $3,
                attempts = CASE WHEN $3 = 'running' THEN attempts + 1 ELSE attempts END,
                started_at = CASE
                    WHEN $3 = 'running' THEN COALESCE(started_at, now())
                    ELSE started_at
                END,
                finished_at = CASE
                    WHEN $3 IN ('succeeded', 'failed', 'cancelled') THEN now()
                    ELSE finished_at
                END,
                output = COALESCE($4, output),
                error = $5
            WHERE id = $1
              AND status = $2
            "#,
        )
        .bind(execution_id)
        .bind(from.as_str())
        .bind(to.as_str())
        .bind(output.cloned())
        .bind(error)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn start_step(
        &self,
        execution_id: Uuid,
        node_id: &str,
        node_type: &str,
        input: &Value,
    ) -> Result<Uuid, StoreError> {
        Ok(sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO execution_steps (
                execution_id,
                node_id,
                node_type,
                status,
                input
            )
            VALUES ($1, $2, $3, 'running', $4)
            RETURNING id
            "#,
        )
        .bind(execution_id)
        .bind(node_id)
        .bind(node_type)
        .bind(input)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn finish_step(
        &self,
        step_id: Uuid,
        output: Option<&Value>,
        error: Option<&str>,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            r#"
            UPDATE execution_steps
            SET status = CASE WHEN $3::text IS NULL THEN 'succeeded' ELSE 'failed' END,
                output = $2,
                error = $3,
                finished_at = now(),
                duration_ms = GREATEST(
                    0,
                    (EXTRACT(EPOCH FROM (now() - started_at)) * 1000)::BIGINT
                )
            WHERE id = $1
              AND status = 'running'
            "#,
        )
        .bind(step_id)
        .bind(output.cloned())
        .bind(error)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }
}


pub struct CredentialRecord {
    pub kind: String,
    pub encrypted_value: Vec<u8>,
}

impl Store {
    pub async fn create_credential(
        &self,
        tenant_id: Uuid,
        name: &str,
        kind: &str,
        encrypted_value: &[u8],
    ) -> Result<Uuid, StoreError> {
        Ok(sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO credentials (tenant_id, name, kind, encrypted_value)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(name)
        .bind(kind)
        .bind(encrypted_value)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn credential(
        &self,
        tenant_id: Uuid,
        credential_id: Uuid,
    ) -> Result<Option<CredentialRecord>, StoreError> {
        let row = sqlx::query_as::<_, (String, Vec<u8>)>(
            r#"
            SELECT kind, encrypted_value
            FROM credentials
            WHERE id = $1
              AND tenant_id = $2
            "#,
        )
        .bind(credential_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(kind, encrypted_value)| CredentialRecord {
            kind,
            encrypted_value,
        }))
    }
}


#[derive(Debug, Clone, Copy)]
pub struct CreatedExecution {
    pub id: Uuid,
    pub created: bool,
}

#[derive(Debug, Clone)]
pub struct WorkflowVersionRecord {
    pub tenant_id: Uuid,
    pub workflow_id: Uuid,
    pub workflow_version_id: Uuid,
    pub definition: WorkflowDefinition,
}

#[derive(Debug, Clone)]
pub struct ExecutionRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub workflow_id: Uuid,
    pub workflow_version_id: Uuid,
    pub trigger_type: String,
    pub trigger_node_id: String,
    pub status: String,
    pub input: Value,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
}

impl Store {
    pub async fn create_workflow(
        &self,
        tenant_id: Uuid,
        name: &str,
        definition: &WorkflowDefinition,
    ) -> Result<(Uuid, Uuid), StoreError> {
        let mut tx = self.pool.begin().await?;

        let workflow_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO workflows (tenant_id, name)
            VALUES ($1, $2)
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(name)
        .fetch_one(&mut *tx)
        .await?;

        let version_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO workflow_versions (workflow_id, version, definition)
            VALUES ($1, 1, $2)
            RETURNING id
            "#,
        )
        .bind(workflow_id)
        .bind(Json(definition.clone()))
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE workflows
            SET active_version_id = $2,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(workflow_id)
        .bind(version_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok((workflow_id, version_id))
    }

    pub async fn save_workflow_version(
        &self,
        tenant_id: Uuid,
        workflow_id: Uuid,
        definition: &WorkflowDefinition,
    ) -> Result<Uuid, StoreError> {
        let mut tx = self.pool.begin().await?;

        let exists = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1
            FROM workflows
            WHERE id = $1
              AND tenant_id = $2
            FOR UPDATE
            "#,
        )
        .bind(workflow_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?;

        if exists.is_none() {
            return Err(StoreError::NotFound("workflow"));
        }

        let version = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT COALESCE(MAX(version), 0)::int + 1
            FROM workflow_versions
            WHERE workflow_id = $1
            "#,
        )
        .bind(workflow_id)
        .fetch_one(&mut *tx)
        .await?;

        let version_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO workflow_versions (workflow_id, version, definition)
            VALUES ($1, $2, $3)
            RETURNING id
            "#,
        )
        .bind(workflow_id)
        .bind(version)
        .bind(Json(definition.clone()))
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE workflows
            SET updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(version_id)
    }

    pub async fn activate_workflow(
        &self,
        tenant_id: Uuid,
        workflow_id: Uuid,
        workflow_version_id: Uuid,
    ) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;

        let result = sqlx::query(
            r#"
            UPDATE workflows AS w
            SET active = TRUE,
                active_version_id = $3,
                updated_at = now()
            WHERE w.id = $2
              AND w.tenant_id = $1
              AND EXISTS (
                  SELECT 1
                  FROM workflow_versions AS v
                  WHERE v.id = $3
                    AND v.workflow_id = w.id
              )
            "#,
        )
        .bind(tenant_id)
        .bind(workflow_id)
        .bind(workflow_version_id)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(false);
        }

        sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'cancelled',
                error = 'superseded by a newer active workflow version',
                finished_at = now()
            WHERE tenant_id = $1
              AND workflow_id = $2
              AND workflow_version_id <> $3
              AND status = 'queued'
            "#,
        )
        .bind(tenant_id)
        .bind(workflow_id)
        .bind(workflow_version_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(true)
    }

    pub async fn deactivate_workflow(
        &self,
        tenant_id: Uuid,
        workflow_id: Uuid,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            r#"
            UPDATE workflows
            SET active = FALSE,
                updated_at = now()
            WHERE id = $1
              AND tenant_id = $2
            "#,
        )
        .bind(workflow_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn active_workflow(
        &self,
        workflow_id: Uuid,
    ) -> Result<Option<WorkflowVersionRecord>, StoreError> {
        let row = sqlx::query_as::<_, (Uuid, Uuid, Uuid, Json<WorkflowDefinition>)>(
            r#"
            SELECT w.tenant_id, w.id, v.id, v.definition
            FROM workflows AS w
            JOIN workflow_versions AS v
              ON v.id = w.active_version_id
             AND v.workflow_id = w.id
            WHERE w.id = $1
              AND w.active = TRUE
            "#,
        )
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(tenant_id, workflow_id, workflow_version_id, definition)| {
                WorkflowVersionRecord {
                    tenant_id,
                    workflow_id,
                    workflow_version_id,
                    definition: definition.0,
                }
            },
        ))
    }

    pub async fn latest_workflow(
        &self,
        tenant_id: Uuid,
        workflow_id: Uuid,
    ) -> Result<Option<WorkflowVersionRecord>, StoreError> {
        let row = sqlx::query_as::<_, (Uuid, Uuid, Uuid, Json<WorkflowDefinition>)>(
            r#"
            SELECT w.tenant_id, w.id, v.id, v.definition
            FROM workflows AS w
            JOIN LATERAL (
                SELECT id, definition
                FROM workflow_versions
                WHERE workflow_id = w.id
                ORDER BY version DESC
                LIMIT 1
            ) AS v ON TRUE
            WHERE w.id = $1
              AND w.tenant_id = $2
            "#,
        )
        .bind(workflow_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(tenant_id, workflow_id, workflow_version_id, definition)| {
                WorkflowVersionRecord {
                    tenant_id,
                    workflow_id,
                    workflow_version_id,
                    definition: definition.0,
                }
            },
        ))
    }

    pub async fn execution(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
    ) -> Result<Option<ExecutionRecord>, StoreError> {
        let row = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                Uuid,
                Uuid,
                String,
                String,
                String,
                Value,
                Option<Value>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<i64>,
            ),
        >(
            r#"
            SELECT
                id,
                tenant_id,
                workflow_id,
                workflow_version_id,
                trigger_type,
                trigger_node_id,
                status,
                input,
                output,
                error,
                started_at::text,
                finished_at::text,
                CASE
                    WHEN started_at IS NULL THEN NULL
                    ELSE (EXTRACT(EPOCH FROM (COALESCE(finished_at, now()) - started_at)) * 1000)::BIGINT
                END
            FROM executions
            WHERE id = $1
              AND tenant_id = $2
            "#,
        )
        .bind(execution_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(
                id,
                tenant_id,
                workflow_id,
                workflow_version_id,
                trigger_type,
                trigger_node_id,
                status,
                input,
                output,
                error,
                started_at,
                finished_at,
                duration_ms,
            )| ExecutionRecord {
                id,
                tenant_id,
                workflow_id,
                workflow_version_id,
                trigger_type,
                trigger_node_id,
                status,
                input,
                output,
                error,
                started_at,
                finished_at,
                duration_ms,
            },
        ))
    }

    pub async fn workflow_version(
        &self,
        tenant_id: Uuid,
        workflow_id: Uuid,
        workflow_version_id: Uuid,
    ) -> Result<Option<WorkflowVersionRecord>, StoreError> {
        let row = sqlx::query_as::<_, (Uuid, Uuid, Uuid, Json<WorkflowDefinition>)>(
            r#"
            SELECT w.tenant_id, w.id, v.id, v.definition
            FROM workflows AS w
            JOIN workflow_versions AS v
              ON v.workflow_id = w.id
            WHERE w.id = $1
              AND w.tenant_id = $2
              AND v.id = $3
            "#,
        )
        .bind(workflow_id)
        .bind(tenant_id)
        .bind(workflow_version_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(tenant_id, workflow_id, workflow_version_id, definition)| {
                WorkflowVersionRecord {
                    tenant_id,
                    workflow_id,
                    workflow_version_id,
                    definition: definition.0,
                }
            },
        ))
    }

    pub async fn cancel_execution(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            r#"
            UPDATE executions
            SET status = 'cancelled',
                finished_at = now()
            WHERE id = $1
              AND tenant_id = $2
              AND status IN ('queued', 'running')
            "#,
        )
        .bind(execution_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }


    pub async fn is_execution_cancelled(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
    ) -> Result<bool, StoreError> {
        Ok(sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM executions
                WHERE id = $1
                  AND tenant_id = $2
                  AND status = 'cancelled'
            )
            "#,
        )
        .bind(execution_id)
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await?)
    }

}


#[derive(Debug, Clone)]
pub struct WorkflowSummaryRecord {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
    pub active_version_id: Option<Uuid>,
    pub latest_version_id: Uuid,
    pub latest_version: i32,
}

#[derive(Debug, Clone)]
pub struct CredentialSummaryRecord {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct ExecutionSummaryRecord {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub workflow_version_id: Uuid,
    pub trigger_type: String,
    pub trigger_node_id: String,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ExecutionStepRecord {
    pub id: Uuid,
    pub node_id: String,
    pub node_type: String,
    pub status: String,
    pub input: Value,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

impl Store {
    pub async fn list_workflows(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<WorkflowSummaryRecord>, StoreError> {
        let rows = sqlx::query_as::<_, (Uuid, String, bool, Option<Uuid>, Uuid, i32)>(
            r#"
            SELECT
                w.id,
                w.name,
                w.active,
                w.active_version_id,
                latest.id,
                latest.version
            FROM workflows AS w
            JOIN LATERAL (
                SELECT id, version
                FROM workflow_versions
                WHERE workflow_id = w.id
                ORDER BY version DESC
                LIMIT 1
            ) AS latest ON TRUE
            WHERE w.tenant_id = $1
            ORDER BY w.updated_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, name, active, active_version_id, latest_version_id, latest_version)| {
                    WorkflowSummaryRecord {
                        id,
                        name,
                        active,
                        active_version_id,
                        latest_version_id,
                        latest_version,
                    }
                },
            )
            .collect())
    }

    pub async fn workflow_summary(
        &self,
        tenant_id: Uuid,
        workflow_id: Uuid,
    ) -> Result<Option<WorkflowSummaryRecord>, StoreError> {
        let row = sqlx::query_as::<_, (Uuid, String, bool, Option<Uuid>, Uuid, i32)>(
            r#"
            SELECT
                w.id,
                w.name,
                w.active,
                w.active_version_id,
                latest.id,
                latest.version
            FROM workflows AS w
            JOIN LATERAL (
                SELECT id, version
                FROM workflow_versions
                WHERE workflow_id = w.id
                ORDER BY version DESC
                LIMIT 1
            ) AS latest ON TRUE
            WHERE w.tenant_id = $1
              AND w.id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(id, name, active, active_version_id, latest_version_id, latest_version)| {
                WorkflowSummaryRecord {
                    id,
                    name,
                    active,
                    active_version_id,
                    latest_version_id,
                    latest_version,
                }
            },
        ))
    }

    pub async fn list_credentials(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<CredentialSummaryRecord>, StoreError> {
        Ok(sqlx::query_as::<_, (Uuid, String, String)>(
            r#"
            SELECT id, name, kind
            FROM credentials
            WHERE tenant_id = $1
            ORDER BY name
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|(id, name, kind)| CredentialSummaryRecord { id, name, kind })
        .collect())
    }

    pub async fn list_executions(
        &self,
        tenant_id: Uuid,
        workflow_id: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<ExecutionSummaryRecord>, StoreError> {
        let rows = sqlx::query_as::<
            _,
            (Uuid, Uuid, Uuid, String, String, String, Option<String>, String, Option<i64>),
        >(
            r#"
            SELECT
                id,
                workflow_id,
                workflow_version_id,
                trigger_type,
                trigger_node_id,
                status,
                error,
                created_at::text,
                CASE
                    WHEN started_at IS NULL THEN NULL
                    ELSE (EXTRACT(EPOCH FROM (COALESCE(finished_at, now()) - started_at)) * 1000)::BIGINT
                END
            FROM executions
            WHERE tenant_id = $1
              AND ($2::uuid IS NULL OR workflow_id = $2)
            ORDER BY created_at DESC
            LIMIT $3
            "#,
        )
        .bind(tenant_id)
        .bind(workflow_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    workflow_id,
                    workflow_version_id,
                    trigger_type,
                    trigger_node_id,
                    status,
                    error,
                    created_at,
                    duration_ms,
                )| ExecutionSummaryRecord {
                    id,
                    workflow_id,
                    workflow_version_id,
                    trigger_type,
                    trigger_node_id,
                    status,
                    error,
                    created_at,
                    duration_ms,
                },
            )
            .collect())
    }

    pub async fn execution_steps(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
    ) -> Result<Vec<ExecutionStepRecord>, StoreError> {
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                String,
                Value,
                Option<Value>,
                Option<String>,
                Option<i64>,
                String,
                Option<String>,
            ),
        >(
            r#"
            SELECT
                s.id,
                s.node_id,
                s.node_type,
                s.status,
                s.input,
                s.output,
                s.error,
                s.duration_ms,
                s.started_at::text,
                s.finished_at::text
            FROM execution_steps AS s
            JOIN executions AS e
              ON e.id = s.execution_id
            WHERE s.execution_id = $1
              AND e.tenant_id = $2
            ORDER BY s.started_at
            "#,
        )
        .bind(execution_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    node_id,
                    node_type,
                    status,
                    input,
                    output,
                    error,
                    duration_ms,
                    started_at,
                    finished_at,
                )| ExecutionStepRecord {
                    id,
                    node_id,
                    node_type,
                    status,
                    input,
                    output,
                    error,
                    duration_ms,
                    started_at,
                    finished_at,
                },
            )
            .collect())
    }
}


#[derive(Debug, Clone)]
pub struct JobRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub workflow_id: Uuid,
    pub workflow_version_id: Uuid,
    pub trigger_node_id: String,
    pub run_at: DateTime<Utc>,
    pub attempts: i32,
}

impl Store {
    pub async fn active_schedule_workflows(
        &self,
    ) -> Result<Vec<WorkflowVersionRecord>, StoreError> {
        let rows = sqlx::query_as::<_, (Uuid, Uuid, Uuid, Json<WorkflowDefinition>)>(
            r#"
            SELECT w.tenant_id, w.id, v.id, v.definition
            FROM workflows AS w
            JOIN workflow_versions AS v
              ON v.id = w.active_version_id
             AND v.workflow_id = w.id
            WHERE w.active = TRUE
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(tenant_id, workflow_id, workflow_version_id, definition)| {
                    WorkflowVersionRecord {
                        tenant_id,
                        workflow_id,
                        workflow_version_id,
                        definition: definition.0,
                    }
                },
            )
            .collect())
    }

    pub async fn enqueue_job(
        &self,
        tenant_id: Uuid,
        workflow_id: Uuid,
        workflow_version_id: Uuid,
        trigger_node_id: &str,
        run_at: DateTime<Utc>,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            r#"
            INSERT INTO jobs (
                tenant_id,
                workflow_id,
                workflow_version_id,
                trigger_node_id,
                run_at
            )
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (
                workflow_id,
                workflow_version_id,
                trigger_node_id,
                run_at
            )
            DO NOTHING
            "#,
        )
        .bind(tenant_id)
        .bind(workflow_id)
        .bind(workflow_version_id)
        .bind(trigger_node_id)
        .bind(run_at)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn claim_job(&self) -> Result<Option<JobRecord>, StoreError> {
        let row = sqlx::query_as::<
            _,
            (Uuid, Uuid, Uuid, Uuid, String, DateTime<Utc>, i32),
        >(
            r#"
            UPDATE jobs
            SET status = 'running',
                attempts = attempts + 1
            WHERE id = (
                SELECT id
                FROM jobs
                WHERE status = 'queued'
                  AND run_at <= now()
                ORDER BY run_at, id
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            RETURNING
                id,
                tenant_id,
                workflow_id,
                workflow_version_id,
                trigger_node_id,
                run_at,
                attempts
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(
                id,
                tenant_id,
                workflow_id,
                workflow_version_id,
                trigger_node_id,
                run_at,
                attempts,
            )| JobRecord {
                id,
                tenant_id,
                workflow_id,
                workflow_version_id,
                trigger_node_id,
                run_at,
                attempts,
            },
        ))
    }

    pub async fn attach_job_execution(
        &self,
        job_id: Uuid,
        execution_id: Uuid,
    ) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET execution_id = $2
            WHERE id = $1
              AND status = 'running'
            "#,
        )
        .bind(job_id)
        .bind(execution_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn finish_job(
        &self,
        job_id: Uuid,
        succeeded: bool,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = CASE WHEN $2 THEN 'succeeded' ELSE 'failed' END,
                error = $3,
                finished_at = now()
            WHERE id = $1
              AND status = 'running'
            "#,
        )
        .bind(job_id)
        .bind(succeeded)
        .bind(error)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn recover_interrupted_work(&self) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            UPDATE execution_steps
            SET status = 'failed',
                error = COALESCE(error, 'server restarted during execution'),
                finished_at = now(),
                duration_ms = GREATEST(
                    0,
                    (EXTRACT(EPOCH FROM (now() - started_at)) * 1000)::BIGINT
                )
            WHERE status = 'running'
            "#,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE executions
            SET status = 'failed',
                error = COALESCE(error, 'server restarted during execution'),
                finished_at = now()
            WHERE status = 'running'
               OR (
                    status = 'queued'
                    AND id IN (
                        SELECT execution_id
                        FROM jobs
                        WHERE status = 'running'
                          AND execution_id IS NOT NULL
                    )
               )
            "#,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'queued',
                execution_id = NULL,
                error = 'requeued after server restart',
                finished_at = NULL
            WHERE status = 'running'
            "#,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }
}
