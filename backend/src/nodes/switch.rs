use crate::{engine::model::NodeResult, nodes::resolve_template};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct SwitchCase {
    equals: Value,
    route: String,
}

#[derive(Deserialize)]
struct SwitchConfig {
    value: Value,
    cases: Vec<SwitchCase>,
    #[serde(default)]
    default_route: Option<String>,
}

pub fn execute_switch(config: &Value, input: &Value) -> Result<NodeResult, String> {
    let config: SwitchConfig = serde_json::from_value(config.clone())
        .map_err(|error| format!("invalid switch node config: {error}"))?;

    let value = resolve_template(&config.value, input)?;
    let route = config
        .cases
        .into_iter()
        .find(|case| case.equals == value)
        .map(|case| case.route)
        .or(config.default_route);

    let mut result = NodeResult::new(input.clone());
    if let Some(r) = route {
        result = result.with_route(r);
    }
    Ok(result)
}
