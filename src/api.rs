use crate::{
    credentials::CredentialCipher,
    nodes::{execute_node, NodeContext},
    runner::run_execution,
    store::{ExecutionRecord, Store, WorkflowVersionRecord},
    workflow::{Node, NodeType, WorkflowDefinition},
};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header::AUTHORIZATION, HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{types::Uuid, PgPool};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub store: Store,
    pub http: Client,
    pub credentials: Arc<CredentialCipher>,
    pub admin_token: Arc<str>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/workflows", get(list_workflows).post(create_workflow))
        .route("/api/workflows/{workflow_id}", get(get_workflow))
        .route("/api/workflows/{workflow_id}/versions", post(save_workflow_version))
        .route(
            "/api/workflows/{workflow_id}/activate/{version_id}",
            post(activate_workflow),
        )
        .route(
            "/api/workflows/{workflow_id}/run/{node_id}",
            post(run_manual),
        )
        .route("/api/credentials", get(list_credentials).post(create_credential))
        .route("/api/nodes/test", post(test_node))
        .route("/api/executions", get(list_executions))
        .route("/api/executions/{execution_id}", get(get_execution))
        .route("/api/executions/{execution_id}/retry", post(retry_execution))
        .route("/api/executions/{execution_id}/cancel", post(cancel_execution))
        .route(
            "/webhook/{workflow_id}/{node_id}",
            post(run_webhook).put(run_webhook).patch(run_webhook),
        )
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Response {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({
                "service": "sequana",
                "status": "ok",
                "database": "ok"
            })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "service": "sequana",
                "status": "degraded",
                "database": "unavailable"
            })),
        )
            .into_response(),
    }
}

async fn list_workflows(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = authorize(&headers, &state)?;
    let workflows = state
        .store
        .list_workflows(tenant_id)
        .await
        .map_err(|_| ApiError::internal())?;

    Ok(Json(Value::Array(
        workflows
            .into_iter()
            .map(|workflow| {
                json!({
                    "id": workflow.id,
                    "name": workflow.name,
                    "active": workflow.active,
                    "active_version_id": workflow.active_version_id,
                    "latest_version_id": workflow.latest_version_id,
                    "latest_version": workflow.latest_version
                })
            })
            .collect(),
    )))
}

async fn get_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workflow_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = authorize(&headers, &state)?;
    let workflow_id = parse_uuid(&workflow_id, "workflow_id")?;

    let summary = state
        .store
        .workflow_summary(tenant_id, workflow_id)
        .await
        .map_err(|_| ApiError::internal())?
        .ok_or_else(|| ApiError::not_found("workflow not found"))?;
    let workflow = state
        .store
        .latest_workflow(tenant_id, workflow_id)
        .await
        .map_err(|_| ApiError::internal())?
        .ok_or_else(|| ApiError::not_found("workflow not found"))?;

    Ok(Json(json!({
        "id": summary.id,
        "name": summary.name,
        "active": summary.active,
        "active_version_id": summary.active_version_id,
        "latest_version_id": summary.latest_version_id,
        "latest_version": summary.latest_version,
        "definition": workflow.definition
    })))
}

#[derive(Deserialize)]
struct CreateWorkflowRequest {
    name: String,
    definition: WorkflowDefinition,
}

async fn create_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateWorkflowRequest>,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = authorize(&headers, &state)?;

    if request.name.trim().is_empty() {
        return Err(ApiError::bad_request("workflow name cannot be empty"));
    }
    request
        .definition
        .validate()
        .map_err(ApiError::bad_request)?;

    let (workflow_id, version_id) = state
        .store
        .create_workflow(tenant_id, request.name.trim(), &request.definition)
        .await
        .map_err(|_| ApiError::internal())?;

    Ok(Json(json!({
        "workflow_id": workflow_id,
        "version_id": version_id
    })))
}

