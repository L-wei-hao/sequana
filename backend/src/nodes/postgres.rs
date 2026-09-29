use crate::{
    credentials::CredentialPayload,
    engine::model::NodeResult,
    nodes::{resolve_template, NodeContext},
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{
    postgres::PgPoolOptions,
    types::{Json, Uuid},
    PgPool,
};
use std::time::Duration;

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SqlMode {
    #[default]
    Query,
    Execute,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SqlParamType {
    Text,
    I64,
    F64,
    Bool,
    Uuid,
    Json,
}

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
    #[serde(default)]
    credential_id: Option<String>,
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
        SqlParamType::Text => value
            .as_str()
            .map(|val| BoundParam::Text(Some(val.into())))
            .ok_or_else(|| "SQL text parameter must resolve to a string".to_string()),
        SqlParamType::I64 => value
            .as_i64()
            .map(|val| BoundParam::I64(Some(val)))
            .ok_or_else(|| "SQL i64 parameter must resolve to an integer".to_string()),
        SqlParamType::F64 => value
            .as_f64()
            .map(|val| BoundParam::F64(Some(val)))
            .ok_or_else(|| "SQL f64 parameter must resolve to a number".to_string()),
        SqlParamType::Bool => value
            .as_bool()
            .map(|val| BoundParam::Bool(Some(val)))
            .ok_or_else(|| "SQL bool parameter must resolve to a boolean".to_string()),
        SqlParamType::Uuid => {
            let val = value
                .as_str()
                .ok_or_else(|| "SQL UUID parameter must resolve to a string".to_string())?;
            let parsed = Uuid::parse_str(val).map_err(|error| error.to_string())?;
            Ok(BoundParam::Uuid(Some(parsed)))
        }
        SqlParamType::Json => Ok(BoundParam::Json(Some(Json(value)))),
    }
}

pub async fn execute_postgres(
    config: &Value,
    input: &Value,
    context: &NodeContext<'_>,
) -> Result<NodeResult, String> {
    let config: PostgresConfig = serde_json::from_value(config.clone())
        .map_err(|error| format!("invalid postgres node config: {error}"))?;

    let params = config
        .params
        .into_iter()
        .map(|param| resolve_sql_param(param, input))
        .collect::<Result<Vec<_>, _>>()?;

    if let Some(cred_id_str) = config.credential_id {
        if !cred_id_str.trim().is_empty() {
            let cred_id = Uuid::parse_str(&cred_id_str)
                .map_err(|e| format!("invalid postgres credential_id: {e}"))?;
            let record = context
                .store
                .credential(context.tenant_id, cred_id)
                .await
                .map_err(|e| format!("failed to load postgres credential: {e:?}"))?
                .ok_or_else(|| format!("PostgreSQL credential not found: {cred_id}"))?;

            if record.kind != "postgres" {
                return Err("credential is not a postgres credential".into());
            }

            let decrypted = context
                .credentials
                .decrypt(context.tenant_id, &record.encrypted_value)?;
            let payload: CredentialPayload = serde_json::from_str(&decrypted)
                .map_err(|e| format!("failed to parse postgres credential payload: {e}"))?;

            if let CredentialPayload::Postgres {
                host,
                port,
                database,
                username,
                password,
                ssl_mode,
            } = &payload
            {
                let encoded_password = zeroize::Zeroizing::new(urlencoding::encode(password));
                let conn_str = zeroize::Zeroizing::new(format!(
                    "postgres://{}:{}@{}:{}/{}?sslmode={}",
                    username,
                    encoded_password.as_str(),
                    host,
                    port,
                    database,
                    ssl_mode
                ));

                let target_pool = PgPoolOptions::new()
                    .max_connections(2)
                    .acquire_timeout(Duration::from_secs(5))
                    .connect(&conn_str)
                    .await
                    .map_err(|e| {
                        format!("failed to connect to external PostgreSQL database: {e}")
                    })?;

                return run_query_on_pool(&target_pool, &config.query, config.mode, params).await;
            }
        }
    }

    run_query_on_pool(context.db, &config.query, config.mode, params).await
}

mod urlencoding {
    use std::fmt::Write;

    pub fn encode(s: &str) -> String {
        let mut encoded = String::with_capacity(s.len());
        for b in s.bytes() {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
                encoded.push(b as char);
            } else {
                let _ = write!(encoded, "%{:02X}", b);
            }
        }
        encoded
    }
}

async fn run_query_on_pool(
    pool: &PgPool,
    raw_query: &str,
    mode: SqlMode,
    params: Vec<BoundParam>,
) -> Result<NodeResult, String> {
    match mode {
        SqlMode::Execute => {
            let mut query = sqlx::query(raw_query);
            for param in params {
                query = match param {
                    BoundParam::Text(val) => query.bind(val),
                    BoundParam::I64(val) => query.bind(val),
                    BoundParam::F64(val) => query.bind(val),
                    BoundParam::Bool(val) => query.bind(val),
                    BoundParam::Uuid(val) => query.bind(val),
                    BoundParam::Json(val) => query.bind(val),
                };
            }
            let result = query
                .execute(pool)
                .await
                .map_err(|error| format!("PostgreSQL execute failed: {error}"))?;
            Ok(NodeResult::new(
                json!({ "rows_affected": result.rows_affected() }),
            ))
        }
        SqlMode::Query => {
            let sql = raw_query.trim().trim_end_matches(';');
            if sql.is_empty() {
                return Err("PostgreSQL query cannot be empty".into());
            }

            let wrapped = format!("SELECT to_jsonb(sequana_row) FROM ({sql}) AS sequana_row");
            let mut query = sqlx::query_scalar::<_, Json<Value>>(&wrapped);
            for param in params {
                query = match param {
                    BoundParam::Text(val) => query.bind(val),
                    BoundParam::I64(val) => query.bind(val),
                    BoundParam::F64(val) => query.bind(val),
                    BoundParam::Bool(val) => query.bind(val),
                    BoundParam::Uuid(val) => query.bind(val),
                    BoundParam::Json(val) => query.bind(val),
                };
            }
            let rows = query
                .fetch_all(pool)
                .await
                .map_err(|error| format!("PostgreSQL query failed: {error}"))?
                .into_iter()
                .map(|row| row.0)
                .collect::<Vec<_>>();
            Ok(NodeResult::new(Value::Array(rows)))
        }
    }
}
