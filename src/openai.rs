use crate::{
    credentials::CredentialCipher,
    engine::NodeResult,
    nodes::resolve_template,
    store::Store,
};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::types::Uuid;
use std::time::Duration;

#[derive(Deserialize)]
struct OpenAiConfig {
    credential_id: String,
    model: String,
    #[serde(default)]
    instructions: Option<Value>,
    input: Value,
    #[serde(default)]
    reasoning_effort: Option<String>,
    #[serde(default)]
    max_output_tokens: Option<u32>,
    #[serde(default)]
    output: OutputFormat,
    #[serde(default = "default_timeout_ms")]
    timeout_ms: u64,
}

#[derive(Default, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutputFormat {
    #[default]
    Text,
    JsonSchema {
        name: String,
        schema: Value,
        #[serde(default = "default_true")]
        strict: bool,
    },
}

fn default_true() -> bool {
    true
}

fn default_timeout_ms() -> u64 {
    120_000
}

pub async fn execute_openai(
    config: &Value,
    input: &Value,
    tenant_id: Uuid,
    store: &Store,
    cipher: &CredentialCipher,
    client: &Client,
) -> Result<NodeResult, String> {
    let config: OpenAiConfig =
        serde_json::from_value(config.clone()).map_err(|error| error.to_string())?;

    let credential_id = Uuid::parse_str(&config.credential_id)
        .map_err(|error| format!("invalid OpenAI credential_id: {error}"))?;
    let credential = store
        .credential(tenant_id, credential_id)
        .await
        .map_err(|error| format!("failed to load OpenAI credential: {error:?}"))?
        .ok_or("OpenAI credential does not exist for this tenant")?;

    if credential.kind != "openai" {
        return Err("credential is not an OpenAI credential".into());
    }

    let api_key = cipher.decrypt(tenant_id, &credential.encrypted_value)?;
    let request = build_request(&config, input)?;

    let response = client
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(api_key)
        .timeout(Duration::from_millis(config.timeout_ms))
        .json(&request)
        .send()
        .await
        .map_err(|error| format!("OpenAI request failed: {error}"))?;

    let status = response.status();
    let response_body = response
        .text()
        .await
        .map_err(|error| format!("failed to read OpenAI response: {error}"))?;
    let response_json: Value =
        serde_json::from_str(&response_body).unwrap_or(Value::String(response_body));

    if !status.is_success() {
        let message = response_json
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("OpenAI returned an error");
        return Err(format!("OpenAI {}: {message}", status.as_u16()));
    }

    let text = extract_output_text(&response_json)?;
    let output = match config.output {
        OutputFormat::Text => Value::String(text),
        OutputFormat::JsonSchema { .. } => serde_json::from_str(&text)
            .map_err(|error| format!("OpenAI structured output was invalid JSON: {error}"))?,
    };

    Ok(NodeResult {
        output: json!({
            "output": output,
            "model": response_json.get("model").cloned().unwrap_or(Value::Null),
            "response_id": response_json.get("id").cloned().unwrap_or(Value::Null),
            "usage": {
                "input_tokens": response_json.pointer("/usage/input_tokens").and_then(Value::as_u64).unwrap_or(0),
                "cached_tokens": response_json.pointer("/usage/input_tokens_details/cached_tokens").and_then(Value::as_u64).unwrap_or(0),
                "output_tokens": response_json.pointer("/usage/output_tokens").and_then(Value::as_u64).unwrap_or(0)
            }
        }),
        route: None,
    })
}

fn build_request(config: &OpenAiConfig, input: &Value) -> Result<Value, String> {
    let resolved_input = resolve_template(&config.input, input)?;
    let resolved_input = match resolved_input {
        Value::String(value) => Value::String(value),
        value => Value::String(value.to_string()),
    };

    let mut request = json!({
        "model": config.model,
        "input": resolved_input,
        "store": false
    });

    if let Some(instructions) = &config.instructions {
        let instructions = resolve_template(instructions, input)?;
        request["instructions"] = Value::String(match instructions {
            Value::String(value) => value,
            value => value.to_string(),
        });
    }

    if let Some(effort) = &config.reasoning_effort {
        request["reasoning"] = json!({ "effort": effort });
    }

    if let Some(max_output_tokens) = config.max_output_tokens {
        request["max_output_tokens"] = json!(max_output_tokens);
    }

    request["text"] = match &config.output {
        OutputFormat::Text => json!({
            "format": {
                "type": "text"
            }
        }),
        OutputFormat::JsonSchema {
            name,
            schema,
            strict,
        } => json!({
            "format": {
                "type": "json_schema",
                "name": name,
                "schema": schema,
                "strict": strict
            }
        }),
    };

    Ok(request)
}

fn extract_output_text(response: &Value) -> Result<String, String> {
    let mut text = String::new();

    for item in response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for content in item
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if content.get("type").and_then(Value::as_str) == Some("output_text") {
                if let Some(value) = content.get("text").and_then(Value::as_str) {
                    text.push_str(value);
                }
            }
        }
    }

    if text.is_empty() {
        Err("OpenAI response contained no output_text".into())
    } else {
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_structured_response_request() {
        let config: OpenAiConfig = serde_json::from_value(json!({
            "credential_id": "00000000-0000-0000-0000-000000000001",
            "model": "gpt-test",
            "instructions": {"$from": "/instructions"},
            "input": {"$from": "/body"},
            "reasoning_effort": "medium",
            "max_output_tokens": 500,
            "output": {
                "type": "json_schema",
                "name": "candidate",
                "strict": true,
                "schema": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    },
                    "required": ["name"],
                    "additionalProperties": false
                }
            }
        }))
        .unwrap();

        let request = build_request(
            &config,
            &json!({
                "instructions": "Return JSON",
                "body": "Ada"
            }),
        )
        .unwrap();

        assert_eq!(request["model"], "gpt-test");
        assert_eq!(request["input"], "Ada");
        assert_eq!(request["instructions"], "Return JSON");
        assert_eq!(request["reasoning"]["effort"], "medium");
        assert_eq!(request["max_output_tokens"], 500);
        assert_eq!(request["text"]["format"]["type"], "json_schema");
        assert_eq!(request["text"]["format"]["name"], "candidate");
        assert_eq!(request["text"]["format"]["strict"], true);
        assert_eq!(request["store"], false);
    }

    #[test]
    fn extracts_output_text() {
        let response = json!({
            "output": [{
                "type": "message",
                "content": [
                    { "type": "output_text", "text": "hello " },
                    { "type": "output_text", "text": "world" }
                ]
            }]
        });

        assert_eq!(extract_output_text(&response).unwrap(), "hello world");
    }
}
