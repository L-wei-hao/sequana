use crate::{
    engine::model::NodeResult,
    nodes::{resolve_template, NodeContext},
};
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
    context: &NodeContext<'_>,
) -> Result<NodeResult, String> {
    let config: OpenAiConfig = serde_json::from_value(config.clone())
        .map_err(|error| format!("invalid openai node config: {error}"))?;

    let credential_id = Uuid::parse_str(&config.credential_id)
        .map_err(|error| format!("invalid OpenAI credential_id: {error}"))?;

    let credential = context
        .store
        .credential(context.tenant_id, credential_id)
        .await
        .map_err(|error| format!("failed to load OpenAI credential: {error:?}"))?
        .ok_or("OpenAI credential does not exist for this tenant")?;

    if credential.kind != "openai" {
        return Err("credential is not an OpenAI credential".into());
    }

    let api_key_raw = context.credentials.decrypt(context.tenant_id, &credential.encrypted_value)?;
    let api_key = if let Ok(parsed) = serde_json::from_str::<Value>(&api_key_raw) {
        parsed
            .get("api_key")
            .and_then(Value::as_str)
            .unwrap_or(&api_key_raw)
            .to_string()
    } else {
        api_key_raw.trim().to_string()
    };

    let request = build_request(&config, input)?;

    let response = context
        .http
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

    let model = response_json.get("model").cloned().unwrap_or(Value::Null);
    let response_id = response_json.get("id").cloned().unwrap_or(Value::Null);
    let input_tokens = response_json
        .pointer("/usage/input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cached_tokens = response_json
        .pointer("/usage/input_tokens_details/cached_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = response_json
        .pointer("/usage/output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let metadata = json!({
        "provider": "openai",
        "model": model,
        "response_id": response_id,
        "input_tokens": input_tokens,
        "cached_tokens": cached_tokens,
        "output_tokens": output_tokens
    });

    let node_output = json!({
        "output": output,
        "model": model,
        "response_id": response_id,
        "usage": {
            "input_tokens": input_tokens,
            "cached_tokens": cached_tokens,
            "output_tokens": output_tokens
        }
    });

    Ok(NodeResult::new(node_output).with_metadata(metadata))
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
