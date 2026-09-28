use aes_gcm::{
    aead::{Aead, Generate, Key, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use sqlx::types::Uuid;

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
}