async fn save_workflow_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workflow_id): Path<String>,
    Json(definition): Json<WorkflowDefinition>,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = authorize(&headers, &state)?;
    let workflow_id = parse_uuid(&workflow_id, "workflow_id")?;
    definition.validate().map_err(ApiError::bad_request)?;

    let version_id = state
        .store
        .save_workflow_version(tenant_id, workflow_id, &definition)
        .await
        .map_err(|error| match error {
            crate::store::StoreError::NotFound(_) => ApiError::not_found("workflow not found"),
            _ => ApiError::internal(),
        })?;

    Ok(Json(json!({ "version_id": version_id })))
}

async fn activate_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((workflow_id, version_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = authorize(&headers, &state)?;
    let workflow_id = parse_uuid(&workflow_id, "workflow_id")?;
    let version_id = parse_uuid(&version_id, "version_id")?;

    if !state
        .store
        .activate_workflow(tenant_id, workflow_id, version_id)
        .await
        .map_err(|_| ApiError::internal())?
    {
        return Err(ApiError::not_found("workflow version not found"));
    }

    Ok(Json(json!({ "active": true })))
}

async fn run_manual(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((workflow_id, node_id)): Path<(String, String)>,
    Json(input): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = authorize(&headers, &state)?;
    let workflow_id = parse_uuid(&workflow_id, "workflow_id")?;

    let workflow = state
        .store
        .latest_workflow(tenant_id, workflow_id)
        .await
        .map_err(|_| ApiError::internal())?
        .ok_or_else(|| ApiError::not_found("workflow not found"))?;

    ensure_trigger(&workflow, &node_id, NodeType::ManualTrigger)?;

    let execution = state
        .store
        .create_execution(
            tenant_id,
            workflow.workflow_id,
            workflow.workflow_version_id,
            "manual",
            &node_id,
            &input,
            None,
        )
        .await
        .map_err(|_| ApiError::internal())?;

    let output = run_execution(
        &state.store,
        &state.db,
        &state.http,
        state.credentials.as_ref(),
        tenant_id,
        execution.id,
        &workflow.definition,
        &node_id,
        input,
    )
    .await
    .map_err(|error| ApiError(StatusCode::INTERNAL_SERVER_ERROR, error))?;

    Ok(Json(json!({
        "execution_id": execution.id,
        "output": output
    })))
}

async fn list_credentials(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = authorize(&headers, &state)?;
    let credentials = state
        .store
        .list_credentials(tenant_id)
        .await
        .map_err(|_| ApiError::internal())?;

    Ok(Json(Value::Array(
        credentials
            .into_iter()
            .map(|credential| {
                json!({
                    "id": credential.id,
                    "name": credential.name,
                    "kind": credential.kind
                })
            })
            .collect(),
    )))
}

#[derive(Deserialize)]
struct TestNodeRequest {
    node: Node,
    #[serde(default)]
    input: Value,
}

async fn test_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TestNodeRequest>,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = authorize(&headers, &state)?;
    let context = NodeContext {
        db: &state.db,
        http: &state.http,
        store: &state.store,
        tenant_id,
        credentials: state.credentials.as_ref(),
    };

    let result = execute_node(&request.node, &request.input, &context)
        .await
        .map_err(|error| ApiError(StatusCode::BAD_REQUEST, error))?;

    Ok(Json(json!({
        "output": result.output,
        "route": result.route
    })))
}

#[derive(Deserialize)]
struct CreateCredentialRequest {
    name: String,
    kind: String,
    value: String,
}

async fn create_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateCredentialRequest>,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = authorize(&headers, &state)?;

    if request.name.trim().is_empty() || request.value.is_empty() {
        return Err(ApiError::bad_request(
            "credential name and value cannot be empty",
        ));
    }
    if request.kind != "openai" {
        return Err(ApiError::bad_request(
            "only openai credentials are supported in V1",
        ));
    }

    let encrypted = state
        .credentials
        .encrypt(tenant_id, &request.value)
        .map_err(|_| ApiError::internal())?;
    let credential_id = state
        .store
        .create_credential(tenant_id, request.name.trim(), &request.kind, &encrypted)
        .await
        .map_err(|_| ApiError::internal())?;

    Ok(Json(json!({ "credential_id": credential_id })))
}

