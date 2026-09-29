use sequana::{config::Config, credentials::CredentialCipher, state::AppState};
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

    let listener = tokio::net::TcpListener::bind(&config.bind_address)
        .await
        .unwrap_or_else(|err| panic!("failed to bind {}: {}", config.bind_address, err));

    println!("Sequana server running on http://{}", config.bind_address);

    let shutdown = state.store.shutdown_token();
    let signal = tokio::spawn(async move {
        shutdown_signal().await;
        shutdown.cancel();
    });
    sequana::runtime::serve(listener, state)
        .await
        .expect("server or background service encountered a fatal error");
    signal.abort();
}

#[cfg(unix)]
async fn shutdown_signal() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to install SIGTERM handler");
    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            if let Err(error) = result { eprintln!("failed to listen for Ctrl-C: {error}"); }
        }
        _ = terminate.recv() => {}
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("failed to listen for Ctrl-C: {error}");
    }
}
