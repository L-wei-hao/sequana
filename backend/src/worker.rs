use crate::{
    credentials::CredentialCipher,
    engine::{
        execute::run_execution,
        model::{NodeType, ScheduleConfig},
    },
    store::{JobRecord, Store},
};
use reqwest::Client;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

pub fn start_workers(
    store: Arc<Store>,
    database: PgPool,
    http: Client,
    credentials: Arc<CredentialCipher>,
    workers: usize,
) {
    for worker_id in 0..workers.max(1) {
        tokio::spawn(worker_loop(
            worker_id,
            store.clone(),
            database.clone(),
            http.clone(),
            credentials.clone(),
        ));
    }
}

async fn worker_loop(
    worker_id: usize,
    store: Arc<Store>,
    database: PgPool,
    http: Client,
    credentials: Arc<CredentialCipher>,
) {
    loop {
        match store.claim_job().await {
            Ok(Some(job)) => {
                let result = process_job(
                    &job,
                    &store,
                    &database,
                    &http,
                    credentials.as_ref(),
                )
                .await;
                let error = result.as_ref().err().map(String::as_str);

                if let Err(store_error) =
                    store.finish_job(job.id, result.is_ok(), error).await
                {
                    eprintln!(
                        "worker {worker_id}: failed to finish job {}: {store_error:?}",
                        job.id
                    );
                }

                if let Err(error) = result {
                    eprintln!(
                        "worker {worker_id}: job {} attempt {} failed: {}",
                        job.id, job.attempts, error
                    );
                }
            }
            Ok(None) => sleep(Duration::from_millis(500)).await,
            Err(error) => {
                eprintln!("worker {worker_id}: failed to claim job: {error:?}");
                sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn process_job(
    job: &JobRecord,
    store: &Store,
    database: &PgPool,
    http: &Client,
    credentials: &CredentialCipher,
) -> Result<(), String> {
    let workflow = store
        .workflow_version(
            job.tenant_id,
            job.workflow_id,
            job.workflow_version_id,
        )
        .await
        .map_err(|error| format!("failed to load scheduled workflow: {error:?}"))?
        .ok_or_else(|| "scheduled workflow version no longer exists".to_string())?;

    let node = workflow
        .definition
        .nodes
        .iter()
        .find(|node| {
            node.id == job.trigger_node_id && node.node_type == NodeType::Schedule
        })
        .ok_or_else(|| "scheduled trigger node no longer exists".to_string())?;

    let config = ScheduleConfig::parse(&node.config)?;
    let execution = store
        .create_execution(
            job.tenant_id,
            job.workflow_id,
            job.workflow_version_id,
            "schedule",
            &job.trigger_node_id,
            &config.input,
            None,
            None,
        )
        .await
        .map_err(|error| format!("failed to create scheduled execution: {error:?}"))?;

    store
        .attach_job_execution(job.id, execution.id)
        .await
        .map_err(|error| format!("failed to attach scheduled execution: {error:?}"))?;

    run_execution(
        store,
        database,
        http,
        credentials,
        job.tenant_id,
        execution.id,
        &workflow.definition,
        &job.trigger_node_id,
        config.input,
    )
    .await?;

    Ok(())
}
