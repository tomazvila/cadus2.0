//! `GET /api/operator/flags` — the A6 operator view (M5 U12).
//!
//! Requirements: A6 (a fallback is explicit, never silent), C6 (approval binds
//! to the digest), C3 (the row counts run inside `begin_tenant`).
//!
//! Every expected value here is a LITERAL: the status codes, the error codes,
//! the field values of one flag row, and the whole gate note of the fixture
//! template. Nothing is re-read from the code under test.
//!
//! The gate note literal comes from `crates/core/tests/template_gate.rs`,
//! `a_band_constrained_template_is_approvable_and_the_skip_is_recorded`. That
//! test pins the same sentence against `cadus_core::template::gate`, so this
//! file proves the route carries `Verified::notes` to the operator unchanged.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use serde_json::{Value, json};

use common::admin::{
    KEY, admin_get as call, app_without_admin as app, assert_forbidden, seed_account, template_body,
};
use common::{SESSION_TOKEN_ONE, get, send};

/// A second serving key, which the fixture curriculum does NOT name.
const UNKNOWN_KEY: &str = "not-a-topic/kp9";

/// The digest of the approved template of [`KEY`].
const DIGEST: &str = "u12-band-digest";

/// The one note the gate records for the fixture template.
///
/// `crates/core/tests/template_gate.rs` pins the same sentence. Neither corner
/// of the crossed-corner rule satisfies the band, so the gate skips the rule and
/// says so (M4 review 1, finding 22).
const GATE_NOTE: &str = "the crossed-corner rule is skipped for a and b: the constraints admit no \
                         tuple at a=2 with b=9, nor at a=10 with b=1";

/// The count of instances the gate walks for the fixture template.
///
/// `b < a < b + 3` over 1..10 twice admits 17 tuples: 9 with a difference of 1
/// and 8 with a difference of 2.
const INSTANCES_CHECKED: u64 = 17;

/// Write one approved `content_store` template row with the admin pool.
async fn seed_template(db: &TestDb, digest: &str, kp_id: &str, body: &str) {
    sqlx::query(
        "INSERT INTO content_store (digest, kp_id, kind, body, status, approved_at)
         VALUES ($1, $2, 'template', $3::text::jsonb, 'approved', now())",
    )
    .bind(digest)
    .bind(kp_id)
    .bind(body)
    .execute(&db.admin)
    .await
    .unwrap();
}

/// The `flags` row of one serving key.
fn flag_of<'a>(body: &'a Value, kp_id: &str) -> &'a Value {
    body.get("flags")
        .and_then(Value::as_array)
        .expect("the answer carries no flags array")
        .iter()
        .find(|row| row.get("kp_id").and_then(Value::as_str) == Some(kp_id))
        .unwrap_or_else(|| panic!("the answer names no flag row for {kp_id}: {body}"))
}

// --------------------------------------------------------------------------- //
// The two refusals
// --------------------------------------------------------------------------- //

/// A request with no credential is `401 unauthorized`.
#[tokio::test]
async fn a_request_with_no_session_is_unauthorized() {
    TestDb::with(|db| async move {
        let app = app(&db);

        let answer = send(&app, get("/api/operator/flags")).await;

        assert_eq!(answer.status.as_u16(), 401);
        assert_eq!(answer.code(), "unauthorized");
    })
    .await;
}

/// A live session on an account that is not an admin is `403 forbidden`.
///
/// The account exists, the session is live, and the answer still carries no flag
/// row: the A6 view is an operator surface, and a learner never reads it.
#[tokio::test]
async fn a_session_that_is_not_an_admin_is_forbidden() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_account(&db, "u12-learner@example.test", SESSION_TOKEN_ONE.1, false).await;

        let answer = call(&app, "/api/operator/flags").await;

        assert_forbidden(&answer);
        assert!(
            answer.body.get("flags").is_none(),
            "the refusal carried a flags block: {}",
            answer.body
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The answer
// --------------------------------------------------------------------------- //

/// An admin reads the flag row and the gate notes of the approved template.
///
/// The row set of `operator_flags` is every knowledge point that holds a pool
/// row or a template document, so the two seeded templates give two rows.
#[tokio::test]
async fn an_admin_reads_the_flags_and_the_gate_notes() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_account(&db, "u12-admin@example.test", SESSION_TOKEN_ONE.1, true).await;
        seed_template(&db, DIGEST, KEY, &template_body().to_string()).await;

        let answer = call(&app, "/api/operator/flags").await;

        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        let flag = flag_of(&answer.body, KEY);
        assert_eq!(
            flag,
            &json!({
                "kp_id": "band/kp1",
                "approved_templates": 1,
                "pool_depth": 0,
                "last_source": null,
                "last_exemplar_at": null,
                "needs_template": false,
                "source_exhausted": false,
            })
        );
        assert_eq!(
            answer.body.get("gate"),
            Some(&json!([{
                "kp_id": "band/kp1",
                "digest": "u12-band-digest",
                "gated": true,
                "exhaustive": true,
                "instances_checked": INSTANCES_CHECKED,
                "notes": [GATE_NOTE],
            }]))
        );
        assert_eq!(answer.body.get("gate_limit"), Some(&json!(20)));
        assert_eq!(answer.body.get("gate_truncated"), Some(&json!(false)));
    })
    .await;
}

