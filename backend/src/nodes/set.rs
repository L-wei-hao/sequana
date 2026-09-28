use crate::{engine::model::NodeResult, nodes::resolve_template};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
struct SetConfig {
    #[serde(default)]
    values: Option<Value>,
    #[serde(default)]
    rename: Option<BTreeMap<String, String>>,
    #[serde(default)]
    remove: Option<Vec<String>>,
    #[serde(default = "default_true")]
    merge_input: bool,
}

pub fn execute_set(config: &Value, input: &Value) -> Result<NodeResult, String> {
    let config: SetConfig = serde_json::from_value(config.clone())
        .map_err(|error| format!("invalid set node config: {error}"))?;

    let mut current_obj: Map<String, Value> = if config.merge_input {
        input.as_object().cloned().unwrap_or_default()
    } else {
        Map::new()
    };

    if let Some(values) = config.values {
        let resolved = resolve_template(&values, input)?;
        match resolved {
            Value::Object(obj) => {
                current_obj.extend(obj);
            }
            other if !config.merge_input => {
                return Ok(NodeResult::new(other));
            }
            _ => {}
        }
    }

    if let Some(rename_map) = config.rename {
        for (old_key, new_key) in rename_map {
            if let Some(val) = current_obj.remove(&old_key) {
                current_obj.insert(new_key, val);
            }
        }
    }

    if let Some(remove_keys) = config.remove {
        for key in remove_keys {
            current_obj.remove(&key);
        }
    }

    Ok(NodeResult::new(Value::Object(current_obj)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_set_and_rename_and_remove() {
        let input = json!({ "a": 1, "b": 2, "c": 3 });
        let config = json!({
            "values": { "d": 4 },
            "rename": { "a": "alpha" },
            "remove": ["b"],
            "merge_input": true
        });

        let result = execute_set(&config, &input).unwrap();
        assert_eq!(
            result.output,
            json!({ "alpha": 1, "c": 3, "d": 4 })
        );
    }
}
