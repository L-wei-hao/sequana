use crate::{
    engine::model::NodeType, error::AppError, state::AppState, store::WorkflowVersionRecord,
};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Map, Value};
use sqlx::types::Uuid;
use std::str::FromStr;

pub async fn run_webhook_with_node(
    State(state): State<AppState>,
    Path((workflow_id, node_id)): Path<(String, String)>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let workflow_id = Uuid::parse_str(&workflow_id)
        .map_err(|_| AppError::bad_request("invalid workflow_id"))?;

    let workflow = state
        .store
        .active_workflow(workflow_id)
        .await?
        .ok_or_else(|| AppError::not_found("active workflow not found"))?;

    execute_webhook_request(state, workflow, Some(node_id), method, uri, headers, body).await
}

pub async fn run_webhook_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let workflow = state
        .store
        .active_workflow_by_slug(&slug)
        .await?
        .ok_or_else(|| AppError::not_found(format!("active workflow not found for path: {slug}")))?;

    execute_webhook_request(state, workflow, None, method, uri, headers, body).await
}

async fn execute_webhook_request(
    state: AppState,
    workflow: WorkflowVersionRecord,
    target_node_id: Option<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let webhook_node = if let Some(node_id) = &target_node_id {
        workflow
            .definition
            .nodes
            .iter()
            .find(|node| node.id == *node_id && node.node_type == NodeType::Webhook)
            .ok_or_else(|| AppError::not_found("webhook node not found"))?
    } else {
        workflow
            .definition
            .nodes
            .iter()
            .find(|node| node.node_type == NodeType::Webhook)
            .ok_or_else(|| AppError::bad_request("workflow contains no webhook trigger node"))?
    };

    let node_id = webhook_node.id.clone();

    let body_json = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&body).into_owned()))
    };

    let header_map: Map<String, Value> = headers
        .iter()
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|val| (k.to_string(), Value::String(val.to_string())))
        })
        .collect();

    let input = json!({
        "method": method.as_str(),
        "query": uri.query().unwrap_or(""),
        "path": uri.path(),
        "headers": header_map,
        "body": body_json
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
        return Err(AppError::bad_request("idempotency-key is too long"));
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
            None,
        )
        .await?;

    if !execution.created {
        let existing = state
            .store
            .execution(workflow.tenant_id, execution.id)
            .await?
            .ok_or_else(|| AppError::internal("could not retrieve existing execution"))?;

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

    let output = crate::engine::run_execution(
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
    .map_err(AppError::internal)?;

    Ok(response_from_output(output))
}

fn response_from_output(mut output: Value) -> Response {
    let status = output
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok())
        .and_then(|status| StatusCode::from_u16(status).ok())
        .unwrap_or(StatusCode::OK);

    let custom_headers: Vec<(HeaderName, HeaderValue)> = output
        .get("headers")
        .and_then(Value::as_object)
        .map(|headers_obj| {
            headers_obj
                .iter()
                .filter_map(|(k, v)| {
                    let v_str = v.as_str()?;
                    let hname = HeaderName::from_str(k).ok()?;
                    let hval = HeaderValue::from_str(v_str).ok()?;
                    Some((hname, hval))
                })
                .collect()
        })
        .unwrap_or_default();

    let body_value = match output {
        Value::Object(mut map) => map.remove("body").unwrap_or(Value::Object(map)),
        other => other,
    };

    let mut response = (status, Json(body_value)).into_response();
    let headers_mut = response.headers_mut();
    for (name, val) in custom_headers {
        headers_mut.insert(name, val);
    }

    response
}
