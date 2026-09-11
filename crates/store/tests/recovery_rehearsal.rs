//! Disposable snapshot replay and process-stop recovery evidence.
//!
//! `scripts/check_recovery.sh` runs these ignored probes against databases whose
//! names start with `cadus2_recovery_`. The probes refuse all other databases.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

mod common;

use std::time::Duration;

use cadus_core::event::{Event, SchemaVersion, Timestamp};
use cadus_core::projector::{PROJECTOR_VERSION, ProjectionInput, blob_digest};
use cadus_store::state::{append_event, lock_web_state, project_and_save};
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db, begin_tenant};
use common::events::{BASE_US, Fixture, attempt_row, end, review, start};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use uuid::Uuid;

const EMPTY_USER: &str = "71000000-0000-0000-0000-000000000001";
const ORDINARY_USER: &str = "71000000-0000-0000-0000-000000000002";
const REVIEW_USER: &str = "71000000-0000-0000-0000-000000000003";
const INTEGRATED_USER: &str = "71000000-0000-0000-0000-000000000004";
const LARGE_USER: &str = "71000000-0000-0000-0000-000000000005";
const USER_IDS: [&str; 5] = [
    EMPTY_USER,
    ORDINARY_USER,
    REVIEW_USER,
    INTEGRATED_USER,
    LARGE_USER,
];

struct Pools {
    admin: PgPool,
    app: PgPool,
}

impl Pools {
    async fn open() -> Self {
        let admin_url = required_url("CADUS_RECOVERY_ADMIN_URL");
        let app_url = required_url("CADUS_RECOVERY_APP_URL");
        assert_safe_url(&admin_url);
        assert_safe_url(&app_url);
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect(&admin_url)
            .await
            .expect("the admin pool opens");
        let app = PgPoolOptions::new()
            .max_connections(2)
            .connect(&app_url)
            .await
            .expect("the app pool opens");
        let database: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&admin)
            .await
            .expect("the database name reads");
        assert!(database.starts_with("cadus2_recovery_"));
        Self { admin, app }
    }

    async fn close(self) {
        self.app.close().await;
        self.admin.close().await;
    }
}

fn required_url(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

fn assert_safe_url(url: &str) {
    assert!(
        url.contains("@127.0.0.1:55434/cadus2_recovery_"),
        "the recovery probe refused a URL outside the disposable test cluster"
    );
}

fn user_id(text: &str) -> Uuid {
    Uuid::parse_str(text).expect("the fixture user id parses")
}

fn input(fixture: &Fixture) -> ProjectionInput<'_> {
    ProjectionInput::new(
        &fixture.graph,
        &fixture.cfg,
        Timestamp::from_micros(BASE_US + 86_400_000_000),
    )
}

fn integrated_attempt() -> Event {
    let value = serde_json::json!({
        "type": "integrated_attempt",
        "ts": "2026-01-02T00:00:00Z",
        "session": "recovery-integrated",
        "v": SchemaVersion::current(),
        "attempt_id": "recovery-integrated-1",
        "task_id": "recovery-integrated-task",
        "item_id": "recovery-item",
        "item_digest": "recovery-item-digest",
        "topic": "addition",
        "steps": [],
        "final_field": {
            "id": "final",
            "answer": "3",
            "contract": {"kind": "exact"},
            "outcome": "correct",
            "assisted": false,
            "skills": ["addition/kp1"]
        },
        "skills_credited": ["addition/kp1"],
        "solved": true,
        "assisted": false,
        "instruction_kp": "addition/kp1"
    });
    Event::from_json(&value.to_string()).expect("the integrated fixture event reads")
}

fn ordinary_events(session: &str, task: &str) -> Vec<(Event, Option<String>)> {
    vec![
        (start(session), None),
        (
            attempt_row(
                Timestamp::from_micros(BASE_US + 1),
                session,
                task,
                "addition",
                &format!("{task}-attempt"),
                "Compute $8 - 5$.".to_string(),
                "3".to_string(),
            ),
            Some(format!("{task}-attempt")),
        ),
        (end(session), None),
    ]
}

fn large_events() -> Vec<(Event, Option<String>)> {
    let mut events = vec![(start("recovery-large"), None)];
    for index in 0..256 {
        let attempt_id = format!("recovery-large-{index:04}");
        events.push((
            attempt_row(
                Timestamp::from_micros(BASE_US + i64::from(index) + 1),
                "recovery-large",
                "recovery-large-task",
                "addition",
                &attempt_id,
                format!("Compute $8 - 5$ for row {index}."),
                "3".to_string(),
            ),
            Some(attempt_id),
        ));
    }
    events.push((end("recovery-large"), None));
    events
}

async fn insert_user(admin: &PgPool, id: Uuid, label: &str) {
    sqlx::query("INSERT INTO users (id, email, email_verified_at) VALUES ($1, $2, now())")
        .bind(id)
        .bind(format!("recovery+{label}@example.test"))
        .execute(admin)
        .await
        .expect("the recovery user inserts");
}

async fn projection_transaction(
    pools: &Pools,
    id: Uuid,
) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let handle = Db::new(pools.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
    let mut tx = begin_tenant(handle.pool(), id)
        .await
        .expect("the tenant transaction starts");
    lock_web_state(&mut tx, id)
        .await
        .expect("the recovery lock is taken");
    tx
}

