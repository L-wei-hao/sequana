use crate::engine::model::NodeResult;
use serde_json::Value;

pub fn execute_schedule(_config: &Value, input: &Value) -> Result<NodeResult, String> {
    Ok(NodeResult::new(input.clone()))
}
