use crate::{
    credentials::CredentialCipher,
    engine::model::{ExecutionStatus, NodeResult, WorkflowDefinition},
    nodes::{execute_node, NodeContext},
    store::Store,
};
use reqwest::Client;
use serde_json::Value;
use sqlx::{types::Uuid, PgPool};
use std::collections::VecDeque;

pub fn next_node_ids(
    workflow: &WorkflowDefinition,
    node_id: &str,
    result: &NodeResult,
) -> Result<Vec<String>, String> {
    if !workflow.nodes.iter().any(|node| node.id == node_id) {
        return Err(format!("node does not exist: {node_id}"));
    }

    let outgoing: Vec<_> = workflow
        .edges
        .iter()
        .filter(|edge| edge.source == node_id)
        .collect();

    let exact: Vec<String> = result
        .route
        .as_deref()
        .map(|route| {
            outgoing
                .iter()
                .filter(|edge| edge.route.as_deref() == Some(route))
                .map(|edge| edge.target.clone())
                .collect()
        })
        .unwrap_or_default();

    if !exact.is_empty() {
        return Ok(exact);
    }

    Ok(outgoing
        .iter()
        .filter(|edge| edge.route.is_none())
        .map(|edge| edge.target.clone())
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub async fn run_execution(
    store: &Store,
    database: &PgPool,
    http: &Client,
    credentials: &CredentialCipher,
    tenant_id: Uuid,
    execution_id: Uuid,
    workflow: &WorkflowDefinition,
    start_node_id: &str,
    input: Value,
) -> Result<Value, String> {
    let shutdown = store.shutdown_token();
    if shutdown.is_cancelled() {
        store
            .interrupt_execution(execution_id)
            .await
            .map_err(|_| "failed to persist interruption")?;
        return Err("server is shutting down".into());
    }
    let result = {
        let execution = run_execution_inner(
            store,
            database,
            http,
            credentials,
            tenant_id,
            execution_id,
            workflow,
            start_node_id,
            input,
        );
        tokio::pin!(execution);
        let grace = async {
            shutdown.cancelled().await;
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        };
        tokio::pin!(grace);
        let period = std::time::Duration::from_secs(20);
        let mut heartbeat = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        loop {
            tokio::select! {
                result = &mut execution => break result,
                _ = &mut grace => break Err("shutdown grace expired; outcome uncertain".into()),
                _ = heartbeat.tick() => {
                    match store.heartbeat_execution(execution_id).await {
                        Ok(true) => {},
                        _ => break Err("execution lease lost; outcome uncertain".into()),
                    }
                }
            }
        }
    }; // Drop the node future before persisting an uncertain outcome.
    if result.is_err() {
        store
            .interrupt_execution(execution_id)
            .await
            .map_err(|_| "failed to persist execution interruption")?;
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn run_execution_inner(
    store: &Store,
    database: &PgPool,
    http: &Client,
    credentials: &CredentialCipher,
    tenant_id: Uuid,
    execution_id: Uuid,
    workflow: &WorkflowDefinition,
    start_node_id: &str,
    input: Value,
) -> Result<Value, String> {
    let start = workflow
        .nodes
        .iter()
        .find(|node| node.id == start_node_id)
        .ok_or_else(|| format!("start node does not exist: {start_node_id}"))?;

    if !start.node_type.is_trigger() {
        return Err(format!("start node is not a trigger: {start_node_id}"));
    }

    let started = store
        .transition_execution(
            execution_id,
            ExecutionStatus::Queued,
            ExecutionStatus::Running,
            None,
            None,
        )
        .await
        .map_err(|error| format!("failed to start execution: {error:?}"))?;

    if !started {
        return Err("execution is no longer queued".into());
    }

    let context = NodeContext {
        db: database,
        http,
        store,
        tenant_id,
        credentials,
    };
    let mut queue = VecDeque::from([(start_node_id.to_string(), input)]);
    let mut final_outputs = Vec::new();

    while let Some((node_id, node_input)) = queue.pop_front() {
        if store.shutdown_token().is_cancelled() {
            return Err("execution interrupted by shutdown".into());
        }
        if state_cancelled(store, tenant_id, execution_id).await? {
            return Err("execution cancelled".into());
        }

        let node = workflow
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            .ok_or_else(|| format!("node disappeared during execution: {node_id}"))?;

        let step_id = store
            .start_step(execution_id, &node.id, node.node_type.as_str(), &node_input)
            .await
            .map_err(|error| format!("failed to start execution step: {error:?}"))?;

        let result = match execute_node(node, &node_input, &context).await {
            Ok(result) => result,
            Err(error) => {
                let _ = store.finish_step(step_id, None, Some(&error), None).await;
                let _ = store
                    .transition_execution(
                        execution_id,
                        ExecutionStatus::Running,
                        ExecutionStatus::Failed,
                        None,
                        Some(&error),
                    )
                    .await;
                return Err(error);
            }
        };

        let persisted = store
            .finish_step(
                step_id,
                Some(&result.output),
                None,
                result.metadata.as_ref(),
            )
            .await;
        if !matches!(persisted, Ok(true)) {
            let message = "failed to persist execution step; lease or state changed".to_string();
            let _ = store
                .transition_execution(
                    execution_id,
                    ExecutionStatus::Running,
                    ExecutionStatus::Failed,
                    None,
                    Some(&message),
                )
                .await;
            return Err(message);
        }

        if state_cancelled(store, tenant_id, execution_id).await? {
            return Err("execution cancelled".into());
        }

        let next = match next_node_ids(workflow, &node.id, &result) {
            Ok(next) => next,
            Err(error) => {
                fail_execution(store, execution_id, &error).await;
                return Err(error);
            }
        };

        if next.is_empty() {
            final_outputs.push(result.output);
            continue;
        }

        let mut next = next.into_iter().peekable();
        while let Some(next_node_id) = next.next() {
            if next.peek().is_none() {
                queue.push_back((next_node_id, result.output));
                break;
            }
            queue.push_back((next_node_id, result.output.clone()));
        }
    }

    let output = if final_outputs.len() == 1 {
        final_outputs.pop().expect("checked length")
    } else {
        Value::Array(final_outputs)
    };

    let completed = store
        .transition_execution(
            execution_id,
            ExecutionStatus::Running,
            ExecutionStatus::Succeeded,
            Some(&output),
            None,
        )
        .await
        .map_err(|error| format!("failed to complete execution: {error:?}"))?;

    if !completed {
        if state_cancelled(store, tenant_id, execution_id).await? {
            return Err("execution cancelled".into());
        }
        return Err("execution state changed before completion".into());
    }

    Ok(output)
}

async fn state_cancelled(
    store: &Store,
    tenant_id: Uuid,
    execution_id: Uuid,
) -> Result<bool, String> {
    store
        .is_execution_cancelled(tenant_id, execution_id)
        .await
        .map_err(|error| format!("failed to read execution state: {error:?}"))
}

async fn fail_execution(store: &Store, execution_id: Uuid, error: &str) {
    let _ = store
        .transition_execution(
            execution_id,
            ExecutionStatus::Running,
            ExecutionStatus::Failed,
            None,
            Some(error),
        )
        .await;
}
