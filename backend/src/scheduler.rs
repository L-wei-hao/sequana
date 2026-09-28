use crate::{
    engine::model::{NodeType, ScheduleConfig},
    store::Store,
};
use chrono::Utc;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

pub fn start_scheduler(store: Arc<Store>) {
    tokio::spawn(schedule_loop(store));
}

async fn schedule_loop(store: Arc<Store>) {
    loop {
        if let Err(error) = enqueue_upcoming(&store).await {
            eprintln!("scheduler error: {error}");
        }
        sleep(Duration::from_secs(15)).await;
    }
}

async fn enqueue_upcoming(store: &Store) -> Result<(), String> {
    let workflows = store
        .active_schedule_workflows()
        .await
        .map_err(|error| format!("failed to load active workflows: {error:?}"))?;
    let now = Utc::now();

    for workflow in workflows {
        for node in workflow
            .definition
            .nodes
            .iter()
            .filter(|node| node.node_type == NodeType::Schedule)
        {
            let config = match ScheduleConfig::parse(&node.config) {
                Ok(config) => config,
                Err(error) => {
                    eprintln!(
                        "scheduler: workflow {} node {}: {}",
                        workflow.workflow_id, node.id, error
                    );
                    continue;
                }
            };
            let schedule = config.schedule()?;
            let timezone = config.timezone()?;
            let local_now = now.with_timezone(&timezone);
            let Some(next) = schedule.after(&local_now).next() else {
                continue;
            };

            store
                .enqueue_job(
                    workflow.tenant_id,
                    workflow.workflow_id,
                    workflow.workflow_version_id,
                    &node.id,
                    next.with_timezone(&Utc),
                )
                .await
                .map_err(|error| format!("failed to enqueue scheduled job: {error:?}"))?;
        }
    }

    Ok(())
}
