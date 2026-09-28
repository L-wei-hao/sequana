mod api;
mod credentials;
mod db;
mod engine;
mod execution;
mod nodes;
mod openai;
mod runner;
mod scheduler;
mod store;
mod workflow;

use api::AppState;
use credentials::CredentialCipher;
use reqwest::Client;
use std::sync::Arc;
use store::Store;

#[tokio::main]
async fn main() {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let credential_key = std::env::var("SEQUANA_CREDENTIAL_KEY")
        .expect("SEQUANA_CREDENTIAL_KEY must be set");
    let admin_token =
        std::env::var("SEQUANA_ADMIN_TOKEN").expect("SEQUANA_ADMIN_TOKEN must be set");

    if admin_token.len() < 32 {
        panic!("SEQUANA_ADMIN_TOKEN must be at least 32 characters");
    }

    let pool = db::connect(&database_url)
        .await
        .expect("failed to connect to PostgreSQL");

    db::migrate(&pool)
        .await
        .expect("failed to run database migrations");

    let credentials = CredentialCipher::from_hex_key(&credential_key)
        .expect("SEQUANA_CREDENTIAL_KEY is invalid");
    let state = AppState {
        db: pool.clone(),
        store: Store::new(pool),
        http: Client::new(),
        credentials: Arc::new(credentials),
        admin_token: Arc::from(admin_token),
    };

    state
        .store
        .recover_interrupted_work()
        .await
        .expect("failed to recover interrupted work");

    let worker_count = std::env::var("SEQUANA_WORKERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(2)
        .max(1);

    scheduler::start(
        state.store.clone(),
        state.db.clone(),
        state.http.clone(),
        state.credentials.clone(),
        worker_count,
    );

    let app = api::router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("failed to bind 0.0.0.0:8080");

    axum::serve(listener, app)
        .await
        .expect("server failed");
}
