//! Bounded operator probe for replaying one retained production snapshot.
//!
//! The probe accepts only a database named `cadus2_prodrestore_*` on the local
//! disposable test cluster. It takes the sole tenant through the normal locked
//! store projection path twice and refuses to run while a model API key is set.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use cadus_core::config::Config;
use cadus_core::curriculum::load_curriculum;
use cadus_core::event::Timestamp;
use cadus_core::projector::{PROJECTOR_VERSION, ProjectionInput};
use cadus_core::retention::report::{IntegratedPerformance, RetentionReport};
use cadus_store::state::{load_events, lock_web_state, project_and_save};
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db, begin_tenant};
use cadus_web::report::report_json;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::types::Uuid;
use sqlx::{PgPool, Row};

const URL_ENV: &str = "CADUS_PRODUCTION_REPLAY_URL";
const DATABASE_PREFIX: &str = "cadus2_prodrestore_";
const FIXED_NOW_US: i64 = 1_788_724_800_000_000;

fn required_url() -> String {
    std::env::var(URL_ENV).unwrap_or_else(|_| panic!("{URL_ENV} is not set"))
}

async fn open_bounded_pool(url: &str) -> PgPool {
    assert!(
        url.contains("@127.0.0.1:55434/cadus2_prodrestore_"),
        "the production replay probe refused a URL outside the disposable local cluster"
    );
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await
        .expect("the disposable restored database opens");
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("the restored database name reads");
    assert!(
        database.starts_with(DATABASE_PREFIX),
        "the production replay probe refused database {database}"
    );
    pool
}

fn assert_model_calls_disabled() {
    for name in ["OPENAI_API_KEY", "ANTHROPIC_API_KEY"] {
        assert!(
            std::env::var(name).unwrap_or_default().is_empty(),
            "the production replay probe refused to run while {name} is set"
        );
    }
}

async fn event_fingerprint(admin: &PgPool) -> String {
    sqlx::query_scalar(
        "SELECT count(*)::text || '|' || COALESCE(max(seq), 0)::text || '|' || \
         COALESCE(md5(string_agg(user_id::text || ':' || seq::text || ':' || v::text || ':' || \
         payload::text, E'\\n' ORDER BY user_id, seq)), md5('')) FROM events",
    )
    .fetch_one(admin)
    .await
    .expect("the event fingerprint reads")
}

async fn stored_model(admin: &PgPool, user: Uuid) -> (Value, i64, i32) {
    let row = sqlx::query(
        "SELECT model, through_seq, projector_version FROM learner_models WHERE user_id = $1",
    )
    .bind(user)
    .fetch_one(admin)
    .await
    .expect("the stored learner model reads");
    (
        row.get("model"),
        row.get("through_seq"),
        row.get("projector_version"),
    )
}

async fn project_and_report(
    app: &PgPool,
    user: Uuid,
    input: &ProjectionInput<'_>,
    cfg: &Config,
) -> (bool, i64, Vec<u8>) {
    let db = Db::new(app.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
    let mut tx = begin_tenant(db.pool(), user)
        .await
        .expect("the tenant transaction starts");
    lock_web_state(&mut tx, user)
        .await
        .expect("the learner state lock is taken");
    let projection = project_and_save(&mut tx, user, input, None)
        .await
        .expect("the restored event stream projects");
    let rows = load_events(&mut tx, user)
        .await
        .expect("the restored event stream reads for its report");
    let events: Vec<_> = rows.into_iter().map(|row| row.event).collect();
    let report = RetentionReport::build(
        &projection.model,
        &cfg.retention,
        &cfg.policy_version,
        IntegratedPerformance::of_events(&events),
    );
    let report = serde_json::to_vec(&report_json(&report, &cfg.policy_digest()))
        .expect("the framework report serializes");
    tx.commit().await.expect("the projection write commits");
    (projection.replayed, projection.through_seq, report)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[tokio::test]
#[ignore = "an operator owns the retained disposable production restore"]
async fn restored_production_account_replays_once_and_resumes_identically() {
    assert_model_calls_disabled();
    let admin_url = required_url();
    let app_url = admin_url.replacen("test:test@", "cadus_app@", 1);
    assert_ne!(
        admin_url, app_url,
        "the app-role URL derives from the admin URL"
    );
    let admin = open_bounded_pool(&admin_url).await;
    let app = open_bounded_pool(&app_url).await;

    let users: Vec<Uuid> =
        sqlx::query_scalar("SELECT DISTINCT user_id FROM events ORDER BY user_id")
            .fetch_all(&admin)
            .await
            .expect("the event tenants read");
    assert_eq!(
        users.len(),
        1,
        "the retained snapshot must hold exactly one event tenant"
    );
    let user = users[0];
    let (_, initial_seq, initial_version) = stored_model(&admin, user).await;
    assert_ne!(initial_version, PROJECTOR_VERSION as i32);

    let before = event_fingerprint(&admin).await;
    let curriculum_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
    let (curriculum, findings) =
        load_curriculum(&curriculum_root).expect("the candidate curriculum loads for replay");
    assert!(
        findings.iter().all(|finding| !finding.fatal),
        "the candidate curriculum has a fatal finding"
    );
    let cfg = Config::default();
    let now = Timestamp::from_micros(FIXED_NOW_US);
    let input = ProjectionInput::new(&curriculum, &cfg, now).with_timezone(cfg.timezone.as_deref());

    let (first_replayed, first_seq, first_report) =
        project_and_report(&app, user, &input, &cfg).await;
    assert!(
        first_replayed,
        "the stale projector marker must force a full replay"
    );
    assert_eq!(first_seq, 36);
    let (first_model, stored_first_seq, stored_first_version) = stored_model(&admin, user).await;
    let first_model = serde_json::to_vec(&first_model).expect("the first model serializes");
    assert_eq!(stored_first_seq, 36);
    assert_eq!(stored_first_version, PROJECTOR_VERSION as i32);
    assert_eq!(event_fingerprint(&admin).await, before);

    let (second_replayed, second_seq, second_report) =
        project_and_report(&app, user, &input, &cfg).await;
    assert!(
        !second_replayed,
        "the second read must resume the version-7 cache"
    );
    assert_eq!(second_seq, first_seq);
    let (second_model, stored_second_seq, stored_second_version) = stored_model(&admin, user).await;
    let second_model = serde_json::to_vec(&second_model).expect("the second model serializes");
    let after = event_fingerprint(&admin).await;

    assert_eq!(second_model, first_model);
    assert_eq!(second_report, first_report);
    assert_eq!(stored_second_seq, stored_first_seq);
    assert_eq!(stored_second_version, stored_first_version);
    assert_eq!(after, before);
    println!(
        "PRODUCTION REPLAY user={user} initial_projector={initial_version} initial_seq={initial_seq} \
         first_replayed={first_replayed} second_replayed={second_replayed} \
         through_seq={second_seq} projector_version={stored_second_version} \
         model_sha256={} report_sha256={} events={after}",
        sha256(&second_model),
        sha256(&second_report),
    );

    app.close().await;
    admin.close().await;
}
