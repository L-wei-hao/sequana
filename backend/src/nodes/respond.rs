use crate::{engine::model::NodeResult, nodes::resolve_template};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct RespondConfig {
    #[serde(default = "default_status")]
    status: u16,
    #[serde(default)]
    headers: BTreeMap<String, Value>,
    #[serde(default)]
    body: Option<Value>,
}

fn default_status() -> u16 {
    200
}

pub fn execute_respond(config: &Value, input: &Value) -> Result<NodeResult, String> {
    let config: RespondConfig = serde_json::from_value(config.clone())
        .map_err(|error| format!("invalid respond node config: {error}"))?;

    if !(100..=599).contains(&config.status) {
        return Err("response status must be between 100 and 599".into());
    }

    let mut headers = BTreeMap::new();
    for (k, v) in config.headers {
        let val = resolve_template(&v, input)?;
        let val_str = match val {
            Value::String(s) => s,
            other => other.to_string(),
        };
        headers.insert(k, val_str);
    }

    let body = config
        .body
        .as_ref()
        .map(|body| resolve_template(body, input))
        .transpose()?
        .unwrap_or_else(|| input.clone());

    Ok(NodeResult::new(json!({
        "status": config.status,
        "headers": headers,
        "body": body
    })))
}
