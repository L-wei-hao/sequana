use crate::engine::model::{ExecutionStatus, WorkflowDefinition};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{
    types::{Json, Uuid},
    PgPool,
};

#[derive(Debug)]
pub enum StoreError {
    InvalidTransition {
        from: ExecutionStatus,
        to: ExecutionStatus,
    },
    Database(sqlx::Error),
    NotFound(&'static str),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid status transition from {:?} to {:?}", from, to)
            }
            Self::Database(err) => write!(f, "database error: {}", err),
            Self::NotFound(item) => write!(f, "{} not found", item),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<sqlx::Error> for StoreError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

#[derive(Clone)]
pub struct Store {
    pool: PgPool,
}

#[derive(Debug, Clone, Copy)]
pub struct CreatedExecution {
    pub id: Uuid,
    pub created: bool,
}

#[derive(Debug, Clone)]
pub struct WorkflowSummaryRecord {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub active: bool,
    pub active_version_id: Option<Uuid>,
    pub latest_version_id: Uuid,
    pub latest_version: i32,
    pub updated_at: String,
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
    pub retry_of_execution_id: Option<Uuid>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
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
    pub metadata: Option<Value>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CredentialRecord {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub encrypted_value: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct CredentialSummaryRecord {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub created_at: String,
    pub updated_at: String,
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
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_workflow(
        &self,
        tenant_id: Uuid,
        name: &str,
        description: Option<&str>,
        definition: &WorkflowDefinition,
    ) -> Result<(Uuid, Uuid), StoreError> {
        let mut tx = self.pool.begin().await?;

        let workflow_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO workflows (tenant_id, name, description)
            VALUES ($1, $2, $3)
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(name)
        .bind(description)
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

    pub async fn update_workflow(
        &self,
        tenant_id: Uuid,
        workflow_id: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            r#"
            UPDATE workflows
            SET name = $3,
                description = COALESCE($4, description),
                updated_at = now()
            WHERE id = $2
              AND tenant_id = $1
            "#,
        )
        .bind(tenant_id)
        .bind(workflow_id)
        .bind(name)
        .bind(description)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn delete_workflow(
        &self,
        tenant_id: Uuid,
        workflow_id: Uuid,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            r#"
            DELETE FROM workflows
            WHERE id = $2
              AND tenant_id = $1
            "#,
        )
        .bind(tenant_id)
        .bind(workflow_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn activate_workflow(
        &self,
        tenant_id: Uuid,
        workflow_id: Uuid,
        version_id: Uuid,
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
        .bind(version_id)
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
        .bind(version_id)
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

    pub async fn list_workflows(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<WorkflowSummaryRecord>, StoreError> {
        let rows = sqlx::query_as::<
            _,
            (Uuid, String, Option<String>, bool, Option<Uuid>, Uuid, i32, String),
        >(
            r#"
            SELECT
                w.id,
                w.name,
                w.description,
                w.active,
                w.active_version_id,
                latest.id,
                latest.version,
                w.updated_at::text
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
                |(
                    id,
                    name,
                    description,
                    active,
                    active_version_id,
                    latest_version_id,
                    latest_version,
                    updated_at,
                )| {
                    WorkflowSummaryRecord {
                        id,
                        name,
                        description,
                        active,
                        active_version_id,
                        latest_version_id,
                        latest_version,
                        updated_at,
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
        let row = sqlx::query_as::<
            _,
            (Uuid, String, Option<String>, bool, Option<Uuid>, Uuid, i32, String),
        >(
            r#"
            SELECT
                w.id,
                w.name,
                w.description,
                w.active,
                w.active_version_id,
                latest.id,
                latest.version,
                w.updated_at::text
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
            |(
                id,
                name,
                description,
                active,
                active_version_id,
                latest_version_id,
                latest_version,
                updated_at,
            )| {
                WorkflowSummaryRecord {
                    id,
                    name,
                    description,
                    active,
                    active_version_id,
                    latest_version_id,
                    latest_version,
                    updated_at,
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

    pub async fn active_workflow_by_slug(
        &self,
        slug: &str,
    ) -> Result<Option<WorkflowVersionRecord>, StoreError> {
        if let Ok(id) = Uuid::parse_str(slug) {
            if let Some(wf) = self.active_workflow(id).await? {
                return Ok(Some(wf));
            }
        }

        let rows = sqlx::query_as::<_, (Uuid, Uuid, Uuid, Json<WorkflowDefinition>, String)>(
            r#"
            SELECT w.tenant_id, w.id, v.id, v.definition, w.name
            FROM workflows AS w
            JOIN workflow_versions AS v
              ON v.id = w.active_version_id
             AND v.workflow_id = w.id
            WHERE w.active = TRUE
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        for (tenant_id, workflow_id, workflow_version_id, def, name) in rows {
            for node in &def.0.nodes {
                if node.node_type == crate::engine::model::NodeType::Webhook {
                    if let Some(path) = node.config.get("path").and_then(Value::as_str) {
                        if path.trim_start_matches('/') == slug.trim_start_matches('/') {
                            return Ok(Some(WorkflowVersionRecord {
                                tenant_id,
                                workflow_id,
                                workflow_version_id,
                                definition: def.0,
                            }));
                        }
                    }
                }
            }

            let normalized_name = name.to_lowercase().replace(' ', "-");
            if normalized_name == slug.to_lowercase() {
                return Ok(Some(WorkflowVersionRecord {
                    tenant_id,
                    workflow_id,
                    workflow_version_id,
                    definition: def.0,
                }));
            }
        }

        Ok(None)
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

    pub async fn create_execution(
        &self,
        tenant_id: Uuid,
        workflow_id: Uuid,
        workflow_version_id: Uuid,
        trigger_type: &str,
        trigger_node_id: &str,
        input: &Value,
        idempotency_key: Option<&str>,
        retry_of: Option<Uuid>,
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
                idempotency_key,
                retry_of_execution_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
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
        .bind(retry_of)
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
        metadata: Option<&Value>,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            r#"
            UPDATE execution_steps
            SET status = CASE WHEN $3::text IS NULL THEN 'succeeded' ELSE 'failed' END,
                output = $2,
                error = $3,
                metadata = $4,
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
        .bind(metadata.cloned())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
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
                Option<Uuid>,
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
                retry_of_execution_id,
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
                retry_of_execution_id,
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
                retry_of_execution_id,
                started_at,
                finished_at,
                duration_ms,
            },
        ))
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
                Option<Value>,
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
                s.metadata,
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
                    metadata,
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
                    metadata,
                    started_at,
                    finished_at,
                },
            )
            .collect())
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

    pub async fn update_credential(
        &self,
        tenant_id: Uuid,
        credential_id: Uuid,
        name: &str,
        encrypted_value: &[u8],
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            r#"
            UPDATE credentials
            SET name = $3,
                encrypted_value = $4,
                updated_at = now()
            WHERE id = $1
              AND tenant_id = $2
            "#,
        )
        .bind(credential_id)
        .bind(tenant_id)
        .bind(name)
        .bind(encrypted_value)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn delete_credential(
        &self,
        tenant_id: Uuid,
        credential_id: Uuid,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            r#"
            DELETE FROM credentials
            WHERE id = $1
              AND tenant_id = $2
            "#,
        )
        .bind(credential_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn list_credentials(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<CredentialSummaryRecord>, StoreError> {
        let rows = sqlx::query_as::<_, (Uuid, String, String, String, String)>(
            r#"
            SELECT id, name, kind, created_at::text, updated_at::text
            FROM credentials
            WHERE tenant_id = $1
            ORDER BY name
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, name, kind, created_at, updated_at)| {
                CredentialSummaryRecord {
                    id,
                    name,
                    kind,
                    created_at,
                    updated_at,
                }
            })
            .collect())
    }

    pub async fn credential(
        &self,
        tenant_id: Uuid,
        credential_id: Uuid,
    ) -> Result<Option<CredentialRecord>, StoreError> {
        let row = sqlx::query_as::<_, (Uuid, String, String, Vec<u8>)>(
            r#"
            SELECT id, name, kind, encrypted_value
            FROM credentials
            WHERE id = $1
              AND tenant_id = $2
            "#,
        )
        .bind(credential_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(id, name, kind, encrypted_value)| CredentialRecord {
            id,
            name,
            kind,
            encrypted_value,
        }))
    }

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
