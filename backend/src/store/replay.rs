use super::{ExecutionRecord, Store, StoreError};
use crate::{credentials::CredentialCipher, telemetry::sanitize};
use serde_json::Value;
use sqlx::types::Uuid;
use std::sync::Arc;
use zeroize::Zeroizing;

impl Store {
    /// Production stores encrypt exact execution payloads for explicit replay access.
    pub fn with_cipher(pool: sqlx::PgPool, cipher: Arc<CredentialCipher>) -> Self {
        Self {
            cipher: Some(cipher),
            ..Self::new(pool)
        }
    }

    pub(super) fn protect_payload(
        &self,
        tenant: Uuid,
        value: &Value,
    ) -> Result<(Value, Option<Vec<u8>>), StoreError> {
        let diagnostic = sanitize(value);
        let encrypted = match &self.cipher {
            Some(cipher) => {
                let serialized = Zeroizing::new(
                    serde_json::to_string(value).map_err(|_| StoreError::PayloadProtection)?,
                );
                Some(
                    cipher
                        .encrypt(tenant, &serialized)
                        .map_err(|_| StoreError::PayloadProtection)?,
                )
            }
            None if diagnostic == *value => None,
            None => return Err(StoreError::PayloadProtection),
        };
        Ok((diagnostic, encrypted))
    }

    /// Only replay handlers should use this; telemetry endpoints use `execution`.
    pub async fn execution_for_replay(
        &self,
        tenant: Uuid,
        id: Uuid,
    ) -> Result<Option<ExecutionRecord>, StoreError> {
        self.recover_expired_execution(tenant, id).await?;
        let Some(mut record) = self.execution(tenant, id).await? else {
            return Ok(None);
        };
        let (input, output): (Option<Vec<u8>>, Option<Vec<u8>>) = sqlx::query_as(
            "SELECT input_encrypted, output_encrypted FROM executions WHERE id=$1 AND tenant_id=$2",
        )
        .bind(id)
        .bind(tenant)
        .fetch_one(&self.pool)
        .await?;
        if let Some(input) = input {
            record.input = self.decrypt_payload(tenant, &input)?;
        }
        if let Some(output) = output {
            record.output = Some(self.decrypt_payload(tenant, &output)?);
        }
        Ok(Some(record))
    }

    fn decrypt_payload(&self, tenant: Uuid, encrypted: &[u8]) -> Result<Value, StoreError> {
        let cipher = self.cipher.as_ref().ok_or(StoreError::PayloadProtection)?;
        let plaintext = cipher
            .decrypt(tenant, encrypted)
            .map_err(|_| StoreError::PayloadProtection)?;
        serde_json::from_str(&plaintext).map_err(|_| StoreError::PayloadProtection)
    }
}
