use crate::{engine::model::NodeResult, nodes::resolve_template};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum ConditionOperator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Truthy,
    Falsy,
    Contains,
    Exists,
    IsEmpty,
}

#[derive(Deserialize)]
struct IfConfig {
    left: Value,
    operator: ConditionOperator,
    #[serde(default)]
    right: Option<Value>,
}

pub fn execute_if(config: &Value, input: &Value) -> Result<NodeResult, String> {
    let config: IfConfig = serde_json::from_value(config.clone())
        .map_err(|error| format!("invalid if node config: {error}"))?;

    let left = resolve_template(&config.left, input).unwrap_or(Value::Null);
    let right = config
        .right
        .as_ref()
        .map(|value| resolve_template(value, input).unwrap_or(Value::Null));

    let matched = compare(&left, config.operator, right.as_ref())?;

    Ok(NodeResult::new(input.clone()).with_route(if matched { "true" } else { "false" }))
}

fn compare(
    left: &Value,
    operator: ConditionOperator,
    right: Option<&Value>,
) -> Result<bool, String> {
    match operator {
        ConditionOperator::Eq => Ok(left == right.unwrap_or(&Value::Null)),
        ConditionOperator::Ne => Ok(left != right.unwrap_or(&Value::Null)),
        ConditionOperator::Truthy => Ok(truthy(left)),
        ConditionOperator::Falsy => Ok(!truthy(left)),
        ConditionOperator::Exists => Ok(!left.is_null()),
        ConditionOperator::IsEmpty => Ok(is_empty(left)),
        ConditionOperator::Contains => {
            let right = right.ok_or("contains comparison requires right value")?;
            match left {
                Value::String(s) => {
                    let needle = right.as_str().ok_or("string contains requires string right value")?;
                    Ok(s.contains(needle))
                }
                Value::Array(arr) => Ok(arr.contains(right)),
                Value::Object(obj) => {
                    let needle = right.as_str().ok_or("object contains requires string key right value")?;
                    Ok(obj.contains_key(needle))
                }
                _ => Ok(false),
            }
        }
        ConditionOperator::Gt
        | ConditionOperator::Gte
        | ConditionOperator::Lt
        | ConditionOperator::Lte => {
            let right = right.ok_or("ordered comparison requires right value")?;
            let ordering = if let (Some(left), Some(right)) = (left.as_f64(), right.as_f64()) {
                left.partial_cmp(&right)
            } else if let (Some(left), Some(right)) = (left.as_str(), right.as_str()) {
                Some(left.cmp(right))
            } else {
                None
            }
            .ok_or("ordered comparison requires two numbers or two strings")?;

            Ok(match operator {
                ConditionOperator::Gt => ordering.is_gt(),
                ConditionOperator::Gte => ordering.is_gt() || ordering.is_eq(),
                ConditionOperator::Lt => ordering.is_lt(),
                ConditionOperator::Lte => ordering.is_lt() || ordering.is_eq(),
                _ => unreachable!(),
            })
        }
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_contains_and_is_empty() {
        let input = json!({ "tag": "candidate-profile", "notes": "" });
        let config_contains = json!({
            "left": { "$from": "/tag" },
            "operator": "contains",
            "right": "profile"
        });
        assert_eq!(execute_if(&config_contains, &input).unwrap().route.as_deref(), Some("true"));

        let config_empty = json!({
            "left": { "$from": "/notes" },
            "operator": "is_empty"
        });
        assert_eq!(execute_if(&config_empty, &input).unwrap().route.as_deref(), Some("true"));
    }
}
