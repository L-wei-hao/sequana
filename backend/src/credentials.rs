use aes_gcm::{
    aead::{Aead, Generate, Key, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use sqlx::types::Uuid;
use zeroize::{Zeroize, Zeroizing};

/// Credential request JSON whose owned string values are wiped on drop.
#[derive(Deserialize)]
#[serde(transparent)]
pub struct SecretJson(pub Value);

impl Zeroize for SecretJson {
    fn zeroize(&mut self) {
        fn wipe(value: &mut Value) {
            match value {
                Value::String(value) => value.zeroize(),
                Value::Array(values) => values.iter_mut().for_each(wipe),
                Value::Object(values) => values.values_mut().for_each(wipe),
                _ => {}
            }
        }
        wipe(&mut self.0);
    }
}

impl Drop for SecretJson {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl zeroize::ZeroizeOnDrop for SecretJson {}

#[derive(Serialize, Deserialize, Zeroize, zeroize::ZeroizeOnDrop)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CredentialPayload {
    Openai {
        api_key: String,
    },
    Postgres {
        host: String,
        #[serde(default = "default_postgres_port")]
        port: u16,
        database: String,
        username: String,
        password: String,
        #[serde(default = "default_ssl_mode")]
        ssl_mode: String,
    },
    HttpBearer {
        token: String,
    },
    HttpBasic {
        username: String,
        password: String,
    },
    HttpHeader {
        header_name: String,
        header_value: String,
    },
}

impl std::fmt::Debug for CredentialPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialPayload([REDACTED])")
    }
}

fn default_postgres_port() -> u16 {
    5432
}

fn default_ssl_mode() -> String {
    "prefer".to_string()
}

pub struct CredentialCipher {
    cipher: Aes256Gcm,
    fingerprint_key: Zeroizing<[u8; 32]>,
}

impl CredentialCipher {
    pub fn from_hex_key(key: &str) -> Result<Self, String> {
        let key = Zeroizing::new(decode_hex_key(key)?);
        let mut fingerprint_key = Zeroizing::new([0u8; 32]);
        hkdf::Hkdf::<Sha256>::new(Some(b"sequana-credential-key-v1"), key.as_slice())
            .expand(
                b"execution-idempotency-fingerprint-v1",
                &mut fingerprint_key[..],
            )
            .map_err(|_| "idempotency fingerprint key derivation failed")?;
        Ok(Self {
            cipher: Aes256Gcm::new(
                <&Key<Aes256Gcm>>::try_from(key.as_slice())
                    .map_err(|_| "invalid encryption key")?,
            ),
            fingerprint_key,
        })
    }

    /// Returns a stable, tenant-bound keyed digest without exposing payload plaintext.
    pub fn payload_fingerprint(&self, tenant_id: Uuid, payload: &[u8]) -> [u8; 32] {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.fingerprint_key[..])
            .expect("HMAC accepts keys of any length");
        mac.update(b"sequana-execution-idempotency-v1\0");
        mac.update(tenant_id.as_bytes());
        mac.update(payload);
        mac.finalize().into_bytes().into()
    }

    pub fn encrypt(&self, tenant_id: Uuid, plaintext: &str) -> Result<Vec<u8>, String> {
        let nonce = Nonce::generate();
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: tenant_id.as_bytes(),
                },
            )
            .map_err(|_| "credential encryption failed")?;

        let mut stored = nonce.to_vec();
        stored.extend(ciphertext);
        Ok(stored)
    }

    pub fn decrypt(&self, tenant_id: Uuid, stored: &[u8]) -> Result<Zeroizing<String>, String> {
        if stored.len() <= 12 {
            return Err("credential ciphertext is invalid".into());
        }

        let (nonce, ciphertext) = stored.split_at(12);
        let plaintext = self
            .cipher
            .decrypt(
                <&Nonce<_>>::try_from(nonce).map_err(|_| "invalid credential nonce")?,
                Payload {
                    msg: ciphertext,
                    aad: tenant_id.as_bytes(),
                },
            )
            .map_err(|_| "credential decryption failed")?;

        match String::from_utf8(plaintext) {
            Ok(plaintext) => Ok(Zeroizing::new(plaintext)),
            Err(error) => {
                error.into_bytes().zeroize();
                Err("credential plaintext is not UTF-8".into())
            }
        }
    }

    pub fn validate_and_serialize_payload(
        kind: &str,
        raw_value: &Value,
    ) -> Result<Zeroizing<String>, String> {
        match kind {
            "openai" => {
                if let Some(key) = raw_value.as_str() {
                    if key.trim().is_empty() {
                        return Err("OpenAI API key cannot be empty".into());
                    }
                    Ok(key.trim().to_string())
                } else if let Some(key) = raw_value.get("api_key").and_then(Value::as_str) {
                    if key.trim().is_empty() {
                        return Err("OpenAI API key cannot be empty".into());
                    }
                    Ok(key.trim().to_string())
                } else {
                    Err(
                        "OpenAI credential requires an API key string or object with 'api_key'"
                            .into(),
                    )
                }
            }
            "postgres" => {
                let port = raw_value
                    .get("port")
                    .and_then(Value::as_u64)
                    .unwrap_or(u64::from(default_postgres_port()));
                let port = u16::try_from(port)
                    .map_err(|_| "PostgreSQL credential port exceeds the valid range")?;
                let payload = CredentialPayload::Postgres {
                    host: raw_value
                        .get("host")
                        .and_then(Value::as_str)
                        .ok_or("host is required")?
                        .to_owned(),
                    port,
                    database: raw_value
                        .get("database")
                        .and_then(Value::as_str)
                        .ok_or("database is required")?
                        .to_owned(),
                    username: raw_value
                        .get("username")
                        .and_then(Value::as_str)
                        .ok_or("username is required")?
                        .to_owned(),
                    password: raw_value
                        .get("password")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    ssl_mode: raw_value
                        .get("ssl_mode")
                        .and_then(Value::as_str)
                        .unwrap_or("prefer")
                        .to_owned(),
                };
                serde_json::to_string(&payload).map_err(|e| e.to_string())
            }
            "http_bearer" => {
                let token = if let Some(token) = raw_value.as_str() {
                    token
                } else {
                    raw_value
                        .get("token")
                        .and_then(Value::as_str)
                        .ok_or("token is required")?
                };
                serde_json::to_string(&CredentialPayload::HttpBearer {
                    token: token.to_string(),
                })
                .map_err(|e| e.to_string())
            }
            "http_basic" => {
                let username = raw_value
                    .get("username")
                    .and_then(Value::as_str)
                    .ok_or("username is required")?;
                let password = raw_value
                    .get("password")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                serde_json::to_string(&CredentialPayload::HttpBasic {
                    username: username.to_string(),
                    password: password.to_string(),
                })
                .map_err(|e| e.to_string())
            }
            "http_header" => {
                let header_name = raw_value
                    .get("header_name")
                    .and_then(Value::as_str)
                    .ok_or("header_name is required")?;
                let header_value = raw_value
                    .get("header_value")
                    .and_then(Value::as_str)
                    .ok_or("header_value is required")?;
                serde_json::to_string(&CredentialPayload::HttpHeader {
                    header_name: header_name.to_string(),
                    header_value: header_value.to_string(),
                })
                .map_err(|e| e.to_string())
            }
            other => Err(format!("unsupported credential kind: {other}")),
        }
        .map(Zeroizing::new)
    }
}

