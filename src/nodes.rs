use crate::{
    credentials::CredentialCipher,
    engine::NodeResult,
    openai::execute_openai,
    store::Store,
    workflow::{Node, NodeType},
};
use reqwest::{header::{HeaderName, HeaderValue}, Client, Method};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Map, Value};
use sqlx::{types::{Json, Uuid}, PgPool};
use std::{collections::BTreeMap, str::FromStr, time::Duration};

pub struct NodeContext<'a> {
    pub db: &'a PgPool,
    pub http: &'a Client,
    pub store: &'a Store,
    pub tenant_id: Uuid,
    pub credentials: &'a CredentialCipher,
}

pub async fn execute_node(node: &Node, input: &Value, context: &NodeContext<'_>) -> Result<NodeResult, String> {
    match &node.node_type {
        NodeType::ManualTrigger | NodeType::Webhook | NodeType::Schedule => Ok(NodeResult { output: input.clone(), route: None }),
        NodeType::Set => execute_set(&node.config, input),
        NodeType::If => execute_if(&node.config, input),
        NodeType::Switch => execute_switch(&node.config, input),
        NodeType::HttpRequest => execute_http(&node.config, input, context.http).await,
        NodeType::Postgres => execute_postgres(&node.config, input, context.db).await,
        NodeType::RespondToWebhook => execute_respond(&node.config, input),
        NodeType::OpenAi => execute_openai(
            &node.config,
            input,
            context.tenant_id,
            context.store,
            context.credentials,
            context.http,
        ).await,
    }
}

fn parse_config<T: DeserializeOwned>(config: &Value) -> Result<T, String> {
    serde_json::from_value(config.clone()).map_err(|error| error.to_string())
}

pub fn resolve_template(template: &Value, input: &Value) -> Result<Value, String> {
    match template {
        Value::Object(object) if object.len() == 1 && object.contains_key("$from") => {
            let pointer = object["$from"].as_str().ok_or("$from must be a JSON Pointer string")?;
            input.pointer(pointer).cloned().ok_or_else(|| format!("input path does not exist: {pointer}"))
        }
        Value::Object(object) => {
            let mut resolved = Map::new();
            for (key, value) in object {
                resolved.insert(key.clone(), resolve_template(value, input)?);
            }
            Ok(Value::Object(resolved))
        }
        Value::Array(values) => values.iter().map(|value| resolve_template(value, input)).collect::<Result<Vec<_>, _>>().map(Value::Array),
        value => Ok(value.clone()),
    }
}

fn default_true() -> bool { true }

#[derive(Deserialize)]
struct SetConfig {
    values: Value,
    #[serde(default = "default_true")]
    merge_input: bool,
}

