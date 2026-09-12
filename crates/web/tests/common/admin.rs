//! The fixtures of `tests/admin_content.rs` and its parts.

use cadus_core::curriculum::load::{RawCurriculum, RawUnit};
use cadus_core::curriculum::model::{Catalog, Course, Exemplar, KnowledgePoint, Slug, Topic, Unit};
use cadus_core::curriculum::{AnswerKind, Curriculum};
use cadus_core::review_engine;
use cadus_store::content::{CurrentContext, template_review_context};
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::Content;
use cadus_web::{AppState, create_app};
use sqlx::types::chrono::{DateTime, Utc};

pub use super::prelude::*;
use super::*;

/// The serving key of the fixture knowledge point.
pub const KEY: &str = "band/kp1";

/// A second serving key. The fixture curriculum does not name it, and the queue
/// read needs no curriculum.
pub const OTHER_KEY: &str = "band/kp2";

/// The digest of the pending template under review.
pub const PENDING: &str = "r5-pending-digest";

/// A digest no row carries.
pub const ABSENT: &str = "r5-no-such-digest";

/// The four paths of spec section 3.2, with [`PENDING`] in the digest segment.
pub const LIST_PATH: &str = "/api/admin/content";
pub const SHOW_PATH: &str = "/api/admin/content/r5-pending-digest";
pub const APPROVE_PATH: &str = "/api/admin/content/r5-pending-digest/approve";
pub const REJECT_PATH: &str = "/api/admin/content/r5-pending-digest/reject";

/// The eight instances the show route renders for [`template_body`].
///
/// The draw runs from the constant seed `cadus_core::template::GATE_SEED`, so
/// the list is a function of the document alone and two reviewers of one digest
/// read the same eight problems. Each answer is the difference the statement
/// asks for, and the pair is pinned here character for character.
pub const INSTANCES: [(&str, &str); 8] = [
    ("Compute $9 - 7$.", "2"),
    ("Compute $4 - 2$.", "2"),
    ("Compute $5 - 3$.", "2"),
    ("Compute $6 - 5$.", "1"),
    ("Compute $3 - 2$.", "1"),
    ("Compute $4 - 3$.", "1"),
    ("Compute $10 - 9$.", "1"),
    ("Compute $3 - 1$.", "2"),
];

/// The difference that one pinned statement asks for, read out of the statement
/// itself.
///
/// The function is the test's OWN arithmetic. It proves that each pinned answer
/// is the answer of its pinned problem, so the eight pairs above are a contract
/// and not a snapshot of whatever the code produced.
pub fn difference_of(text: &str) -> String {
    let inner = text.trim_start_matches("Compute $").trim_end_matches("$.");
    let (left, right) = inner.split_once(" - ").expect("the statement has no minus");
    let left: i64 = left.parse().expect("the left side is not a number");
    let right: i64 = right.parse().expect("the right side is not a number");
    (left - right).to_string()
}

/// The band template of the fixture knowledge point, as one document body.
///
/// The document is the fixture of `crates/web/tests/operator_flags.rs`, so the
/// gate verdict this file reads is the verdict that file already pins.
pub fn template_body() -> Value {
    json!({
        "v": 1,
        "topic_id": "band",
        "answer_kind": "numeric",
        "statement": "Compute ${a} - {b}$.",
        "params": {
            "a": {"kind": "int", "low": 1, "high": 10},
            "b": {"kind": "int", "low": 1, "high": 10}
        },
        "constraints": [
            {"op": "gt", "left": "a", "right": "b"},
            {"op": "lt", "left": "a", "right": {"add": ["b", {"lit": 3}]}}
        ],
        "answer_expr": "a - b",
        "solution_sketch": "Take ${b}$ from ${a}$.",
        "hints": ["Which number is larger?"],
        "samples": [
            {"params": {"a": 2, "b": 1}, "expected": "1"},
            {"params": {"a": 10, "b": 9}, "expected": "1"},
            {"params": {"a": 3, "b": 1}, "expected": "2"},
            {"params": {"a": 10, "b": 8}, "expected": "2"}
        ]
    })
}

// --------------------------------------------------------------------------- //
// The harness
// --------------------------------------------------------------------------- //

/// The fixture curriculum: one course, one topic `band`, one knowledge point.
pub fn graph() -> Curriculum {
    let mut point = kp(
        "kp1",
        vec![Exemplar {
            answer_contract: None,
            problem: "Compute $7 - 2$.".to_string(),
            answer: "5".to_string(),
            solution_sketch: None,
        }],
    );
    point.name = "The first point".to_string();
    one_unit_curriculum(vec![topic("band", vec![point])])
}

