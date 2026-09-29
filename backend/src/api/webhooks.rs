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
    let workflow_id =
        Uuid::parse_str(&workflow_id).map_err(|_| AppError::bad_request("invalid workflow_id"))?;

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
        .ok_or_else(|| {
            AppError::not_found(format!("active workflow not found for path: {slug}"))
        })?;

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

    let idempotency_key = webhook_idempotency_key(&headers, &node_id)?;
    let header_map = workflow_header_map(&headers, idempotency_key.as_ref());

    let input = json!({
        "method": method.as_str(),
        "query": uri.query().unwrap_or(""),
        "path": uri.path(),
        "headers": header_map,
        "body": body_json
    });

    let execution = state
        .store
        .create_execution(
            workflow.tenant_id,
            workflow.workflow_id,
            workflow.workflow_version_id,
            "webhook",
            &node_id,
            &input,
            idempotency_key
                .as_ref()
                .map(|idempotency_key| idempotency_key.storage_key.as_str()),
            None,
        )
        .await?;

    if !execution.created {
        let existing = state
            .store
            .execution_for_replay(workflow.tenant_id, execution.id)
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
    .map_err(|error| AppError::execution_failure(&error))?;

    Ok(response_from_output(output))
}

struct WebhookIdempotencyKey {
    storage_key: String,
    header_value: String,
}

fn workflow_header_map(
    headers: &HeaderMap,
    idempotency_key: Option<&WebhookIdempotencyKey>,
) -> Map<String, Value> {
    let mut header_map: Map<String, Value> = headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.to_string(), Value::String(value.to_owned())))
        })
        .collect();

    if let Some(idempotency_key) = idempotency_key {
        header_map.retain(|name, _| {
            !name.eq_ignore_ascii_case("idempotency-key")
                && !name.eq_ignore_ascii_case("x-idempotency-key")
        });
        header_map.insert(
            "idempotency-key".to_owned(),
            Value::String(idempotency_key.header_value.clone()),
        );
    }

    header_map
}

fn webhook_idempotency_key(
    headers: &HeaderMap,
    node_id: &str,
) -> Result<Option<WebhookIdempotencyKey>, AppError> {
    fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, AppError> {
        let mut values = headers.get_all(name).iter();
        let Some(value) = values.next() else {
            return Ok(None);
        };
        if values.next().is_some() {
            return Err(AppError::bad_request(
                "idempotency-key header must appear only once",
            ));
        }
        value
            .to_str()
            .map(Some)
            .map_err(|_| AppError::bad_request("idempotency-key must be valid text"))
    }

    let standard = header_value(headers, "idempotency-key")?;
    let prefixed = header_value(headers, "x-idempotency-key")?;
    if standard.is_some() && prefixed.is_some() && standard != prefixed {
        return Err(AppError::bad_request("conflicting idempotency-key headers"));
    }

    let Some(value) = standard.or(prefixed).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let key = format!("{node_id}:{value}");
    if key.len() > 512 {
        return Err(AppError::bad_request("idempotency-key is too long"));
    }
    Ok(Some(WebhookIdempotencyKey {
        storage_key: key,
        header_value: value.to_owned(),
    }))
}

fn response_from_output(output: Value) -> Response {
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

#[cfg(test)]
mod idempotency_header_tests {
    use super::*;

    #[test]
    fn supports_both_idempotency_header_names() {
        let mut headers = HeaderMap::new();
        headers.insert("x-idempotency-key", HeaderValue::from_static("payload-key"));
        let key = webhook_idempotency_key(&headers, "start").unwrap().unwrap();
        assert_eq!(key.storage_key, "start:payload-key");
        assert_eq!(key.header_value, "payload-key");
    }

    #[test]
    fn rejects_conflicting_idempotency_header_values() {
        let mut headers = HeaderMap::new();
        headers.insert("idempotency-key", HeaderValue::from_static("first"));
        headers.insert("x-idempotency-key", HeaderValue::from_static("second"));
        let error = webhook_idempotency_key(&headers, "start").err().unwrap();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn rejects_repeated_idempotency_header_values() {
        let mut headers = HeaderMap::new();
        headers.append("idempotency-key", HeaderValue::from_static("first"));
        headers.append("idempotency-key", HeaderValue::from_static("second"));
        let error = webhook_idempotency_key(&headers, "start").err().unwrap();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn equivalent_aliases_produce_identical_workflow_headers() {
        let mut standard = HeaderMap::new();
        standard.insert("idempotency-key", HeaderValue::from_static("payload-key"));
        let standard_key = webhook_idempotency_key(&standard, "start")
            .unwrap()
            .unwrap();

        let mut prefixed = HeaderMap::new();
        prefixed.insert("x-idempotency-key", HeaderValue::from_static("payload-key"));
        let prefixed_key = webhook_idempotency_key(&prefixed, "start")
            .unwrap()
            .unwrap();

        let standard_input = workflow_header_map(&standard, Some(&standard_key));
        let prefixed_input = workflow_header_map(&prefixed, Some(&prefixed_key));
        assert_eq!(standard_input, prefixed_input);
        assert_eq!(standard_input["idempotency-key"], "payload-key");
        assert!(!standard_input.contains_key("x-idempotency-key"));
    }
}
