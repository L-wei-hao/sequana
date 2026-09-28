use crate::engine::model::{NodeType, ScheduleConfig, WorkflowDefinition};
use std::collections::{HashMap, HashSet, VecDeque};

pub fn validate_workflow(workflow: &WorkflowDefinition) -> Result<(), String> {
    if workflow.nodes.is_empty() {
        return Err("workflow must contain at least one node".into());
    }

    let mut ids = HashSet::with_capacity(workflow.nodes.len());
    for node in &workflow.nodes {
        if node.id.trim().is_empty() {
            return Err("node id cannot be empty".into());
        }
        if !ids.insert(node.id.as_str()) {
            return Err(format!("duplicate node id: {}", node.id));
        }
        if node.node_type == NodeType::Schedule {
            ScheduleConfig::parse(&node.config)
                .map_err(|error| format!("schedule node {}: {error}", node.id))?;
        }
    }

    let mut outgoing: HashMap<&str, Vec<&str>> = workflow
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), Vec::new()))
        .collect();
    let mut inbound: HashMap<&str, usize> = workflow
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), 0))
        .collect();

    for edge in &workflow.edges {
        if !ids.contains(edge.source.as_str()) {
            return Err(format!("edge source does not exist: {}", edge.source));
        }
        if !ids.contains(edge.target.as_str()) {
            return Err(format!("edge target does not exist: {}", edge.target));
        }

        outgoing
            .get_mut(edge.source.as_str())
            .expect("validated source")
            .push(edge.target.as_str());
        *inbound
            .get_mut(edge.target.as_str())
            .expect("validated target") += 1;
    }

    let triggers: Vec<_> = workflow
        .nodes
        .iter()
        .filter(|node| node.node_type.is_trigger())
        .collect();

    if triggers.is_empty() {
        return Err("workflow must contain at least one trigger".into());
    }

    for trigger in &triggers {
        if inbound[trigger.id.as_str()] != 0 {
            return Err(format!(
                "trigger node cannot have incoming edges: {}",
                trigger.id
            ));
        }
    }

    let mut remaining = inbound.clone();
    let mut queue: VecDeque<&str> = remaining
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect();
    let mut visited = 0;

    while let Some(node_id) = queue.pop_front() {
        visited += 1;
        for target in &outgoing[node_id] {
            let count = remaining.get_mut(target).expect("validated target");
            *count -= 1;
            if *count == 0 {
                queue.push_back(target);
            }
        }
    }

    if visited != workflow.nodes.len() {
        return Err("workflow contains a cycle".into());
    }

    for node in &workflow.nodes {
        if !node.node_type.is_trigger() && inbound[node.id.as_str()] > 1 {
            return Err(format!(
                "node cannot have multiple incoming edges in V1: {}",
                node.id
            ));
        }

        if node.node_type == NodeType::RespondToWebhook
            && !outgoing[node.id.as_str()].is_empty()
        {
            return Err(format!(
                "respond_to_webhook must be terminal: {}",
                node.id
            ));
        }
    }

    let mut reachable = HashSet::new();
    let mut queue: VecDeque<&str> = triggers
        .iter()
        .map(|node| node.id.as_str())
        .collect();

    while let Some(node_id) = queue.pop_front() {
        if !reachable.insert(node_id) {
            continue;
        }
        queue.extend(outgoing[node_id].iter().copied());
    }

    if let Some(node) = workflow
        .nodes
        .iter()
        .find(|node| !reachable.contains(node.id.as_str()))
    {
        return Err(format!("node is not reachable from a trigger: {}", node.id));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::model::{Edge, Node, NodeType};
    use serde_json::Value;

    fn node(id: &str, node_type: NodeType) -> Node {
        Node {
            id: id.into(),
            node_type,
            config: Value::Null,
            position: None,
        }
    }

    #[test]
    fn rejects_edge_to_missing_node() {
        let workflow = WorkflowDefinition {
            nodes: vec![node("trigger", NodeType::ManualTrigger)],
            edges: vec![Edge {
                source: "trigger".into(),
                target: "missing".into(),
                route: None,
            }],
        };
        assert!(validate_workflow(&workflow).is_err());
    }

    #[test]
    fn rejects_cycles() {
        let workflow = WorkflowDefinition {
            nodes: vec![
                node("trigger", NodeType::ManualTrigger),
                node("a", NodeType::Set),
                node("b", NodeType::Set),
            ],
            edges: vec![
                Edge {
                    source: "trigger".into(),
                    target: "a".into(),
                    route: None,
                },
                Edge {
                    source: "a".into(),
                    target: "b".into(),
                    route: None,
                },
                Edge {
                    source: "b".into(),
                    target: "a".into(),
                    route: None,
                },
            ],
        };
        assert_eq!(validate_workflow(&workflow), Err("workflow contains a cycle".into()));
    }
}
