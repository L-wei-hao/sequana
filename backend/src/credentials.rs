use aes_gcm::{
    aead::{Aead, Generate, Key, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

fn default_postgres_port() -> u16 {
    5432
}

fn default_ssl_mode() -> String {
    "prefer".to_string()
}

pub struct CredentialCipher {
    cipher: Aes256Gcm,
}

impl CredentialCipher {
    pub fn from_hex_key(key: &str) -> Result<Self, String> {
        let key = decode_hex_key(key)?;
        Ok(Self {
            cipher: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key)),
        })
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

    pub fn decrypt(&self, tenant_id: Uuid, stored: &[u8]) -> Result<String, String> {
        if stored.len() <= 12 {
            return Err("credential ciphertext is invalid".into());
        }

        let (nonce, ciphertext) = stored.split_at(12);
        let plaintext = self
            .cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: tenant_id.as_bytes(),
                },
            )
            .map_err(|_| "credential decryption failed")?;

        String::from_utf8(plaintext).map_err(|_| "credential plaintext is not UTF-8".into())
    }

    pub fn validate_and_serialize_payload(kind: &str, raw_value: &Value) -> Result<String, String> {
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
                    Err("OpenAI credential requires an API key string or object with 'api_key'".into())
                }
            }
            "postgres" => {
                let payload: CredentialPayload = serde_json::from_value(
                    serde_json::json!({
                        "kind": "postgres",
                        "host": raw_value.get("host").and_then(Value::as_str).ok_or("host is required")?,
                        "port": raw_value.get("port").and_then(Value::as_u64).unwrap_or(5432) as u16,
                        "database": raw_value.get("database").and_then(Value::as_str).ok_or("database is required")?,
                        "username": raw_value.get("username").and_then(Value::as_str).ok_or("username is required")?,
                        "password": raw_value.get("password").and_then(Value::as_str).unwrap_or(""),
                        "ssl_mode": raw_value.get("ssl_mode").and_then(Value::as_str).unwrap_or("prefer"),
                    })
                ).map_err(|e| format!("invalid postgres credential: {e}"))?;
                serde_json::to_string(&payload).map_err(|e| e.to_string())
            }
            "http_bearer" => {
                let token = if let Some(token) = raw_value.as_str() {
                    token
                } else {
                    raw_value.get("token").and_then(Value::as_str).ok_or("token is required")?
                };
                serde_json::to_string(&CredentialPayload::HttpBearer { token: token.to_string() })
                    .map_err(|e| e.to_string())
            }
            "http_basic" => {
                let username = raw_value.get("username").and_then(Value::as_str).ok_or("username is required")?;
                let password = raw_value.get("password").and_then(Value::as_str).unwrap_or("");
                serde_json::to_string(&CredentialPayload::HttpBasic {
                    username: username.to_string(),
                    password: password.to_string(),
                }).map_err(|e| e.to_string())
            }
            "http_header" => {
                let header_name = raw_value.get("header_name").and_then(Value::as_str).ok_or("header_name is required")?;
                let header_value = raw_value.get("header_value").and_then(Value::as_str).ok_or("header_value is required")?;
                serde_json::to_string(&CredentialPayload::HttpHeader {
                    header_name: header_name.to_string(),
                    header_value: header_value.to_string(),
                }).map_err(|e| e.to_string())
            }
            other => Err(format!("unsupported credential kind: {other}")),
        }
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
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
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
    fn credentials_round_trip_and_are_tenant_bound() {
        let cipher = CredentialCipher::from_hex_key(&"01".repeat(32)).unwrap();
        let tenant = Uuid::nil();
        let encrypted = cipher.encrypt(tenant, "sk-test").unwrap();

        assert_ne!(encrypted, b"sk-test");
        assert_eq!(cipher.decrypt(tenant, &encrypted).unwrap(), "sk-test");

        let other_tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        assert!(cipher.decrypt(other_tenant, &encrypted).is_err());
    }

    #[test]
    fn masks_secret_properly() {
        assert_eq!(mask_secret("secret"), "***");
        assert_eq!(mask_secret("sk-1234567890abcdef"), "sk-...cdef");
    }
}
