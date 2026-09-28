use crate::{
    api::authorize, credentials::CredentialCipher, error::AppError, state::AppState,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::types::Uuid;

pub async fn list_credentials(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;
    let credentials = state.store.list_credentials(tenant_id).await?;

    Ok(Json(Value::Array(
        credentials
            .into_iter()
            .map(|credential| {
                json!({
                    "id": credential.id,
                    "name": credential.name,
                    "kind": credential.kind,
                    "created_at": credential.created_at,
                    "updated_at": credential.updated_at
                })
            })
            .collect(),
    )))
}

#[derive(Deserialize)]
pub struct CreateCredentialRequest {
    pub name: String,
    pub kind: String,
    pub value: Value,
}

pub async fn create_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateCredentialRequest>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;

    if request.name.trim().is_empty() {
        return Err(AppError::bad_request("credential name cannot be empty"));
    }

    let payload_str = CredentialCipher::validate_and_serialize_payload(&request.kind, &request.value)
        .map_err(AppError::bad_request)?;

    let encrypted = state
        .credentials
        .encrypt(tenant_id, &payload_str)
        .map_err(AppError::internal)?;

    let credential_id = state
        .store
        .create_credential(tenant_id, request.name.trim(), &request.kind, &encrypted)
        .await?;

    Ok(Json(json!({ "credential_id": credential_id })))
}

#[derive(Deserialize)]
pub struct UpdateCredentialRequest {
    pub name: String,
    #[serde(default)]
    pub value: Option<Value>,
}

pub async fn update_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(credential_id): Path<Uuid>,
    Json(request): Json<UpdateCredentialRequest>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;

    if request.name.trim().is_empty() {
        return Err(AppError::bad_request("credential name cannot be empty"));
    }

    let existing = state
        .store
        .credential(tenant_id, credential_id)
        .await?
        .ok_or_else(|| AppError::not_found("credential not found"))?;

    let encrypted = if let Some(new_val) = request.value {
        let payload_str = CredentialCipher::validate_and_serialize_payload(&existing.kind, &new_val)
            .map_err(AppError::bad_request)?;
        state
            .credentials
            .encrypt(tenant_id, &payload_str)
            .map_err(AppError::internal)?
    } else {
        existing.encrypted_value
    };

    let updated = state
        .store
        .update_credential(tenant_id, credential_id, request.name.trim(), &encrypted)
        .await?;

    if !updated {
        return Err(AppError::not_found("credential not found"));
    }

    Ok(Json(json!({ "updated": true })))
}

pub async fn delete_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(credential_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = authorize(&headers, &state)?;

    let deleted = state.store.delete_credential(tenant_id, credential_id).await?;
    if !deleted {
        return Err(AppError::not_found("credential not found"));
    }

    Ok(Json(json!({ "deleted": true })))
}
