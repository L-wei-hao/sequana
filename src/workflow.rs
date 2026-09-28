use chrono_tz::Tz;
use cron::Schedule;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    str::FromStr,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    ManualTrigger,
    Webhook,
    RespondToWebhook,
    HttpRequest,
    OpenAi,
    Postgres,
    Set,
    If,
    Switch,
    Schedule,
}

impl NodeType {
    pub fn is_trigger(&self) -> bool {
        matches!(self, Self::ManualTrigger | Self::Webhook | Self::Schedule)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ManualTrigger => "manual_trigger",
            Self::Webhook => "webhook",
            Self::RespondToWebhook => "respond_to_webhook",
            Self::HttpRequest => "http_request",
            Self::OpenAi => "open_ai",
            Self::Postgres => "postgres",
            Self::Set => "set",
            Self::If => "if",
            Self::Switch => "switch",
            Self::Schedule => "schedule",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub node_type: NodeType,
    #[serde(default)]
    pub config: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<NodePosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub route: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    pub cron: String,
    #[serde(default = "default_schedule_timezone")]
    pub timezone: String,
    #[serde(default = "default_schedule_input")]
    pub input: Value,
}

impl ScheduleConfig {
    pub fn parse(value: &Value) -> Result<Self, String> {
        let config: Self = serde_json::from_value(value.clone())
            .map_err(|error| format!("invalid schedule config: {error}"))?;

        config.schedule()?;
        config.timezone()?;
        Ok(config)
    }

    pub fn schedule(&self) -> Result<Schedule, String> {
        let expression = normalize_cron(&self.cron)?;
        Schedule::from_str(&expression)
            .map_err(|error| format!("invalid cron expression: {error}"))
    }

    pub fn timezone(&self) -> Result<Tz, String> {
        self.timezone
            .parse()
            .map_err(|_| format!("invalid timezone: {}", self.timezone))
    }
}

fn default_schedule_timezone() -> String {
    "UTC".into()
}

fn default_schedule_input() -> Value {
    Value::Object(Default::default())
}

fn normalize_cron(expression: &str) -> Result<String, String> {
    let expression = expression.trim();
    if expression.is_empty() {
        return Err("cron expression cannot be empty".into());
    }

    Ok(match expression.split_whitespace().count() {
        5 => format!("0 {expression}"),
        6 | 7 => expression.to_string(),
        _ => return Err("cron expression must have 5, 6, or 7 fields".into()),
    })
}

impl WorkflowDefinition {
    pub fn validate(&self) -> Result<(), String> {
        if self.nodes.is_empty() {
            return Err("workflow must contain at least one node".into());
        }

        let mut ids = HashSet::with_capacity(self.nodes.len());
        for node in &self.nodes {
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

        let mut outgoing: HashMap<&str, Vec<&str>> = self
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), Vec::new()))
            .collect();
        let mut inbound: HashMap<&str, usize> = self
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), 0))
            .collect();

        for edge in &self.edges {
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

        let triggers: Vec<&Node> = self
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

        if visited != self.nodes.len() {
            return Err("workflow contains a cycle".into());
        }

        for node in &self.nodes {
            // ponytail: V1 has no Merge node, so fan-in has no honest semantics yet.
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

        if let Some(node) = self
            .nodes
            .iter()
            .find(|node| !reachable.contains(node.id.as_str()))
        {
            return Err(format!("node is not reachable from a trigger: {}", node.id));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert!(workflow.validate().is_err());
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

        assert_eq!(workflow.validate(), Err("workflow contains a cycle".into()));
    }

    #[test]
    fn rejects_fan_in_without_merge_semantics() {
        let workflow = WorkflowDefinition {
            nodes: vec![
                node("trigger", NodeType::ManualTrigger),
                node("a", NodeType::Set),
                node("b", NodeType::Set),
                node("joined", NodeType::Set),
            ],
            edges: vec![
                Edge { source: "trigger".into(), target: "a".into(), route: None },
                Edge { source: "trigger".into(), target: "b".into(), route: None },
                Edge { source: "a".into(), target: "joined".into(), route: None },
                Edge { source: "b".into(), target: "joined".into(), route: None },
            ],
        };

        assert_eq!(
            workflow.validate(),
            Err("node cannot have multiple incoming edges in V1: joined".into())
        );
    }

    #[test]
    fn accepts_five_field_schedule_in_named_timezone() {
        let config = ScheduleConfig::parse(&serde_json::json!({
            "cron": "*/5 * * * *",
            "timezone": "Asia/Singapore"
        }))
        .unwrap();

        assert!(config.schedule().is_ok());
        assert!(config.timezone().is_ok());
    }

    #[test]
    fn accepts_reachable_dag() {
        let workflow = WorkflowDefinition {
            nodes: vec![
                node("trigger", NodeType::Webhook),
                node("set", NodeType::Set),
                node("respond", NodeType::RespondToWebhook),
            ],
            edges: vec![
                Edge {
                    source: "trigger".into(),
                    target: "set".into(),
                    route: None,
                },
                Edge {
                    source: "set".into(),
                    target: "respond".into(),
                    route: None,
                },
            ],
        };

        assert_eq!(workflow.validate(), Ok(()));
    }
}
