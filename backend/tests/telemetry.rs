use sequana::{engine::WorkflowDefinition, store::Store};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires a disposable TEST_DATABASE_URL"]
async fn step_telemetry_redacts_secrets_without_mutating_runtime_input() {
    let url = std::env::var("TEST_DATABASE_URL").expect("disposable test database URL");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let store = Store::new(pool.clone());
    let tenant = Uuid::new_v4();
    let definition: WorkflowDefinition = serde_json::from_value(json!({
        "nodes": [{"id":"start", "node_type":"manual_trigger", "config":{}}], "edges":[]
    }))
    .unwrap();
    let (workflow, version) = store
        .create_workflow(tenant, "telemetry regression", None, &definition)
        .await
        .unwrap();
    let execution = store
        .create_execution(
            tenant,
            workflow,
            version,
            "manual",
            "start",
            &json!({}),
            None,
            None,
        )
        .await
        .unwrap();
    store
        .transition_execution(
            execution.id,
            sequana::engine::model::ExecutionStatus::Queued,
            sequana::engine::model::ExecutionStatus::Running,
            None,
            None,
        )
        .await
        .unwrap();
    let input = json!({"headers":{"Authorization":"Bearer test-secret","Cookie":"session=test-secret"}, "query":"token=test-secret", "path":"/webhook/test-secret", "body":"opaque body test-secret", "nested":[{"apiKey":"test-secret","password":"test-secret","ok":42}]});
    let step = store
        .start_step(execution.id, "start", "manual_trigger", &input)
        .await
        .unwrap();
    store
        .finish_step(
            step,
            Some(&input),
            Some("provider echoed test-secret"),
            Some(&input),
        )
        .await
        .unwrap();
    let row: (Value, Value, String, Value) =
        sqlx::query_as("SELECT input, output, error, metadata FROM execution_steps WHERE id = $1")
            .bind(step)
            .fetch_one(&pool)
            .await
            .unwrap();
    // Clean fixtures even when the regression assertion fails.
    sqlx::query("DELETE FROM workflows WHERE id=$1")
        .bind(workflow)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(input["nested"][0]["apiKey"], "test-secret");
    assert_eq!(row.0["nested"][0]["ok"], 42);
    assert!(
        !format!("{row:?}").contains("test-secret"),
        "sensitive telemetry reached PostgreSQL"
    );
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires a disposable TEST_DATABASE_URL"]
async fn execution_payloads_are_not_plaintext_telemetry() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cipher = std::sync::Arc::new(
        sequana::credentials::CredentialCipher::from_hex_key(&"01".repeat(32)).unwrap(),
    );
    let store = Store::with_cipher(pool.clone(), cipher);
    let tenant = Uuid::new_v4();
    let definition = WorkflowDefinition {
        nodes: vec![],
        edges: vec![],
    };
    let (workflow, version) = store
        .create_workflow(tenant, "replay regression", None, &definition)
        .await
        .unwrap();
    let input = json!({"password":"private-test-value","path":"/webhook/private-path-value","body":"opaque private-body-value","ok":true});
    let id = store
        .create_execution(
            tenant,
            workflow,
            version,
            "webhook",
            "start",
            &input,
            Some("key"),
            None,
        )
        .await
        .unwrap()
        .id;
    let stored: Value = sqlx::query_scalar("SELECT input FROM executions WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let replay = store
        .execution_for_replay(tenant, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(replay.input, input);
    use sequana::engine::model::ExecutionStatus;
    assert!(store
        .transition_execution(
            id,
            ExecutionStatus::Queued,
            ExecutionStatus::Running,
            None,
            None
        )
        .await
        .unwrap());
    assert!(store
        .transition_execution(
            id,
            ExecutionStatus::Running,
            ExecutionStatus::Succeeded,
            Some(&input),
            None
        )
        .await
        .unwrap());
    assert_eq!(
        store
            .execution_for_replay(tenant, id)
            .await
            .unwrap()
            .unwrap()
            .output,
        Some(input.clone())
    );
    assert!(!store
        .execution(tenant, id)
        .await
        .unwrap()
        .unwrap()
        .output
        .unwrap()
        .to_string()
        .contains("private-test-value"));
    assert!(store
        .execution_for_replay(Uuid::new_v4(), id)
        .await
        .unwrap()
        .is_none());
    sqlx::query("DELETE FROM workflows WHERE id=$1")
        .bind(workflow)
        .execute(&pool)
        .await
        .unwrap();
    assert!(!stored.to_string().contains("private-test-value"));
    assert!(!stored.to_string().contains("private-path-value"));
    assert!(!stored.to_string().contains("private-body-value"));
    pool.close().await;
}
