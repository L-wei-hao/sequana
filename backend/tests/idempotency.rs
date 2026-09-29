//! Idempotency fingerprint integration tests use only the explicit disposable test DB.
use sequana::{
    credentials::CredentialCipher,
    engine::WorkflowDefinition,
    store::{Store, StoreError},
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires a disposable TEST_DATABASE_URL"]
async fn idempotency_key_is_bound_to_the_original_payload() {
    let url = std::env::var("TEST_DATABASE_URL").expect("disposable test database URL");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cipher = Arc::new(CredentialCipher::from_hex_key(&"03".repeat(32)).unwrap());
    let store = Store::with_cipher(pool.clone(), cipher);
    let tenant = Uuid::new_v4();
    let definition = WorkflowDefinition {
        nodes: vec![],
        edges: vec![],
    };
    let (workflow, version) = store
        .create_workflow(tenant, "idempotency fingerprint", None, &definition)
        .await
        .unwrap();

    let first_input = json!({
        "event":"first",
        "counter":1,
        "headers":{"Idempotency-Key":"same-key", "content-type":"application/json"}
    });
    let alias_replay_input = json!({
        "event":"first",
        "counter":1,
        "headers":{"X-Idempotency-Key":"same-key", "content-type":"application/json"}
    });
    let first = store
        .create_execution(
            tenant,
            workflow,
            version,
            "webhook",
            "start",
            &first_input,
            Some("same-key"),
            None,
        )
        .await
        .unwrap();
    let replay = store
        .create_execution(
            tenant,
            workflow,
            version,
            "webhook",
            "start",
            &alias_replay_input,
            Some("same-key"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(first.id, replay.id);
    assert!(!replay.created);

    let changed = json!({"event":"different", "counter":1});
    let conflict = store
        .create_execution(
            tenant,
            workflow,
            version,
            "webhook",
            "start",
            &changed,
            Some("same-key"),
            None,
        )
        .await;
    assert!(matches!(conflict, Err(StoreError::IdempotencyConflict)));

    sqlx::query("UPDATE executions SET lease_expires_at=now()-interval '1 second' WHERE id=$1")
        .bind(first.id)
        .execute(&pool)
        .await
        .unwrap();
    let recovered = store
        .execution_for_replay(tenant, first.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        recovered.status, "failed",
        "expired webhook work must be terminalized, not replayed"
    );
    let retry = store
        .create_execution(
            tenant,
            workflow,
            version,
            "webhook",
            "start",
            &first_input,
            Some("same-key"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(retry.id, first.id);
    assert!(
        !retry.created,
        "a retry after an uncertain crash must not create another execution"
    );

    let fingerprint_len: Option<i32> = sqlx::query_scalar(
        "SELECT octet_length(idempotency_fingerprint) FROM executions WHERE id=$1",
    )
    .bind(first.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(fingerprint_len, Some(32));

    sqlx::query("DELETE FROM workflows WHERE id=$1")
        .bind(workflow)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}