/// A knowledge point the curriculum does not name reports `gated: false`.
///
/// The gate needs the topic's answer kind and the point's exemplars, and a key
/// outside the loaded tree has neither. A6 refuses silence, so the row says the
/// gate did not run and names the reason.
#[tokio::test]
async fn an_unknown_knowledge_point_reports_that_the_gate_did_not_run() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_account(
            &db,
            "u12-admin-unknown@example.test",
            SESSION_TOKEN_ONE.1,
            true,
        )
        .await;
        seed_template(
            &db,
            "u12-unknown-digest",
            UNKNOWN_KEY,
            &template_body().to_string(),
        )
        .await;

        let answer = call(&app, "/api/operator/flags").await;

        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(
            answer.body.get("gate"),
            Some(&json!([{
                "kp_id": "not-a-topic/kp9",
                "digest": "u12-unknown-digest",
                "gated": false,
                "reason": "the curriculum does not name this knowledge point",
                "notes": [],
            }]))
        );
    })
    .await;
}

/// A template the gate refuses reports the rejection, not an empty note list.
///
/// The body below is not a template document at all, so the gate refuses it at
/// its first check. The row that C6 approved is still in `content_store`, and
/// the operator view is where that disagreement becomes visible.
#[tokio::test]
async fn a_refused_template_reports_the_rejection() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_account(
            &db,
            "u12-admin-refused@example.test",
            SESSION_TOKEN_ONE.1,
            true,
        )
        .await;
        seed_template(&db, "u12-broken-digest", KEY, r#"{"v": 1}"#).await;

        let answer = call(&app, "/api/operator/flags").await;

        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        let gate = answer
            .body
            .get("gate")
            .and_then(Value::as_array)
            .expect("the answer carries no gate array");
        assert_eq!(gate.len(), 1, "{}", answer.body);
        assert_eq!(gate[0].get("gated"), Some(&json!(true)));
        assert_eq!(gate[0].get("notes"), Some(&json!([])));
        assert!(
            gate[0].get("rejected").is_some(),
            "a refused template carried no rejection: {}",
            answer.body
        );
    })
    .await;
}

/// The `kp` parameter scopes the whole answer to one serving key.
#[tokio::test]
async fn the_kp_parameter_scopes_the_answer_to_one_key() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_account(
            &db,
            "u12-admin-scope@example.test",
            SESSION_TOKEN_ONE.1,
            true,
        )
        .await;
        seed_template(&db, DIGEST, KEY, &template_body().to_string()).await;
        seed_template(
            &db,
            "u12-other-digest",
            UNKNOWN_KEY,
            &template_body().to_string(),
        )
        .await;

        let all = call(&app, "/api/operator/flags").await;
        let scoped = call(&app, "/api/operator/flags?kp=band/kp1").await;

        assert_eq!(
            all.body
                .get("flags")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            scoped
                .body
                .get("flags")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            scoped
                .body
                .get("gate")
                .and_then(Value::as_array)
                .and_then(|rows| rows.first())
                .and_then(|row| row.get("kp_id")),
            Some(&json!("band/kp1"))
        );
    })
    .await;
}

/// The gate stops at the limit and says so.
///
/// One gate call walks up to `GATE_SAMPLES` instances, so the route bounds the
/// work per request. The 21 seeded templates are one above the limit of 20, and
/// the answer reports the 20 it gated and the flag that says it left one.
#[tokio::test]
async fn the_gate_stops_at_the_limit_and_reports_it() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_account(
            &db,
            "u12-admin-limit@example.test",
            SESSION_TOKEN_ONE.1,
            true,
        )
        .await;
        // The keys are outside the fixture curriculum, so no gate call runs the
        // 4,096-instance walk and the test stays a test. The limit counts rows,
        // not gate calls, so the bound under test is the same one.
        for index in 0..21 {
            seed_template(
                &db,
                &format!("u12-many-digest-{index:02}"),
                &format!("many-{index:02}/kp1"),
                &template_body().to_string(),
            )
            .await;
        }

        let answer = call(&app, "/api/operator/flags").await;

        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(
            answer
                .body
                .get("flags")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(21)
        );
        assert_eq!(
            answer
                .body
                .get("gate")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(20)
        );
        assert_eq!(answer.body.get("gate_truncated"), Some(&json!(true)));
    })
    .await;
}