/// The router of a test, with the fixture curriculum and the admin path.
///
/// The admin path is the superuser pool of the throwaway database, which is the
/// production `cadus_admin` role in every way this unit reads: it holds the
/// INSERT, UPDATE, and DELETE that `cadus_app` does not.
pub fn app(db: &TestDb) -> Router {
    create_app(
        state_with_content(db, graph())
            .with_admin(Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS)),
    )
}

/// The same router with NO admin path, which is the default of a deployment
/// that sets no `CADUS_ADMIN_DATABASE_URL`.
pub fn app_without_admin(db: &TestDb) -> Router {
    app_with_content(db, graph())
}

/// Seed one account with a live session on `token_hash`, and return its id.
pub async fn seed_account(db: &TestDb, email: &str, token_hash: &str, admin: bool) -> Uuid {
    let user = db.seed_user(email).await;
    seed_session(
        db,
        user,
        token_hash,
        shift(-60),
        shift(-60),
        shift(2_592_000),
    )
    .await;
    if admin {
        sqlx::query("UPDATE users SET is_admin = true WHERE id = $1")
            .bind(user)
            .execute(&db.admin)
            .await
            .unwrap();
    }
    user
}

/// Seed the admin account of a test.
pub async fn seed_admin(db: &TestDb) -> Uuid {
    seed_account(db, "r5-admin@example.test", SESSION_TOKEN_ONE.1, true).await
}

/// One `content_store` row a test seeds.
pub struct Seed<'a> {
    pub digest: &'a str,
    pub kp_id: &'a str,
    pub kind: &'a str,
    pub status: &'a str,
    pub body: Value,
    pub attempts: i32,
    pub cost: Option<&'a str>,
    /// When set, overrides the fixture curriculum digest.
    pub curriculum_digest: Option<&'a str>,
    /// When set, overrides the build-time review engine digest.
    pub review_engine_digest: Option<&'a str>,
}

impl<'a> Seed<'a> {
    /// One template row of [`template_body`], with one attempt and no bill.
    pub fn template(digest: &'a str, kp_id: &'a str, status: &'a str) -> Self {
        Self {
            digest,
            kp_id,
            kind: "template",
            status,
            body: template_body(),
            attempts: 1,
            cost: None,
            curriculum_digest: None,
            review_engine_digest: None,
        }
    }
}

/// Write one `content_store` row with the superuser pool.
pub async fn seed_row(db: &TestDb, seed: &Seed<'_>) {
    let cur_digest = seed.curriculum_digest.map_or_else(
        fixture_curriculum_digest,
        |d| d.to_owned(),
    );
    let eng_digest = seed.review_engine_digest.map_or_else(
        || fixture_review_engine_digest().to_owned(),
        |d| d.to_owned(),
    );
    sqlx::query(
        "INSERT INTO content_store
            (digest, kp_id, kind, body, status, authoring_attempts, authoring_cost_usd,
             approved_curriculum_digest, approved_review_engine_digest)
         VALUES ($1, $2, $3, $4::text::jsonb, $5, $6, $7::text::numeric, $8, $9)",
    )
    .bind(seed.digest)
    .bind(seed.kp_id)
    .bind(seed.kind)
    .bind(seed.body.to_string())
    .bind(seed.status)
    .bind(seed.attempts)
    .bind(seed.cost)
    .bind(&cur_digest)
    .bind(&eng_digest)
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Write the pending template under review, with its T3 bill.
pub async fn seed_pending(db: &TestDb) {
    let mut seed = Seed::template(PENDING, KEY, "pending");
    seed.attempts = 4;
    seed.cost = Some("0.012500");
    seed_row(db, &seed).await;
}

/// Write `count` approved templates on `kp_id`, with a digest of `prefix<n>`.
pub async fn seed_approved(db: &TestDb, kp_id: &str, prefix: &str, count: usize) {
    for n in 0..count {
        let digest = format!("{prefix}{n}");
        seed_row(db, &Seed::template(&digest, kp_id, "approved")).await;
    }
}

/// The `status`, `review_reason`, `approved_by`, and `approved_at` of one row.
pub async fn row_state(db: &TestDb, digest: &str) -> (String, Option<String>, Option<Uuid>, bool) {
    let row: (String, Option<String>, Option<Uuid>, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT status, review_reason, approved_by, approved_at
             FROM content_store WHERE digest = $1",
    )
    .bind(digest)
    .fetch_one(&db.admin)
    .await
    .unwrap();
    (row.0, row.1, row.2, row.3.is_some())
}

/// A `GET` that presents the admin session token.
pub async fn admin_get(app: &Router, uri: &str) -> Answer {
    send(app, get_bearer(uri, SESSION_TOKEN_ONE.0)).await
}

/// A `POST` that presents the admin session token.
pub async fn admin_post(app: &Router, uri: &str, body: &Value) -> Answer {
    send(app, post_bearer(uri, SESSION_TOKEN_ONE.0, body)).await
}

/// The queue line of one digest.
pub fn item_of<'a>(body: &'a Value, digest: &str) -> &'a Value {
    body.get("items")
        .and_then(Value::as_array)
        .expect("the answer carries no items array")
        .iter()
        .find(|item| item.get("digest").and_then(Value::as_str) == Some(digest))
        .unwrap_or_else(|| panic!("the queue names no line for {digest}: {body}"))
}