#[derive(Deserialize)]
struct ListExecutionsQuery {
    #[serde(default)]
    workflow_id: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
}

async fn list_executions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListExecutionsQuery>,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = authorize(&headers, &state)?;
    let workflow_id = query
        .workflow_id
        .as_deref()
        .map(|value| parse_uuid(value, "workflow_id"))
        .transpose()?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    let executions = state
        .store
        .list_executions(tenant_id, workflow_id, limit)
        .await
        .map_err(|_| ApiError::internal())?;

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

async fn get_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(execution_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = authorize(&headers, &state)?;
    let execution_id = parse_uuid(&execution_id, "execution_id")?;
    let execution = state
        .store
        .execution(tenant_id, execution_id)
        .await
        .map_err(|_| ApiError::internal())?
        .ok_or_else(|| ApiError::not_found("execution not found"))?;
    let steps = state
        .store
        .execution_steps(tenant_id, execution_id)
        .await
        .map_err(|_| ApiError::internal())?;

    let mut payload = execution_json(&execution);
    payload
        .as_object_mut()
        .expect("execution_json returns an object")
        .insert(
            "steps".into(),
            Value::Array(
                steps
                    .into_iter()
                    .map(|step| {
                        json!({
                            "id": step.id,
                            "node_id": step.node_id,
                            "node_type": step.node_type,
                            "status": step.status,
                            "input": step.input,
                            "output": step.output,
                            "error": step.error,
                            "duration_ms": step.duration_ms,
                            "started_at": step.started_at,
                            "finished_at": step.finished_at
                        })
                    })
                    .collect(),
            ),
        );

    Ok(Json(payload))
}

async fn retry_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(execution_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = authorize(&headers, &state)?;
    let execution_id = parse_uuid(&execution_id, "execution_id")?;
    let previous = state
        .store
        .execution(tenant_id, execution_id)
        .await
        .map_err(|_| ApiError::internal())?
        .ok_or_else(|| ApiError::not_found("execution not found"))?;

    if matches!(previous.status.as_str(), "queued" | "running") {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "execution is still active".into(),
        ));
    }

    let workflow = state
        .store
        .workflow_version(
            tenant_id,
            previous.workflow_id,
            previous.workflow_version_id,
        )
        .await
        .map_err(|_| ApiError::internal())?
        .ok_or_else(|| ApiError::not_found("workflow version not found"))?;

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
        )
        .await
        .map_err(|_| ApiError::internal())?;

    let output = run_execution(
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
    .map_err(|error| ApiError(StatusCode::INTERNAL_SERVER_ERROR, error))?;

    Ok(Json(json!({
        "execution_id": retry.id,
        "retried_from": execution_id,
        "output": output
    })))
}

async fn cancel_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(execution_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = authorize(&headers, &state)?;
    let execution_id = parse_uuid(&execution_id, "execution_id")?;

    if !state
        .store
        .cancel_execution(tenant_id, execution_id)
        .await
        .map_err(|_| ApiError::internal())?
    {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "execution is not queued or running".into(),
        ));
    }

    Ok(Json(json!({
        "execution_id": execution_id,
        "status": "cancelled"
    })))
}

