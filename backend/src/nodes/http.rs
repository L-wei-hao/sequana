use crate::{
    credentials::{CredentialCipher, CredentialPayload},
    engine::model::NodeResult,
    nodes::{resolve_template, NodeContext},
};
use reqwest::{
    header::{HeaderName, HeaderValue},
    Method,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::types::Uuid;
use std::{
    collections::BTreeMap,
    str::FromStr,
    time::{Duration, Instant},
};

fn default_method() -> String {
    "GET".into()
}

#[derive(Deserialize)]
struct HttpConfig {
    #[serde(default = "default_method")]
    method: String,
    url: Value,
    #[serde(default)]
    headers: BTreeMap<String, Value>,
    #[serde(default)]
    body: Option<Value>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    credential_id: Option<String>,
    #[serde(default)]
    continue_on_http_error: bool,
}

pub async fn execute_http(
    config: &Value,
    input: &Value,
    context: &NodeContext<'_>,
) -> Result<NodeResult, String> {
    let config: HttpConfig = serde_json::from_value(config.clone())
        .map_err(|error| format!("invalid http node config: {error}"))?;

    let method = Method::from_bytes(config.method.as_bytes())
        .map_err(|error| format!("invalid HTTP method: {error}"))?;

    let url_val = resolve_template(&config.url, input)?;
    let url = url_val
        .as_str()
        .ok_or("HTTP URL must resolve to a string")?;

    validate_url(url)?;

    let mut request = context.http.request(method, url);

    if let Some(timeout_ms) = config.timeout_ms {
        request = request.timeout(Duration::from_millis(timeout_ms));
    }

    if let Some(cred_id_str) = config.credential_id {
        if !cred_id_str.trim().is_empty() {
            let cred_id =
                Uuid::parse_str(&cred_id_str).map_err(|e| format!("invalid credential_id: {e}"))?;
            let record = context
                .store
                .credential(context.tenant_id, cred_id)
                .await
                .map_err(|e| format!("failed to load HTTP credential: {e:?}"))?
                .ok_or_else(|| format!("HTTP credential not found: {cred_id}"))?;

            request =
                apply_credential_auth(request, context.credentials, context.tenant_id, &record)?;
        }
    }

    for (name, val) in config.headers {
        let name = HeaderName::from_str(&name)
            .map_err(|error| format!("invalid HTTP header name: {error}"))?;
        let resolved = resolve_template(&val, input)?;
        let val_str = match resolved {
            Value::String(s) => s,
            other => other.to_string(),
        };
        let value = HeaderValue::from_str(&val_str)
            .map_err(|error| format!("invalid HTTP header value: {error}"))?;
        request = request.header(name, value);
    }

    if let Some(body) = config.body {
        let resolved = resolve_template(&body, input)?;
        request = request.json(&resolved);
    }

    let start = Instant::now();
    let response = request
        .send()
        .await
        .map_err(|error| format!("HTTP request failed: {error}"))?;

    let duration_ms = start.elapsed().as_millis() as u64;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.to_string(), Value::String(v.to_string())))
        })
        .collect::<Map<String, Value>>();

    let text = response
        .text()
        .await
        .map_err(|error| format!("failed to read HTTP response: {error}"))?;
    if status >= 400 && !config.continue_on_http_error {
        return Err(format!("HTTP {status}: {text}"));
    }
    let body = match serde_json::from_str(&text) {
        Ok(body) => body,
        Err(_) => Value::String(text),
    };

    Ok(NodeResult::new(json!({
        "status": status,
        "headers": headers,
        "body": body,
        "duration_ms": duration_ms
    })))
}

fn validate_url(url: &str) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("HTTP URL must start with http:// or https://".into());
    }
    Ok(())
}

fn apply_credential_auth(
    mut request: reqwest::RequestBuilder,
    cipher: &CredentialCipher,
    tenant_id: Uuid,
    record: &crate::store::CredentialRecord,
) -> Result<reqwest::RequestBuilder, String> {
    let decrypted = cipher.decrypt(tenant_id, &record.encrypted_value)?;
    match record.kind.as_str() {
        "http_bearer" => {
            let payload: CredentialPayload = serde_json::from_str(&decrypted)
                .map_err(|_| "failed to parse bearer credential".to_string())?;
            if let CredentialPayload::HttpBearer { token } = &payload {
                request = request.bearer_auth(token);
            }
        }
        "http_basic" => {
            let payload: CredentialPayload = serde_json::from_str(&decrypted)
                .map_err(|_| "failed to parse basic credential".to_string())?;
            if let CredentialPayload::HttpBasic { username, password } = &payload {
                request = request.basic_auth(username, Some(password));
            }
        }
        "http_header" => {
            let payload: CredentialPayload = serde_json::from_str(&decrypted)
                .map_err(|_| "failed to parse header credential".to_string())?;
            if let CredentialPayload::HttpHeader {
                header_name,
                header_value,
            } = &payload
            {
                let name = HeaderName::from_str(header_name)
                    .map_err(|e| format!("invalid header name: {e}"))?;
                let val = HeaderValue::from_str(header_value)
                    .map_err(|e| format!("invalid header value: {e}"))?;
                request = request.header(name, val);
            }
        }
        "openai" => {
            request = request.bearer_auth(decrypted.trim());
        }
        other => {
            return Err(format!(
                "unsupported credential kind for HTTP node: {other}"
            ))
        }
    }
    Ok(request)
}