/// Fail the test when `answer` is not the `403 forbidden` a learner gets on
/// an admin route.
pub fn assert_forbidden(answer: &Answer) {
    assert_eq!(answer.status.as_u16(), 403, "{}", answer.body);
    assert_eq!(answer.code(), "forbidden");
    assert_eq!(
        answer
            .body
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str),
        Some("This route serves an admin account only.")
    );
}

/// Fail the test when `answer` is not the `422 invalid_request` that carries
/// `message`.
pub fn assert_invalid_request(answer: &Answer, message: &str) {
    assert_eq!(answer.status.as_u16(), 422, "{}", answer.body);
    assert_eq!(answer.code(), "invalid_request");
    assert_eq!(
        answer
            .body
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str),
        Some(message)
    );
}

/// Seed the admin and the pending template, and read the show route of the
/// pending digest as the admin. The answer is the `200`.
pub async fn show_pending(db: &TestDb) -> Answer {
    let app = app(db);
    seed_admin(db).await;
    seed_pending(db).await;
    let answer = admin_get(&app, SHOW_PATH).await;
    assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
    answer
}

/// Seed the admin and the pending template, and read the queue as the admin.
pub async fn list_after_pending(db: &TestDb) -> Answer {
    let app = app(db);
    seed_admin(db).await;
    seed_pending(db).await;
    admin_get(&app, LIST_PATH).await
}

/// The checks of an instruction document on the show route: its `kind`, no
/// instance, no gate, and `summary`.
pub fn assert_instruction_document(answer: &Answer, kind: &str, summary: &str) {
    assert_eq!(answer.body.get("kind"), Some(&json!(kind)));
    assert_eq!(answer.body.get("instances"), Some(&json!([])));
    assert_eq!(answer.body.get("gate"), Some(&Value::Null));
    assert_eq!(answer.body.get("summary"), Some(&json!(summary)));
}

/// The curriculum digest of the fixture graph.
pub fn fixture_curriculum_digest() -> String {
    Content::new(graph())
        .curriculum_context_digest()
        .expect("the fixture curriculum produces a digest")
        .to_owned()
}

/// The build-time review engine digest.
pub fn fixture_review_engine_digest() -> &'static str {
    review_engine::DIGEST
}

/// Compute the JSON body for a template approval POST with the fixture context.
pub async fn fixture_approve_body(db: &TestDb, digest: &str) -> Value {
    approve_body_inner(db, KEY, digest, true).await
}

/// The same, for a teach or hint_ladder: the current bank context, no candidate.
pub async fn fixture_approve_body_instruction(db: &TestDb) -> Value {
    approve_body_inner(db, KEY, "", false).await
}

/// Compute the approval body for any kind at `kp_id`.
async fn approve_body_inner(db: &TestDb, kp_id: &str, digest: &str, is_template: bool) -> Value {
    let cur_digest = fixture_curriculum_digest();
    let eng_digest = fixture_review_engine_digest();
    let current = CurrentContext {
        policy_digest: None,
        curriculum_digest: &cur_digest,
        review_engine_digest: eng_digest,
    };
    let candidate = is_template.then_some(digest);
    let (template_context, _) = template_review_context(
        &db.admin,
        kp_id,
        current,
        candidate,
    )
    .await
    .expect("the template context reads from the seeded database");
    json!({
        "policy_digest": null,
        "template_context_digest": template_context,
        "curriculum_digest": cur_digest,
        "review_engine_digest": eng_digest,
    })
}
