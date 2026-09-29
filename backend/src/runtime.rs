use crate::{api, scheduler, state::AppState, worker};
use std::{io, time::Duration};
use tokio::net::TcpListener;

const BACKGROUND_TASK_DRAIN_TIMEOUT: Duration = Duration::from_secs(40);
const LEASE_RECOVERY_INTERVAL: Duration = Duration::from_secs(15);

/// Runs HTTP and background services under the store's root cancellation token.
pub async fn serve(listener: TcpListener, state: AppState) -> io::Result<()> {
    let scheduler = scheduler::start_scheduler(state.store.clone());
    let workers = worker::start_workers(
        state.store.clone(),
        state.db.clone(),
        state.http.clone(),
        state.credentials.clone(),
        state.config.workers,
    );
    let shutdown = state.store.shutdown_token();
    let recovery_store = state.store.clone();
    let recovery_shutdown = shutdown.clone();
    let recovery = tokio::spawn(async move {
        let mut interval = tokio::time::interval(LEASE_RECOVERY_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = recovery_shutdown.cancelled() => break,
                _ = interval.tick() => {
                    if let Err(error) = recovery_store.recover_interrupted_work().await {
                        eprintln!("expired-work recovery failed: {error}");
                    }
                }
            }
        }
    });
    let server_result = axum::serve(listener, api::router(state))
        .with_graceful_shutdown(shutdown.clone().cancelled_owned())
        .await;

    // Also stop background services if the HTTP server exits with an error.
    shutdown.cancel();
    let drain = async {
        recovery
            .await
            .map_err(|error| io::Error::other(format!("lease recovery task failed: {error}")))?;
        scheduler
            .await
            .map_err(|error| io::Error::other(format!("scheduler task failed: {error}")))?;
        for worker in workers {
            worker
                .await
                .map_err(|error| io::Error::other(format!("worker task failed: {error}")))?;
        }
        Ok::<(), io::Error>(())
    };
    tokio::time::timeout(BACKGROUND_TASK_DRAIN_TIMEOUT, drain)
        .await
        .map_err(|_| {
            io::Error::new(io::ErrorKind::TimedOut, "background task drain timed out")
        })??;
    server_result
}
