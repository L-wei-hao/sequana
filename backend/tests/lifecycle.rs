//! These tests never read .env or DATABASE_URL. Each gets an isolated schema.
use sequana::{
    engine::model::{ExecutionStatus, WorkflowDefinition},
    store::Store,
};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

struct Fixture {
    pool: PgPool,
    admin: PgPool,
    schema: String,
    store: Store,
    tenant: Uuid,
    workflow: Uuid,
    version: Uuid,
}
impl Fixture {
    async fn new() -> Option<Self> {
        let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
            eprintln!("SKIPPED lifecycle PostgreSQL test: TEST_DATABASE_URL is not set");
            return None;
        };
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("test PostgreSQL connection");
        let schema = format!("lifecycle_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .unwrap();
        let search_path = format!("SET search_path TO {schema}");
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .after_connect(move |conn, _| {
                let command = search_path.clone();
                Box::pin(async move {
                    sqlx::query(&command).execute(conn).await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let store = Store::new(pool.clone());
        let tenant = Uuid::new_v4();
        let definition = WorkflowDefinition {
            nodes: vec![],
            edges: vec![],
        };
        let (workflow, version) = store
            .create_workflow(tenant, "lifecycle", None, &definition)
            .await
            .unwrap();
        Some(Self {
            pool,
            admin,
            schema,
            store,
            tenant,
            workflow,
            version,
        })
    }
    async fn execution(&self) -> Uuid {
        self.store
            .create_execution(
                self.tenant,
                self.workflow,
                self.version,
                "webhook",
                "trigger",
                &serde_json::json!({}),
                None,
                None,
            )
            .await
            .unwrap()
            .id
    }
    async fn close(self) {
        self.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin)
            .await
            .unwrap();
        self.admin.close().await;
    }
}

#[tokio::test]
async fn cancelled_background_tasks_exit_without_touching_database() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    let store = std::sync::Arc::new(Store::new(pool.clone()));
    store.shutdown_token().cancel();
    let scheduler = sequana::scheduler::start_scheduler(store.clone());
    let cipher = std::sync::Arc::new(
        sequana::credentials::CredentialCipher::from_hex_key(&"00".repeat(32)).unwrap(),
    );
    let workers = sequana::worker::start_workers(store, pool, reqwest::Client::new(), cipher, 2);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        scheduler.await.unwrap();
        for worker in workers {
            worker.await.unwrap();
        }
    })
    .await
    .expect("cancelled tasks must not wait for a database connection");
}

#[tokio::test]
async fn another_instance_recovery_preserves_fresh_execution_and_steps() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let id = f.execution().await;
    assert!(f
        .store
        .transition_execution(
            id,
            ExecutionStatus::Queued,
            ExecutionStatus::Running,
            None,
            None
        )
        .await
        .unwrap());
    f.store
        .start_step(id, "trigger", "webhook", &serde_json::json!({}))
        .await
        .unwrap();
    let other = Store::new(f.pool.clone());
    other.recover_interrupted_work().await.unwrap();
    assert_eq!(
        f.store
            .execution(f.tenant, id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "running"
    );
    assert_eq!(
        f.store.execution_steps(f.tenant, id).await.unwrap()[0].status,
        "running"
    );
    f.close().await;
}

#[tokio::test]
async fn claimed_job_has_a_fresh_owned_lease() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    f.store
        .enqueue_job(
            f.tenant,
            f.workflow,
            f.version,
            "trigger",
            chrono::Utc::now(),
        )
        .await
        .unwrap();
    let job = f.store.claim_job().await.unwrap().unwrap();
    let owned: bool = sqlx::query_scalar(
        "SELECT lease_owner IS NOT NULL AND lease_expires_at > now() FROM jobs WHERE id=$1",
    )
    .bind(job.id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    let other = Store::new(f.pool.clone());
    other.finish_job(job.id, true, None).await.unwrap();
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id=$1")
        .bind(job.id)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    f.close().await;
    assert!(owned, "claimed jobs must have a fresh owner-fenced lease");
    assert_eq!(status, "running", "another owner must not finish a job");
}

#[tokio::test]
async fn another_owner_cannot_write_execution_steps() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let id = f.execution().await;
    f.store
        .transition_execution(
            id,
            ExecutionStatus::Queued,
            ExecutionStatus::Running,
            None,
            None,
        )
        .await
        .unwrap();
    let other = Store::new(f.pool.clone());
    let rejected = other
        .start_step(id, "trigger", "webhook", &serde_json::json!({}))
        .await
        .is_err();
    let step = f
        .store
        .start_step(id, "trigger", "webhook", &serde_json::json!({}))
        .await
        .unwrap();
    let finished = other
        .finish_step(step, Some(&serde_json::json!({})), None, None)
        .await
        .unwrap();
    f.close().await;
    assert!(rejected, "non-owner must not create execution steps");
    assert!(!finished, "non-owner must not finish execution steps");
}

