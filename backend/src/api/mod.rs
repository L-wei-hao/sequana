pub mod credentials;
pub mod executions;
pub mod webhooks;
pub mod workflows;

use crate::{error::AppError, state::AppState};
use axum::{
    extract::Request,
    http::{
        header::{AUTHORIZATION, RETRY_AFTER},
        HeaderMap, HeaderValue, StatusCode,
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde_json::json;
use sqlx::types::Uuid;

pub fn router(state: AppState) -> Router {
    let execution_routes = Router::new()
        .route(
            "/api/workflows/{workflow_id}/run/{node_id}",
            post(workflows::run_manual),
        )
        .route("/api/nodes/test", post(workflows::test_node))
        .route(
            "/api/executions/{execution_id}/retry",
            post(executions::retry_execution),
        )
        .route(
            "/webhook/{workflow_id}/{node_id}",
            get(webhooks::run_webhook_with_node)
                .post(webhooks::run_webhook_with_node)
                .put(webhooks::run_webhook_with_node)
                .patch(webhooks::run_webhook_with_node)
                .delete(webhooks::run_webhook_with_node),
        )
        .route(
            "/webhook/{slug}",
            get(webhooks::run_webhook_slug)
                .post(webhooks::run_webhook_slug)
                .put(webhooks::run_webhook_slug)
                .patch(webhooks::run_webhook_slug)
                .delete(webhooks::run_webhook_slug),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            enforce_execution_admission,
        ));

    Router::new()
        .route("/health", get(health))
        .route(
            "/api/workflows",
            get(workflows::list_workflows).post(workflows::create_workflow),
        )
        .route(
            "/api/workflows/{workflow_id}",
            get(workflows::get_workflow)
                .put(workflows::update_workflow)
                .delete(workflows::delete_workflow),
        )
        .route(
            "/api/workflows/{workflow_id}/versions",
            post(workflows::save_workflow_version),
        )
        .route(
            "/api/workflows/{workflow_id}/activate/{version_id}",
            post(workflows::activate_workflow),
        )
        .route(
            "/api/workflows/{workflow_id}/deactivate",
            post(workflows::deactivate_workflow),
        )
        .route("/api/executions", get(executions::list_executions))
        .route(
            "/api/executions/{execution_id}",
            get(executions::get_execution),
        )
        .route(
            "/api/executions/{execution_id}/cancel",
            post(executions::cancel_execution),
        )
        .route(
            "/api/credentials",
            get(credentials::list_credentials).post(credentials::create_credential),
        )
        .route(
            "/api/credentials/{credential_id}",
            put(credentials::update_credential).delete(credentials::delete_credential),
        )
        .merge(execution_routes)
        .with_state(state)
}

async fn enforce_execution_admission(
    axum::extract::State(state): axum::extract::State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let Ok(_permit) = state.execution_admission.clone().try_acquire_owned() else {
        let mut response = (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "execution capacity exhausted" })),
        )
            .into_response();
        response
            .headers_mut()
            .insert(RETRY_AFTER, HeaderValue::from_static("1"));
        return response;
    };

    next.run(request).await
}

async fn health(axum::extract::State(state): axum::extract::State<AppState>) -> Response {
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

pub fn authorize(headers: &HeaderMap, state: &AppState) -> Result<Uuid, AppError> {
    let bearer = headers
        .get(AUTHORIZATION)
        .and_then(|val| val.to_str().ok())
        .and_then(|val| val.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::unauthorized("missing bearer token"))?;

    if !constant_time_eq(bearer.as_bytes(), state.config.admin_token.as_bytes()) {
        return Err(AppError::unauthorized("invalid bearer token"));
    }

    let tenant_id_str = headers
        .get("x-tenant-id")
        .and_then(|val| val.to_str().ok())
        .ok_or_else(|| AppError::bad_request("missing x-tenant-id"))?;

    Uuid::parse_str(tenant_id_str).map_err(|_| AppError::bad_request("invalid x-tenant-id"))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter()
        .zip(right)
        .fold(0u8, |diff, (l, r)| diff | (l ^ r))
        == 0
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    use crate::{config::Config, credentials::CredentialCipher};
    use sqlx::postgres::PgPoolOptions;

    #[tokio::test]
    async fn saturated_execution_routes_return_503_without_limiting_reads() {
        let config = Config {
            database_url: "postgres://unused:unused@127.0.0.1:1/unused".into(),
            credential_key: "00".repeat(32),
            admin_token: "test-admin-token-with-at-least-32-characters".into(),
            workers: 1,
            max_in_flight_executions: 1,
            bind_address: "127.0.0.1:0".into(),
            default_timezone: "UTC".into(),
            base_url: None,
            webhook_base_url: None,
        };
        let pool = PgPoolOptions::new()
            .connect_lazy(&config.database_url)
            .unwrap();
        let cipher = CredentialCipher::from_hex_key(&config.credential_key).unwrap();
        let state = AppState::new(pool, config, cipher);
        let permit = state
            .execution_admission
            .clone()
            .try_acquire_owned()
            .unwrap();
        let shutdown = state.store.shutdown_token();
        let server_shutdown = shutdown.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router(state))
                .with_graceful_shutdown(server_shutdown.cancelled_owned())
                .await
        });

        let client = reqwest::Client::new();
        let overloaded = client
            .post(format!("http://{address}/api/nodes/test"))
            .body("malformed json")
            .send()
            .await
            .unwrap();
        assert_eq!(overloaded.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(overloaded.headers().get("retry-after").unwrap(), "1");

        let read = client
            .get(format!("http://{address}/api/executions"))
            .send()
            .await
            .unwrap();
        assert_eq!(read.status(), StatusCode::UNAUTHORIZED);

        drop(permit);
        shutdown.cancel();
        server.await.unwrap().unwrap();
    }
}