fn execute_set(config: &Value, input: &Value) -> Result<NodeResult, String> {
    let config: SetConfig = parse_config(config)?;
    let values = resolve_template(&config.values, input)?;

    let output = if config.merge_input {
        let mut output = input.as_object().cloned().ok_or("Set merge_input requires object input")?;
        let values = values.as_object().ok_or("Set merge_input requires object values")?;
        output.extend(values.clone());
        Value::Object(output)
    } else {
        values
    };

    Ok(NodeResult { output, route: None })
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConditionOperator { Eq, Ne, Gt, Gte, Lt, Lte, Truthy, Falsy }

#[derive(Deserialize)]
struct IfConfig {
    left: Value,
    operator: ConditionOperator,
    #[serde(default)]
    right: Option<Value>,
}

fn execute_if(config: &Value, input: &Value) -> Result<NodeResult, String> {
    let config: IfConfig = parse_config(config)?;
    let left = resolve_template(&config.left, input)?;
    let right = config.right.as_ref().map(|value| resolve_template(value, input)).transpose()?;
    let matched = compare(&left, config.operator, right.as_ref())?;

    Ok(NodeResult {
        output: input.clone(),
        route: Some(if matched { "true" } else { "false" }.into()),
    })
}

fn compare(left: &Value, operator: ConditionOperator, right: Option<&Value>) -> Result<bool, String> {
    match operator {
        ConditionOperator::Eq => Ok(left == right.ok_or("eq requires right")?),
        ConditionOperator::Ne => Ok(left != right.ok_or("ne requires right")?),
        ConditionOperator::Truthy => Ok(truthy(left)),
        ConditionOperator::Falsy => Ok(!truthy(left)),
        ConditionOperator::Gt | ConditionOperator::Gte | ConditionOperator::Lt | ConditionOperator::Lte => {
            let right = right.ok_or("comparison requires right")?;
            let ordering = if let (Some(left), Some(right)) = (left.as_f64(), right.as_f64()) {
                left.partial_cmp(&right)
            } else if let (Some(left), Some(right)) = (left.as_str(), right.as_str()) {
                Some(left.cmp(right))
            } else {
                None
            }.ok_or("ordered comparison requires two numbers or two strings")?;

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
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

#[derive(Deserialize)]
struct SwitchCase { equals: Value, route: String }

#[derive(Deserialize)]
struct SwitchConfig {
    value: Value,
    cases: Vec<SwitchCase>,
    #[serde(default)]
    default_route: Option<String>,
}

fn execute_switch(config: &Value, input: &Value) -> Result<NodeResult, String> {
    let config: SwitchConfig = parse_config(config)?;
    let value = resolve_template(&config.value, input)?;
    let route = config.cases.into_iter().find(|case| case.equals == value).map(|case| case.route).or(config.default_route);
    Ok(NodeResult { output: input.clone(), route })
}

fn default_method() -> String { "GET".into() }

#[derive(Deserialize)]
struct HttpConfig {
    #[serde(default = "default_method")]
    method: String,
    url: Value,
    #[serde(default)]
    headers: BTreeMap<String, Value>,
    #[serde(default)]
    body: Option<Value>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    continue_on_http_error: bool,
}

async fn execute_http(config: &Value, input: &Value, client: &Client) -> Result<NodeResult, String> {
    let config: HttpConfig = parse_config(config)?;
    let method = Method::from_bytes(config.method.as_bytes()).map_err(|error| format!("invalid HTTP method: {error}"))?;
    let url = resolve_template(&config.url, input)?;
    let url = url
        .as_str()
        .ok_or("HTTP URL must resolve to a string")?;
    let mut request = client.request(method, url);

    if let Some(timeout_ms) = config.timeout_ms {
        request = request.timeout(Duration::from_millis(timeout_ms));
    }

    for (name, value) in config.headers {
        let name = HeaderName::from_str(&name).map_err(|error| format!("invalid HTTP header name: {error}"))?;
        let value = resolve_template(&value, input)?;
        let value = value.as_str().ok_or("HTTP header template must resolve to a string")?;
        let value = HeaderValue::from_str(value).map_err(|error| format!("invalid HTTP header value: {error}"))?;
        request = request.header(name, value);
    }

    if let Some(body) = config.body {
        request = request.json(&resolve_template(&body, input)?);
    }

    let response = request.send().await.map_err(|error| format!("HTTP request failed: {error}"))?;
    let status = response.status().as_u16();
    let headers = response.headers().iter().filter_map(|(name, value)| {
        value.to_str().ok().map(|value| (name.to_string(), Value::String(value.into())))
    }).collect::<Map<String, Value>>();
    let text = response.text().await.map_err(|error| format!("failed to read HTTP response: {error}"))?;
    let body = serde_json::from_str(&text).unwrap_or(Value::String(text.clone()));

    if status >= 400 && !config.continue_on_http_error {
        return Err(format!("HTTP {status}: {text}"));
    }

    Ok(NodeResult { output: json!({ "status": status, "headers": headers, "body": body }), route: None })
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SqlMode { Query, Execute }

impl Default for SqlMode {
    fn default() -> Self { Self::Query }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SqlParamType { Text, I64, F64, Bool, Uuid, Json }

#[derive(Deserialize)]
struct SqlParam {
    #[serde(rename = "type")]
    kind: SqlParamType,
    value: Value,
}

#[derive(Deserialize)]
struct PostgresConfig {
    query: String,
    #[serde(default)]
    mode: SqlMode,
    #[serde(default)]
    params: Vec<SqlParam>,
}

enum BoundParam {
    Text(Option<String>),
    I64(Option<i64>),
    F64(Option<f64>),
    Bool(Option<bool>),
    Uuid(Option<Uuid>),
    Json(Option<Json<Value>>),
}

fn resolve_sql_param(param: SqlParam, input: &Value) -> Result<BoundParam, String> {
    let value = resolve_template(&param.value, input)?;

    if value.is_null() {
        return Ok(match param.kind {
            SqlParamType::Text => BoundParam::Text(None),
            SqlParamType::I64 => BoundParam::I64(None),
            SqlParamType::F64 => BoundParam::F64(None),
            SqlParamType::Bool => BoundParam::Bool(None),
            SqlParamType::Uuid => BoundParam::Uuid(None),
            SqlParamType::Json => BoundParam::Json(None),
        });
    }

    match param.kind {
        SqlParamType::Text => value.as_str().map(|value| BoundParam::Text(Some(value.into()))).ok_or("SQL text parameter must resolve to a string".into()),
        SqlParamType::I64 => value.as_i64().map(|value| BoundParam::I64(Some(value))).ok_or("SQL i64 parameter must resolve to an integer".into()),
        SqlParamType::F64 => value.as_f64().map(|value| BoundParam::F64(Some(value))).ok_or("SQL f64 parameter must resolve to a number".into()),
        SqlParamType::Bool => value.as_bool().map(|value| BoundParam::Bool(Some(value))).ok_or("SQL bool parameter must resolve to a boolean".into()),
        SqlParamType::Uuid => {
            let value = value
                .as_str()
                .ok_or_else(|| "SQL UUID parameter must resolve to a string".to_string())?;
            let value = Uuid::parse_str(value).map_err(|error| error.to_string())?;
            Ok(BoundParam::Uuid(Some(value)))
        },
        SqlParamType::Json => Ok(BoundParam::Json(Some(Json(value)))),
    }
}

async fn execute_postgres(config: &Value, input: &Value, pool: &PgPool) -> Result<NodeResult, String> {
    let config: PostgresConfig = parse_config(config)?;
    let params = config.params.into_iter().map(|param| resolve_sql_param(param, input)).collect::<Result<Vec<_>, _>>()?;

    match config.mode {
        SqlMode::Execute => {
            let mut query = sqlx::query(&config.query);
            for param in params {
                query = match param {
                    BoundParam::Text(value) => query.bind(value),
                    BoundParam::I64(value) => query.bind(value),
                    BoundParam::F64(value) => query.bind(value),
                    BoundParam::Bool(value) => query.bind(value),
                    BoundParam::Uuid(value) => query.bind(value),
                    BoundParam::Json(value) => query.bind(value),
                };
            }
            let result = query.execute(pool).await.map_err(|error| format!("PostgreSQL execute failed: {error}"))?;
            Ok(NodeResult { output: json!({ "rows_affected": result.rows_affected() }), route: None })
        }
        SqlMode::Query => {
            let sql = config.query.trim().trim_end_matches(';');
            if sql.is_empty() {
                return Err("PostgreSQL query cannot be empty".into());
            }

            let wrapped = format!("SELECT to_jsonb(sequana_row) FROM ({sql}) AS sequana_row");
            let mut query = sqlx::query_scalar::<_, Json<Value>>(&wrapped);
            for param in params {
                query = match param {
                    BoundParam::Text(value) => query.bind(value),
                    BoundParam::I64(value) => query.bind(value),
                    BoundParam::F64(value) => query.bind(value),
                    BoundParam::Bool(value) => query.bind(value),
                    BoundParam::Uuid(value) => query.bind(value),
                    BoundParam::Json(value) => query.bind(value),
                };
            }
            let rows = query.fetch_all(pool).await.map_err(|error| format!("PostgreSQL query failed: {error}"))?
                .into_iter().map(|row| row.0).collect::<Vec<_>>();
            Ok(NodeResult { output: Value::Array(rows), route: None })
        }
    }
}

#[derive(Deserialize)]
struct RespondConfig {
    #[serde(default = "default_status")]
    status: u16,
    #[serde(default)]
    body: Option<Value>,
}

fn default_status() -> u16 { 200 }

fn execute_respond(config: &Value, input: &Value) -> Result<NodeResult, String> {
    let config: RespondConfig = parse_config(config)?;
    if !(100..=599).contains(&config.status) {
        return Err("response status must be between 100 and 599".into());
    }
    let body = config.body.as_ref().map(|body| resolve_template(body, input)).transpose()?.unwrap_or_else(|| input.clone());
    Ok(NodeResult { output: json!({ "status": config.status, "body": body }), route: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_json_pointer_templates() {
        let input = json!({ "candidate": { "name": "Ada" } });
        let template = json!({ "name": { "$from": "/candidate/name" }, "source": "webhook" });
        assert_eq!(resolve_template(&template, &input).unwrap(), json!({ "name": "Ada", "source": "webhook" }));
    }

    #[test]
    fn if_routes_boolean_result() {
        let config = json!({ "left": { "$from": "/score" }, "operator": "gte", "right": 80 });
        let result = execute_if(&config, &json!({ "score": 90 })).unwrap();
        assert_eq!(result.route.as_deref(), Some("true"));
    }
}
