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
}

impl AppState {
    pub fn new(
        db: PgPool,
        config: Config,
        credentials: CredentialCipher,
    ) -> Self {
        let store = Arc::new(Store::new(db.clone()));
        Self {
            db,
            store,
            http: Client::new(),
            credentials: Arc::new(credentials),
            config: Arc::new(config),
        }
    }
}
