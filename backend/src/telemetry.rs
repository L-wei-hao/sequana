//! Redacted diagnostic copies. Never mutate the payload used to execute a node.
//!
//! Key-based redaction is defense in depth, not a classifier for arbitrary PII.
//! Free-form provider errors are omitted because they can echo secret values.
use serde_json::{Map, Value};

const REDACTED: &str = "[REDACTED]";

pub fn sanitized_error(error: Option<&str>) -> Option<&'static str> {
    error.map(|_| "Execution failed; sensitive error details omitted")
}

pub fn sanitize(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let normalized: String = key
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric())
                        .flat_map(char::to_lowercase)
                        .collect();
                    let value = if sensitive_key(&normalized) {
                        Value::String(REDACTED.into())
                    } else if normalized == "headers" {
                        sanitize_headers(value)
                    } else if normalized.ends_with("body") || normalized.ends_with("path") {
                        // Opaque payloads and path components may carry credentials under arbitrary names.
                        Value::String(REDACTED.into())
                    } else if matches!(normalized.as_str(), "query" | "querystring" | "url" | "uri")
                        && value.is_string()
                    {
                        // URLs and raw query strings may contain credentials in arbitrary parameter names.
                        Value::String(REDACTED.into())
                    } else {
                        sanitize(value)
                    };
                    (key.clone(), value)
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(sanitize).collect()),
        other => other.clone(),
    }
}

fn sensitive_key(key: &str) -> bool {
    matches!(
        key,
        "authorization"
            | "proxyauthorization"
            | "cookie"
            | "setcookie"
            | "apikey"
            | "xapikey"
            | "password"
            | "passwd"
            | "secret"
            | "clientsecret"
            | "token"
            | "accesstoken"
            | "refreshtoken"
            | "idtoken"
            | "credential"
            | "credentials"
            | "connectionstring"
            | "databaseurl"
            | "privatekey"
    ) || key.ends_with("password")
        || key.ends_with("secret")
        || key.ends_with("token")
        || key.ends_with("apikey")
}

fn sanitize_headers(value: &Value) -> Value {
    let Some(headers) = value.as_object() else {
        return Value::String(REDACTED.into());
    };
    let sanitized: Map<String, Value> = headers
        .iter()
        .map(|(key, value)| {
            // Custom authentication headers cannot be reliably identified by their names.
            let safe = matches!(
                key.to_ascii_lowercase().as_str(),
                "content-type" | "content-length" | "accept"
            );
            (
                key.clone(),
                if safe {
                    sanitize(value)
                } else {
                    Value::String(REDACTED.into())
                },
            )
        })
        .collect();
    Value::Object(sanitized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nested_secrets_and_custom_headers_are_redacted() {
        let value = json!({"API_Key":"secret-value", "nested":[{"refresh_token":"secret-value","ok":42}], "headers":{"X-Custom-Auth":"secret-value","Content-Type":"application/json"}, "url":"https://example.test/?custom=secret-value"});
        let result = sanitize(&value);
        assert!(!result.to_string().contains("secret-value"));
        assert_eq!(result["nested"][0]["ok"], 42);
        assert_eq!(result["headers"]["Content-Type"], "application/json");
        assert_eq!(value["API_Key"], "secret-value");
    }

    #[test]
    fn opaque_body_and_path_are_redacted() {
        let value = json!({
            "body": "opaque token=secret-value",
            "path": "/webhook/secret-value",
            "query": "token=secret-value"
        });
        let result = sanitize(&value);
        assert_eq!(result["body"], REDACTED);
        assert_eq!(result["path"], REDACTED);
        assert_eq!(result["query"], REDACTED);
        assert!(!result.to_string().contains("secret-value"));
        assert_eq!(value["body"], "opaque token=secret-value");
    }
}
