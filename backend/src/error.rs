use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub retryable: bool,
    #[serde(skip)]
    pub status: StatusCode,
}

impl AppError {
    pub fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            node_id: None,
            retryable: false,
            status,
        }
    }

    pub fn with_node(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "VALIDATION_ERROR", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "NOT_FOUND", message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "CONFLICT", message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(StatusCode::GATEWAY_TIMEOUT, "TIMEOUT", message).retryable(true)
    }

    pub fn provider_error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, code, message).retryable(true)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", message)
    }

    pub fn execution_failure(_details: &str) -> Self {
        Self::internal("execution failed; sensitive details omitted")
    }

    pub fn node_test_failure(_details: &str) -> Self {
        Self::bad_request("node test failed; sensitive details omitted")
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        Self::internal(format!("database error: {error}"))
    }
}

impl From<crate::store::StoreError> for AppError {
    fn from(error: crate::store::StoreError) -> Self {
        match error {
            crate::store::StoreError::NotFound(item) => {
                Self::not_found(format!("{item} not found"))
            }
            crate::store::StoreError::InvalidTransition { from, to } => {
                Self::conflict(format!("invalid status transition from {from:?} to {to:?}"))
            }
            crate::store::StoreError::Database(err) => {
                Self::internal(format!("database error: {err}"))
            }
            crate::store::StoreError::LeaseLost => Self::conflict("work lease was lost"),
            crate::store::StoreError::PayloadProtection => {
                Self::internal("execution payload protection failed")
            }
            crate::store::StoreError::IdempotencyConflict => Self::conflict(
                "idempotency key was already used with a different or unverifiable payload",
            ),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": self.message,
            "code": self.code,
            "node_id": self.node_id,
            "retryable": self.retryable,
        });
        (self.status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_failure_does_not_echo_upstream_error_details() {
        let upstream = "upstream response contains sensitive details";
        let error = AppError::execution_failure(upstream);
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(!error.message.contains(upstream));
        assert_eq!(error.message, "execution failed; sensitive details omitted");
    }

    #[test]
    fn node_test_failure_does_not_echo_upstream_error_details() {
        let upstream = "upstream response contains sensitive details";
        let error = AppError::node_test_failure(upstream);
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(!error.message.contains(upstream));
        assert_eq!(error.message, "node test failed; sensitive details omitted");
    }
}