async fn seed_one(pools: &Pools, id: Uuid, events: Vec<(Event, Option<String>)>) {
    let fixture = Fixture::micro();
    let mut tx = projection_transaction(pools, id).await;
    for (event, attempt_id) in &events {
        append_event(&mut tx, id, event, attempt_id.as_deref())
            .await
            .expect("the recovery event appends");
    }
    project_and_save(&mut tx, id, &input(&fixture), Some("recovery-fixture-v1"))
        .await
        .expect("the initial model saves");
    tx.commit().await.expect("the seed transaction commits");
}

async fn project_user(pools: &Pools, id: Uuid, expect_replay: bool) -> (i64, String) {
    let fixture = Fixture::micro();
    let mut tx = projection_transaction(pools, id).await;
    let projection = project_and_save(&mut tx, id, &input(&fixture), Some("recovery-fixture-v1"))
        .await
        .expect("the recovery projection saves");
    assert_eq!(projection.replayed, expect_replay);
    let digest = blob_digest(&projection.model).expect("the model digest builds");
    tx.commit().await.expect("the recovery projection commits");
    (projection.through_seq, digest)
}

#[tokio::test]
#[ignore = "scripts/check_recovery.sh owns the disposable database"]
async fn seed_source_snapshot() {
    let pools = Pools::open().await;
    for (index, id) in USER_IDS.iter().enumerate() {
        insert_user(&pools.admin, user_id(id), &format!("sample-{index}")).await;
    }
    seed_one(&pools, user_id(EMPTY_USER), Vec::new()).await;
    seed_one(
        &pools,
        user_id(ORDINARY_USER),
        ordinary_events("recovery-ordinary", "recovery-ordinary-task"),
    )
    .await;
    let mut review_events = ordinary_events("recovery-review", "recovery-review-task");
    review_events.insert(2, (review(Timestamp::from_micros(BASE_US + 2), 12.0), None));
    seed_one(&pools, user_id(REVIEW_USER), review_events).await;
    seed_one(
        &pools,
        user_id(INTEGRATED_USER),
        vec![
            (start("recovery-integrated"), None),
            (
                integrated_attempt(),
                Some("recovery-integrated-1".to_string()),
            ),
            (end("recovery-integrated"), None),
        ],
    )
    .await;
    seed_one(&pools, user_id(LARGE_USER), large_events()).await;
    let changed = sqlx::query("UPDATE learner_models SET projector_version = 6")
        .execute(&pools.admin)
        .await
        .expect("the old projector marker writes")
        .rows_affected();
    assert_eq!(changed, 5);
    println!("RECOVERY SEED users=5 events=268 largest=258 old_projector=6");
    pools.close().await;
}

#[tokio::test]
#[ignore = "scripts/check_recovery.sh stops this process"]
async fn stop_during_projector_transaction() {
    let pools = Pools::open().await;
    let fixture = Fixture::micro();
    let id = user_id(LARGE_USER);
    let mut tx = projection_transaction(&pools, id).await;
    let projection = project_and_save(&mut tx, id, &input(&fixture), Some("recovery-fixture-v1"))
        .await
        .expect("the uncommitted projection saves");
    assert!(projection.replayed);
    assert_eq!(projection.through_seq, 258);

    let marker =
        std::env::var("CADUS_RECOVERY_STOP_MARKER").expect("CADUS_RECOVERY_STOP_MARKER is set");
    assert!(marker.starts_with("/home/deploy/.cache/cadus2_recovery_"));
    let mut file = std::fs::File::create(&marker).expect("the stop marker creates");
    use std::io::Write as _;
    writeln!(file, "{}", std::process::id()).expect("the process id writes");
    file.sync_all().expect("the stop marker syncs");
    tokio::time::sleep(Duration::from_secs(600)).await;
    tx.rollback()
        .await
        .expect("the paused transaction rolls back");
    panic!("the recovery script did not stop the probe process");
}

#[tokio::test]
#[ignore = "scripts/check_recovery.sh owns the disposable database"]
async fn retry_replays_all_samples_and_commits() {
    let pools = Pools::open().await;
    let mut counts = Vec::new();
    let mut digests = Vec::new();
    for id in USER_IDS {
        let (count, digest) = project_user(&pools, user_id(id), true).await;
        counts.push(count);
        digests.push(digest);
    }
    assert_eq!(counts, [0, 3, 4, 3, 258]);
    assert_eq!(digests.len(), 5);
    let versions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learner_models WHERE projector_version = $1")
            .bind(i32::try_from(PROJECTOR_VERSION).unwrap())
            .fetch_one(&pools.admin)
            .await
            .expect("the projector versions read");
    assert_eq!(versions, 5);
    println!(
        "RECOVERY REPLAY users=5 through_seq=0,3,4,3,258 projector_version={PROJECTOR_VERSION}"
    );
    pools.close().await;
}

#[tokio::test]
#[ignore = "scripts/check_recovery.sh owns the disposable database"]
async fn second_read_resumes_with_identical_models() {
    let pools = Pools::open().await;
    for id in USER_IDS {
        let user = user_id(id);
        let before: serde_json::Value =
            sqlx::query("SELECT model FROM learner_models WHERE user_id = $1")
                .bind(user)
                .fetch_one(&pools.admin)
                .await
                .expect("the saved model reads")
                .get("model");
        let (_, digest) = project_user(&pools, user, false).await;
        let after: serde_json::Value =
            sqlx::query("SELECT model FROM learner_models WHERE user_id = $1")
                .bind(user)
                .fetch_one(&pools.admin)
                .await
                .expect("the resumed model reads")
                .get("model");
        assert_eq!(before, after);
        assert_eq!(digest.len(), 64);
    }
    println!("RECOVERY RESUME users=5 model_json=identical replayed=false");
    pools.close().await;
}