pub fn mask_secret(secret: &str) -> String {
    let len = secret.len();
    if len <= 8 {
        "***".to_string()
    } else {
        format!("{}...{}", &secret[..3], &secret[len - 4..])
    }
}

fn decode_hex_key(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 {
        return Err("SEQUANA_CREDENTIAL_KEY must be exactly 64 hex characters".into());
    }

    let mut key = [0u8; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        key[index] = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
    }
    Ok(key)
}

fn hex_digit(value: u8) -> Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err("SEQUANA_CREDENTIAL_KEY contains non-hex characters".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_debug_redacts_secret_fields() {
        let payload = CredentialPayload::HttpBasic {
            username: "private-user".into(),
            password: "private-password".into(),
        };
        let debug = format!("{payload:?}");
        assert!(!debug.contains("private-user"));
        assert!(!debug.contains("private-password"));
    }

    #[test]
    fn credentials_round_trip_and_are_tenant_bound() {
        let cipher = CredentialCipher::from_hex_key(&"01".repeat(32)).unwrap();
        let tenant = Uuid::nil();
        let encrypted = cipher.encrypt(tenant, "sk-test").unwrap();

        assert_ne!(encrypted, b"sk-test");
        assert_eq!(
            cipher.decrypt(tenant, &encrypted).unwrap().as_str(),
            "sk-test"
        );

        let other_tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        assert!(cipher.decrypt(other_tenant, &encrypted).is_err());
    }

    #[test]
    fn plaintext_buffers_are_zeroizing() {
        let cipher = CredentialCipher::from_hex_key(&"01".repeat(32)).unwrap();
        let serialized = CredentialCipher::validate_and_serialize_payload(
            "http_bearer",
            &serde_json::json!("test-token"),
        )
        .unwrap();
        let encrypted = cipher.encrypt(Uuid::nil(), &serialized).unwrap();
        let decrypted = cipher.decrypt(Uuid::nil(), &encrypted).unwrap();
        assert!(std::any::type_name_of_val(&serialized).contains("Zeroizing"));
        assert!(std::any::type_name_of_val(&decrypted).contains("Zeroizing"));
        assert_eq!(serialized.as_str(), decrypted.as_str());
    }

    #[test]
    fn postgres_credential_rejects_port_overflow() {
        let value = serde_json::json!({
            "host": "db.local",
            "port": 65_536,
            "database": "app",
            "username": "user",
            "password": "test-password"
        });
        assert!(CredentialCipher::validate_and_serialize_payload("postgres", &value).is_err());
    }

    #[test]
    fn credential_payload_zeroizes_fields() {
        fn requires_drop_wipe<T: zeroize::ZeroizeOnDrop>() {}
        requires_drop_wipe::<CredentialPayload>();
        let mut payload = CredentialPayload::HttpBasic {
            username: "private-user".into(),
            password: "private-password".into(),
        };
        payload.zeroize();
        if let CredentialPayload::HttpBasic { username, password } = &payload {
            assert!(username.is_empty());
            assert!(password.is_empty());
        } else {
            panic!("variant changed");
        }
    }

    #[test]
    fn secret_json_preserves_wire_shape_and_wipes_nested_strings() {
        let mut value: SecretJson =
            serde_json::from_str(r#"{"password":"example","nested":["secret",5]}"#).unwrap();
        assert_eq!(value.0["password"], "example");
        value.zeroize();
        assert_eq!(value.0["password"], "");
        assert_eq!(value.0["nested"][0], "");
        assert_eq!(value.0["nested"][1], 5);
        fn requires_drop_wipe<T: zeroize::ZeroizeOnDrop>() {}
        requires_drop_wipe::<SecretJson>();
    }

    #[test]
    fn masks_secret_properly() {
        assert_eq!(mask_secret("secret"), "***");
        assert_eq!(mask_secret("sk-1234567890abcdef"), "sk-...cdef");
    }
}