#[tokio::test]
async fn another_owner_cannot_start_a_queued_execution() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let id = f.execution().await;
    let other = Store::new(f.pool.clone());
    let started = other
        .transition_execution(
            id,
            ExecutionStatus::Queued,
            ExecutionStatus::Running,
            None,
            None,
        )
        .await
        .unwrap();
    let owner_started = f
        .store
        .transition_execution(
            id,
            ExecutionStatus::Queued,
            ExecutionStatus::Running,
            None,
            None,
        )
        .await
        .unwrap();
    f.close().await;
    assert!(
        !started,
        "only the queued execution's lease owner may start it"
    );
    assert!(
        owner_started,
        "the original execution owner retains its lease"
    );
}

#[tokio::test]
async fn job_execution_attachment_reports_lost_lease() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    f.store
        .enqueue_job(
            f.tenant,
            f.workflow,
            f.version,
            "trigger",
            chrono::Utc::now(),
        )
        .await
        .unwrap();
    let job = f.store.claim_job().await.unwrap().unwrap();
    let execution = f.execution().await;
    let other = Store::new(f.pool.clone());
    let wrong_owner = other.attach_job_execution(job.id, execution).await;
    sqlx::query("UPDATE jobs SET lease_expires_at=now()-interval '1 second' WHERE id=$1")
        .bind(job.id)
        .execute(&f.pool)
        .await
        .unwrap();
    let expired_owner = f.store.attach_job_execution(job.id, execution).await;
    f.close().await;
    assert!(
        wrong_owner.is_err(),
        "a non-owner must not attach an execution"
    );
    assert!(
        expired_owner.is_err(),
        "an expired lease must not report success"
    );
}

#[tokio::test]
async fn shutdown_refuses_to_start_execution_and_terminalizes_it() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let id = f.execution().await;
    let definition = serde_json::from_value(serde_json::json!({"nodes":[{"id":"trigger","node_type":"manual_trigger","config":{}}],"edges":[]})).unwrap();
    let cipher = sequana::credentials::CredentialCipher::from_hex_key(&"00".repeat(32)).unwrap();
    f.store.shutdown_token().cancel();
    let result = sequana::engine::run_execution(
        &f.store,
        &f.pool,
        &reqwest::Client::new(),
        &cipher,
        f.tenant,
        id,
        &definition,
        "trigger",
        serde_json::json!({}),
    )
    .await;
    let status = f
        .store
        .execution(f.tenant, id)
        .await
        .unwrap()
        .unwrap()
        .status;
    let steps = f.store.execution_steps(f.tenant, id).await.unwrap();
    f.close().await;
    assert!(result.is_err());
    assert_eq!(status, "failed");
    assert!(steps.is_empty());
}

#[tokio::test]
async fn runtime_joins_background_tasks_on_shutdown() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let config = sequana::config::Config {
        database_url: "unused".into(),
        credential_key: "00".repeat(32),
        admin_token: "test-admin-token-with-at-least-32-characters".into(),
        workers: 1,
        max_in_flight_executions: 32,
        bind_address: "127.0.0.1:0".into(),
        default_timezone: "UTC".into(),
        base_url: None,
        webhook_base_url: None,
    };
    let cipher =
        sequana::credentials::CredentialCipher::from_hex_key(&config.credential_key).unwrap();
    let state = sequana::state::AppState::new(f.pool.clone(), config, cipher);
    let shutdown = state.store.shutdown_token();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(sequana::runtime::serve(listener, state));
    let response = reqwest::get(format!("http://{address}/health"))
        .await
        .unwrap();
    assert!(response.status().is_success());
    shutdown.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    f.close().await;
}

#[tokio::test]
async fn runtime_recovers_expired_execution_leases_without_replaying_them() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let id = f.execution().await;
    sqlx::query("UPDATE executions SET lease_expires_at=now()-interval '1 second' WHERE id=$1")
        .bind(id)
        .execute(&f.pool)
        .await
        .unwrap();
    let config = sequana::config::Config {
        database_url: "unused".into(),
        credential_key: "00".repeat(32),
        admin_token: "test-admin-token-with-at-least-32-characters".into(),
        workers: 1,
        max_in_flight_executions: 32,
        bind_address: "127.0.0.1:0".into(),
        default_timezone: "UTC".into(),
        base_url: None,
        webhook_base_url: None,
    };
    let cipher =
        sequana::credentials::CredentialCipher::from_hex_key(&config.credential_key).unwrap();
    let state = sequana::state::AppState::new(f.pool.clone(), config, cipher);
    let shutdown = state.store.shutdown_token();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(sequana::runtime::serve(listener, state));
    let response = reqwest::get(format!("http://{address}/health"))
        .await
        .unwrap();
    assert!(response.status().is_success());
    let recovered = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let record = f.store.execution(f.tenant, id).await.unwrap().unwrap();
            if record.status == "failed" {
                break record;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("recovery loop should terminalize an expired lease");
    shutdown.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    f.close().await;
    assert!(recovered
        .error
        .unwrap()
        .contains("side effects may have occurred"));
}
