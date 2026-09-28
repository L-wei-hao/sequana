pub mod http;
pub mod if_node;
pub mod manual;
pub mod openai;
pub mod postgres;
pub mod respond;
pub mod schedule;
pub mod set;
pub mod switch;
pub mod webhook;

use crate::{
    credentials::CredentialCipher,
    engine::model::{Node, NodeResult, NodeType},
    store::Store,
};
use reqwest::Client;
use serde_json::{Map, Value};
use sqlx::{types::Uuid, PgPool};

pub struct NodeContext<'a> {
    pub db: &'a PgPool,
    pub http: &'a Client,
    pub store: &'a Store,
    pub tenant_id: Uuid,
    pub credentials: &'a CredentialCipher,
}

pub async fn execute_node(
    node: &Node,
    input: &Value,
    context: &NodeContext<'_>,
) -> Result<NodeResult, String> {
    match &node.node_type {
        NodeType::ManualTrigger => manual::execute_manual(&node.config, input),
        NodeType::Webhook => webhook::execute_webhook(&node.config, input),
        NodeType::Schedule => schedule::execute_schedule(&node.config, input),
        NodeType::Set => set::execute_set(&node.config, input),
        NodeType::If => if_node::execute_if(&node.config, input),
        NodeType::Switch => switch::execute_switch(&node.config, input),
        NodeType::HttpRequest => http::execute_http(&node.config, input, context).await,
        NodeType::Postgres => postgres::execute_postgres(&node.config, input, context).await,
        NodeType::RespondToWebhook => respond::execute_respond(&node.config, input),
        NodeType::OpenAi => openai::execute_openai(&node.config, input, context).await,
    }
}

pub fn resolve_template(template: &Value, input: &Value) -> Result<Value, String> {
    match template {
        Value::Object(object) if object.len() == 1 && object.contains_key("$from") => {
            let pointer = object["$from"]
                .as_str()
                .ok_or("$from must be a JSON Pointer string")?;
            input
                .pointer(pointer)
                .cloned()
                .ok_or_else(|| format!("input path does not exist: {pointer}"))
        }
        Value::String(s) => resolve_string_template(s, input),
        Value::Object(object) => {
            let mut resolved = Map::with_capacity(object.len());
            for (key, value) in object {
                resolved.insert(key.clone(), resolve_template(value, input)?);
            }
            Ok(Value::Object(resolved))
        }
        Value::Array(values) => {
            let mut resolved = Vec::with_capacity(values.len());
            for value in values {
                resolved.push(resolve_template(value, input)?);
            }
            Ok(Value::Array(resolved))
        }
        value => Ok(value.clone()),
    }
}

fn resolve_string_template(s: &str, input: &Value) -> Result<Value, String> {
    let trimmed = s.trim();
    if trimmed.starts_with("{{") && trimmed.ends_with("}}") {
        let inner = trimmed[2..trimmed.len() - 2].trim();
        let path = inner.strip_prefix("$json.").unwrap_or(inner);
        let pointer = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path.replace('.', "/"))
        };

        if let Some(val) = input.pointer(&pointer) {
            return Ok(val.clone());
        }
    }

    if s.contains("{{") && s.contains("}}") {
        let mut result = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(start) = rest.find("{{") {
            result.push_str(&rest[..start]);
            let after_start = &rest[start + 2..];
            if let Some(end) = after_start.find("}}") {
                let expr = after_start[..end].trim();
                let path = expr.strip_prefix("$json.").unwrap_or(expr);
                let pointer = if path.starts_with('/') {
                    path.to_string()
                } else {
                    format!("/{}", path.replace('.', "/"))
                };
                if let Some(val) = input.pointer(&pointer) {
                    match val {
                        Value::String(val_str) => result.push_str(val_str),
                        other => result.push_str(&other.to_string()),
                    }
                }
                rest = &after_start[end + 2..];
            } else {
                result.push_str("{{");
                rest = after_start;
            }
        }
        result.push_str(rest);
        return Ok(Value::String(result));
    }

    Ok(Value::String(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_template_resolution() {
        let input = json!({
            "candidate": {
                "name": "Ada Lovelace",
                "score": 95
            }
        });

        let t1 = json!({ "$from": "/candidate/name" });
        assert_eq!(resolve_template(&t1, &input).unwrap(), json!("Ada Lovelace"));

        let t2 = json!("{{$json.candidate.name}}");
        assert_eq!(resolve_template(&t2, &input).unwrap(), json!("Ada Lovelace"));

        let t3 = json!("Candidate name is {{$json.candidate.name}} with score {{$json.candidate.score}}");
        assert_eq!(
            resolve_template(&t3, &input).unwrap(),
            json!("Candidate name is Ada Lovelace with score 95")
        );
    }
}
