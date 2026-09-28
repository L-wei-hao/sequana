use sequana::{
    api,
    config::Config,
    credentials::CredentialCipher,
    scheduler,
    state::AppState,
    worker,
};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let config = Config::from_env().unwrap_or_else(|err| {
        eprintln!("Configuration error: {err}");
        std::process::exit(1);
    });

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&config.database_url)
        .await
        .expect("failed to connect to PostgreSQL");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run database migrations");

    let cipher = CredentialCipher::from_hex_key(&config.credential_key)
        .expect("SEQUANA_CREDENTIAL_KEY is invalid");

    let state = AppState::new(pool, config.clone(), cipher);

    state
        .store
        .recover_interrupted_work()
        .await
        .expect("failed to recover interrupted work");

    scheduler::start_scheduler(state.store.clone());

    worker::start_workers(
        state.store.clone(),
        state.db.clone(),
        state.http.clone(),
        state.credentials.clone(),
        config.workers,
    );

    let app = api::router(state);

    let listener = tokio::net::TcpListener::bind(&config.bind_address)
        .await
        .unwrap_or_else(|err| panic!("failed to bind {}: {}", config.bind_address, err));

    println!("Sequana server running on http://{}", config.bind_address);

    axum::serve(listener, app)
        .await
        .expect("server encountered fatal error");
}