async fn run_webhook(
    State(state): State<AppState>,
    Path((workflow_id, node_id)): Path<(String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let workflow_id = parse_uuid(&workflow_id, "workflow_id")?;
    let workflow = state
        .store
        .active_workflow(workflow_id)
        .await
        .map_err(|_| ApiError::internal())?
        .ok_or_else(|| ApiError::not_found("active workflow not found"))?;

    ensure_trigger(&workflow, &node_id, NodeType::Webhook)?;

    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&body).into_owned()))
    };

    let input = json!({
        "method": method.as_str(),
        "query": uri.query().unwrap_or(""),
        "body": body
    });

    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(|value| format!("{node_id}:{value}"));

    if idempotency_key
        .as_ref()
        .is_some_and(|key| key.len() > 512)
    {
        return Err(ApiError::bad_request("idempotency-key is too long"));
    }

    let execution = state
        .store
        .create_execution(
            workflow.tenant_id,
            workflow.workflow_id,
            workflow.workflow_version_id,
            "webhook",
            &node_id,
            &input,
            idempotency_key.as_deref(),
        )
        .await
        .map_err(|_| ApiError::internal())?;

    if !execution.created {
        let existing = state
            .store
            .execution(workflow.tenant_id, execution.id)
            .await
            .map_err(|_| ApiError::internal())?
            .ok_or_else(ApiError::internal)?;

        return Ok(match existing.status.as_str() {
            "succeeded" => existing
                .output
                .map(response_from_output)
                .unwrap_or_else(|| StatusCode::NO_CONTENT.into_response()),
            "queued" | "running" => (
                StatusCode::ACCEPTED,
                Json(json!({
                    "execution_id": existing.id,
                    "status": existing.status
                })),
            )
                .into_response(),
            _ => (
                StatusCode::CONFLICT,
                Json(json!({
                    "execution_id": existing.id,
                    "status": existing.status,
                    "error": existing.error
                })),
            )
                .into_response(),
        });
    }

    let output = run_execution(
        &state.store,
        &state.db,
        &state.http,
        state.credentials.as_ref(),
        workflow.tenant_id,
        execution.id,
        &workflow.definition,
        &node_id,
        input,
    )
    .await
    .map_err(|error| ApiError(StatusCode::INTERNAL_SERVER_ERROR, error))?;

    Ok(response_from_output(output))
}

fn ensure_trigger(
    workflow: &WorkflowVersionRecord,
    node_id: &str,
    expected: NodeType,
) -> Result<(), ApiError> {
    let node = workflow
        .definition
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .ok_or_else(|| ApiError::not_found("trigger node not found"))?;

    if node.node_type != expected {
        return Err(ApiError::bad_request("node is not the requested trigger type"));
    }

    Ok(())
}

fn response_from_output(output: Value) -> Response {
    let status = output
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok())
        .and_then(|status| StatusCode::from_u16(status).ok())
        .unwrap_or(StatusCode::OK);

    let body = output.get("body").cloned().unwrap_or(output);
    (status, Json(body)).into_response()
}

fn execution_json(execution: &ExecutionRecord) -> Value {
    json!({
        "id": execution.id,
        "workflow_id": execution.workflow_id,
        "workflow_version_id": execution.workflow_version_id,
        "trigger_type": execution.trigger_type,
        "trigger_node_id": execution.trigger_node_id,
        "status": execution.status,
        "input": execution.input,
        "output": execution.output,
        "error": execution.error,
        "started_at": execution.started_at,
        "finished_at": execution.finished_at,
        "duration_ms": execution.duration_ms
    })
}

fn authorize(headers: &HeaderMap, state: &AppState) -> Result<Uuid, ApiError> {
    let bearer = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "missing bearer token".into()))?;

    if !constant_time_eq(bearer.as_bytes(), state.admin_token.as_bytes()) {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "invalid bearer token".into(),
        ));
    }

    let tenant_id = headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::bad_request("missing x-tenant-id"))?;

    parse_uuid(tenant_id, "x-tenant-id")
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter()
        .zip(right)
        .fold(0u8, |diff, (left, right)| diff | (left ^ right))
        == 0
}

fn parse_uuid(value: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value)
        .map_err(|_| ApiError::bad_request(format!("invalid {field}")))
}

struct ApiError(StatusCode, String);

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, message.into())
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self(StatusCode::NOT_FOUND, message.into())
    }

    fn internal() -> Self {
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal server error".into(),
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_compare_checks_all_bytes() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secrex"));
        assert!(!constant_time_eq(b"secret", b"short"));
    }
}
