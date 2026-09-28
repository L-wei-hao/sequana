use crate::workflow::WorkflowDefinition;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct NodeResult {
    pub output: Value,
    pub route: Option<String>,
}

pub fn next_node_ids(
    workflow: &WorkflowDefinition,
    node_id: &str,
    result: &NodeResult,
) -> Result<Vec<String>, String> {
    if !workflow.nodes.iter().any(|node| node.id == node_id) {
        return Err(format!("node does not exist: {node_id}"));
    }

    let outgoing: Vec<_> = workflow
        .edges
        .iter()
        .filter(|edge| edge.source == node_id)
        .collect();

    let exact: Vec<String> = result
        .route
        .as_deref()
        .map(|route| {
            outgoing
                .iter()
                .filter(|edge| edge.route.as_deref() == Some(route))
                .map(|edge| edge.target.clone())
                .collect()
        })
        .unwrap_or_default();

    if !exact.is_empty() {
        return Ok(exact);
    }

    Ok(outgoing
        .iter()
        .filter(|edge| edge.route.is_none())
        .map(|edge| edge.target.clone())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{Edge, Node, NodeType};

    fn workflow() -> WorkflowDefinition {
        WorkflowDefinition {
            nodes: vec![
                Node {
                    id: "if".into(),
                    node_type: NodeType::If,
                    config: Value::Null,
                    position: None,
                },
                Node {
                    id: "yes".into(),
                    node_type: NodeType::Set,
                    config: Value::Null,
                    position: None,
                },
                Node {
                    id: "fallback".into(),
                    node_type: NodeType::Set,
                    config: Value::Null,
                    position: None,
                },
            ],
            edges: vec![
                Edge {
                    source: "if".into(),
                    target: "yes".into(),
                    route: Some("true".into()),
                },
                Edge {
                    source: "if".into(),
                    target: "fallback".into(),
                    route: None,
                },
            ],
        }
    }

    #[test]
    fn route_prefers_exact_match_then_default() {
        let workflow = workflow();

        let yes = next_node_ids(
            &workflow,
            "if",
            &NodeResult {
                output: Value::Null,
                route: Some("true".into()),
            },
        )
        .unwrap();
        assert_eq!(yes, vec!["yes"]);

        let fallback = next_node_ids(
            &workflow,
            "if",
            &NodeResult {
                output: Value::Null,
                route: Some("false".into()),
            },
        )
        .unwrap();
        assert_eq!(fallback, vec!["fallback"]);
    }
}
