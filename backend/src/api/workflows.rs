use crate::{
    api::authorize,
    engine::{model::NodeType, validate_workflow, WorkflowDefinition},
    error::AppError,
    nodes::{execute_node, NodeContext},
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::types::Uuid;

pub async fn list_workflows(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;
    let workflows = state.store.list_workflows(tenant_id).await?;

    Ok(Json(Value::Array(
        workflows
            .into_iter()
            .map(|workflow| {
                json!({
                    "id": workflow.id,
                    "name": workflow.name,
                    "description": workflow.description,
                    "active": workflow.active,
                    "active_version_id": workflow.active_version_id,
                    "latest_version_id": workflow.latest_version_id,
                    "latest_version": workflow.latest_version,
                    "updated_at": workflow.updated_at
                })
            })
            .collect(),
    )))
}

pub async fn get_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workflow_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;

    let summary = state
        .store
        .workflow_summary(tenant_id, workflow_id)
        .await?
        .ok_or_else(|| AppError::not_found("workflow not found"))?;

    let workflow = state
        .store
        .latest_workflow(tenant_id, workflow_id)
        .await?
        .ok_or_else(|| AppError::not_found("workflow definition not found"))?;

    Ok(Json(json!({
        "id": summary.id,
        "name": summary.name,
        "description": summary.description,
        "active": summary.active,
        "active_version_id": summary.active_version_id,
        "latest_version_id": summary.latest_version_id,
        "latest_version": summary.latest_version,
        "definition": workflow.definition
    })))
}

#[derive(Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub definition: WorkflowDefinition,
}

pub async fn create_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateWorkflowRequest>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;

    if request.name.trim().is_empty() {
        return Err(AppError::bad_request("workflow name cannot be empty"));
    }
    validate_workflow(&request.definition).map_err(AppError::bad_request)?;

    let (workflow_id, version_id) = state
        .store
        .create_workflow(
            tenant_id,
            request.name.trim(),
            request.description.as_deref(),
            &request.definition,
        )
        .await?;

    Ok(Json(json!({
        "workflow_id": workflow_id,
        "version_id": version_id
    })))
}

#[derive(Deserialize)]
pub struct UpdateWorkflowRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

pub async fn update_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workflow_id): Path<Uuid>,
    Json(request): Json<UpdateWorkflowRequest>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;

    if request.name.trim().is_empty() {
        return Err(AppError::bad_request("workflow name cannot be empty"));
    }

    let updated = state
        .store
        .update_workflow(
            tenant_id,
            workflow_id,
            request.name.trim(),
            request.description.as_deref(),
        )
        .await?;

    if !updated {
        return Err(AppError::not_found("workflow not found"));
    }

    Ok(Json(json!({ "updated": true })))
}

pub async fn delete_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workflow_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;

    let deleted = state.store.delete_workflow(tenant_id, workflow_id).await?;
    if !deleted {
        return Err(AppError::not_found("workflow not found"));
    }

    Ok(Json(json!({ "deleted": true })))
}

pub async fn save_workflow_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workflow_id): Path<Uuid>,
    Json(definition): Json<WorkflowDefinition>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;
    validate_workflow(&definition).map_err(AppError::bad_request)?;

    let version_id = state
        .store
        .save_workflow_version(tenant_id, workflow_id, &definition)
        .await
        .map_err(|error| match error {
            crate::store::StoreError::NotFound(_) => AppError::not_found("workflow not found"),
            err => AppError::from(err),
        })?;

    Ok(Json(json!({ "version_id": version_id })))
}

pub async fn activate_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((workflow_id, version_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;

    if !state
        .store
        .activate_workflow(tenant_id, workflow_id, version_id)
        .await?
    {
        return Err(AppError::not_found("workflow version not found"));
    }

    Ok(Json(json!({ "active": true })))
}

pub async fn deactivate_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workflow_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;

    if !state
        .store
        .deactivate_workflow(tenant_id, workflow_id)
        .await?
    {
        return Err(AppError::not_found("workflow not found"));
    }

    Ok(Json(json!({ "active": false })))
}

pub async fn run_manual(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((workflow_id, node_id)): Path<(Uuid, String)>,
    Json(input): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;

    let workflow = state
        .store
        .latest_workflow(tenant_id, workflow_id)
        .await?
        .ok_or_else(|| AppError::not_found("workflow not found"))?;

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
            None,
        )
        .await?;

    let output = crate::engine::run_execution(
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
    .map_err(|error| AppError::execution_failure(&error))?;

    Ok(Json(json!({
        "execution_id": execution.id,
        "output": output
    })))
}

#[derive(Deserialize)]
pub struct TestNodeRequest {
    pub node: crate::engine::model::Node,
    #[serde(default)]
    pub input: Value,
}

pub async fn test_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TestNodeRequest>,
) -> Result<Json<Value>, AppError> {
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
        .map_err(|error| AppError::node_test_failure(&error))?;

    Ok(Json(json!({
        "output": result.output,
        "route": result.route,
        "metadata": result.metadata
    })))
}

fn ensure_trigger(
    workflow: &crate::store::WorkflowVersionRecord,
    node_id: &str,
    expected: NodeType,
) -> Result<(), AppError> {
    let node = workflow
        .definition
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .ok_or_else(|| AppError::not_found("trigger node not found"))?;

    if node.node_type != expected {
        return Err(AppError::bad_request(
            "node is not the requested trigger type",
        ));
    }

    Ok(())
}
