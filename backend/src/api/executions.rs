use crate::{api::authorize, error::AppError, state::AppState};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::types::Uuid;

#[derive(Deserialize)]
pub struct ListExecutionsQuery {
    #[serde(default)]
    pub workflow_id: Option<Uuid>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn list_executions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListExecutionsQuery>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    let executions = state
        .store
        .list_executions(tenant_id, query.workflow_id, limit)
        .await?;

    Ok(Json(Value::Array(
        executions
            .into_iter()
            .map(|execution| {
                json!({
                    "id": execution.id,
                    "workflow_id": execution.workflow_id,
                    "workflow_version_id": execution.workflow_version_id,
                    "trigger_type": execution.trigger_type,
                    "trigger_node_id": execution.trigger_node_id,
                    "status": execution.status,
                    "error": execution.error,
                    "created_at": execution.created_at,
                    "duration_ms": execution.duration_ms
                })
            })
            .collect(),
    )))
}

pub async fn get_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;
    let execution = state
        .store
        .execution(tenant_id, execution_id)
        .await?
        .ok_or_else(|| AppError::not_found("execution not found"))?;

    let steps = state.store.execution_steps(tenant_id, execution_id).await?;

    Ok(Json(json!({
        "id": execution.id,
        "workflow_id": execution.workflow_id,
        "workflow_version_id": execution.workflow_version_id,
        "trigger_type": execution.trigger_type,
        "trigger_node_id": execution.trigger_node_id,
        "status": execution.status,
        "input": execution.input,
        "output": execution.output,
        "error": execution.error,
        "retry_of_execution_id": execution.retry_of_execution_id,
        "started_at": execution.started_at,
        "finished_at": execution.finished_at,
        "duration_ms": execution.duration_ms,
        "steps": steps.into_iter().map(|step| json!({
            "id": step.id,
            "node_id": step.node_id,
            "node_type": step.node_type,
            "status": step.status,
            "input": step.input,
            "output": step.output,
            "error": step.error,
            "duration_ms": step.duration_ms,
            "metadata": step.metadata,
            "started_at": step.started_at,
            "finished_at": step.finished_at
        })).collect::<Vec<_>>()
    })))
}

pub async fn retry_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;
    let previous = state
        .store
        .execution_for_replay(tenant_id, execution_id)
        .await?
        .ok_or_else(|| AppError::not_found("execution not found"))?;

    if matches!(previous.status.as_str(), "queued" | "running") {
        return Err(AppError::conflict("execution is still active"));
    }

    let workflow = state
        .store
        .workflow_version(
            tenant_id,
            previous.workflow_id,
            previous.workflow_version_id,
        )
        .await?
        .ok_or_else(|| AppError::not_found("workflow version not found"))?;

    let retry = state
        .store
        .create_execution(
            tenant_id,
            previous.workflow_id,
            previous.workflow_version_id,
            &previous.trigger_type,
            &previous.trigger_node_id,
            &previous.input,
            None,
            Some(execution_id),
        )
        .await?;

    let output = crate::engine::run_execution(
        &state.store,
        &state.db,
        &state.http,
        state.credentials.as_ref(),
        tenant_id,
        retry.id,
        &workflow.definition,
        &previous.trigger_node_id,
        previous.input,
    )
    .await
    .map_err(|error| AppError::execution_failure(&error))?;

    Ok(Json(json!({
        "execution_id": retry.id,
        "retried_from": execution_id,
        "output": output
    })))
}

pub async fn cancel_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;

    if !state
        .store
        .cancel_execution(tenant_id, execution_id)
        .await?
    {
        return Err(AppError::conflict("execution is not queued or running"));
    }

    Ok(Json(json!({
        "execution_id": execution_id,
        "status": "cancelled"
    })))
}
