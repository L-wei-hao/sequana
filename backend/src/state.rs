use crate::{config::Config, credentials::CredentialCipher, store::Store};
use reqwest::Client;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub store: Arc<Store>,
    pub http: Client,
    pub credentials: Arc<CredentialCipher>,
    pub config: Arc<Config>,
    pub(crate) execution_admission: Arc<tokio::sync::Semaphore>,
}

impl AppState {
    pub fn new(db: PgPool, config: Config, credentials: CredentialCipher) -> Self {
        let credentials = Arc::new(credentials);
        let execution_admission =
            Arc::new(tokio::sync::Semaphore::new(config.max_in_flight_executions));
        let store = Arc::new(Store::with_cipher(db.clone(), credentials.clone()));
        Self {
            db,
            store,
            http: Client::new(),
            credentials,
            config: Arc::new(config),
            execution_admission,
        }
    }
}
