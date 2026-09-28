use crate::engine::model::NodeResult;
use serde_json::Value;

pub fn execute_manual(_config: &Value, input: &Value) -> Result<NodeResult, String> {
    Ok(NodeResult::new(input.clone()))
}
