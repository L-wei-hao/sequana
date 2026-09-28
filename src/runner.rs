use crate::{
    credentials::CredentialCipher,
    engine::next_node_ids,
    execution::ExecutionStatus,
    nodes::{execute_node, NodeContext},
    store::Store,
    workflow::WorkflowDefinition,
};
use reqwest::Client;
use serde_json::Value;
use sqlx::{types::Uuid, PgPool};
use std::collections::VecDeque;

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
    workflow.validate()?;

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
        if state_cancelled(store, tenant_id, execution_id).await? {
            return Err("execution cancelled".into());
        }

        // ponytail: O(n) node lookup is fine for V1 graphs; index by id if graph size ever matters.
        let node = workflow
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            .ok_or_else(|| format!("node disappeared during execution: {node_id}"))?;

        let step_id = store
            .start_step(
                execution_id,
                &node.id,
                node.node_type.as_str(),
                &node_input,
            )
            .await
            .map_err(|error| format!("failed to start execution step: {error:?}"))?;

        let result = match execute_node(node, &node_input, &context).await {
            Ok(result) => result,
            Err(error) => {
                let _ = store.finish_step(step_id, None, Some(&error)).await;
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

        if let Err(error) = store.finish_step(step_id, Some(&result.output), None).await {
            let message = format!("failed to finish execution step: {error:?}");
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

        for next_node_id in next {
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
