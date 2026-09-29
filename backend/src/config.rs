use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub credential_key: String,
    pub admin_token: Arc<str>,
    pub workers: usize,
    pub max_in_flight_executions: usize,
    pub bind_address: String,
    pub default_timezone: String,
    pub base_url: Option<String>,
    pub webhook_base_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let database_url =
            std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set".to_string())?;

        let credential_key = std::env::var("SEQUANA_CREDENTIAL_KEY")
            .map_err(|_| "SEQUANA_CREDENTIAL_KEY must be set".to_string())?;

        if credential_key.len() != 64 {
            return Err("SEQUANA_CREDENTIAL_KEY must be exactly 64 hex characters".to_string());
        }

        let admin_token = std::env::var("SEQUANA_ADMIN_TOKEN")
            .map_err(|_| "SEQUANA_ADMIN_TOKEN must be set".to_string())?;

        if admin_token.len() < 32 {
            return Err("SEQUANA_ADMIN_TOKEN must be at least 32 characters".to_string());
        }

        let workers = std::env::var("SEQUANA_WORKERS")
            .ok()
            .and_then(|val| val.parse::<usize>().ok())
            .unwrap_or(2)
            .max(1);

        let max_in_flight_executions = match std::env::var("SEQUANA_MAX_IN_FLIGHT_EXECUTIONS") {
            Ok(value) => value
                .parse::<usize>()
                .map_err(|_| "SEQUANA_MAX_IN_FLIGHT_EXECUTIONS must be an integer".to_string())?,
            Err(std::env::VarError::NotPresent) => 32,
            Err(_) => return Err("SEQUANA_MAX_IN_FLIGHT_EXECUTIONS is not valid Unicode".into()),
        };
        if !(1..=10_000).contains(&max_in_flight_executions) {
            return Err("SEQUANA_MAX_IN_FLIGHT_EXECUTIONS must be between 1 and 10000".into());
        }

        let bind_address =
            std::env::var("SEQUANA_BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

        let default_timezone = std::env::var("SEQUANA_DEFAULT_TIMEZONE")
            .unwrap_or_else(|_| "Asia/Singapore".to_string());

        let base_url = std::env::var("SEQUANA_BASE_URL").ok();
        let webhook_base_url = std::env::var("SEQUANA_WEBHOOK_BASE_URL").ok();

        Ok(Self {
            database_url,
            credential_key,
            admin_token: Arc::from(admin_token),
            workers,
            max_in_flight_executions,
            bind_address,
            default_timezone,
            base_url,
            webhook_base_url,
        })
    }
}
