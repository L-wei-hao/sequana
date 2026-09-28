use chrono_tz::Tz;
use cron::Schedule;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;

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

#[derive(Debug, Clone)]
pub struct NodeResult {
    pub output: Value,
    pub route: Option<String>,
    pub metadata: Option<Value>,
}

impl NodeResult {
    pub fn new(output: Value) -> Self {
        Self {
            output,
            route: None,
            metadata: None,
        }
    }

    pub fn with_route(mut self, route: impl Into<String>) -> Self {
        self.route = Some(route.into());
        self
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl ExecutionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Queued, Self::Running)
                | (Self::Queued, Self::Cancelled)
                | (Self::Running, Self::Succeeded)
                | (Self::Running, Self::Failed)
                | (Self::Running, Self::Cancelled)
        )
    }
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
        let config: Self = Self::deserialize(value)
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
    "Asia/Singapore".into()
}

fn default_schedule_input() -> Value {
    Value::Object(Default::default())
}

pub fn normalize_cron(expression: &str) -> Result<String, String> {
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
