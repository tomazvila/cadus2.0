//! `/api/admin/content*` — the C6 review surface (M6 R5).
//!
//! Requirements: C6 (a human approves, and the approval binds to the digest), A6
//! (a skipped check is explicit), T3 (the attempts and the money reach the
//! reviewer), R4 (the routes do local CPU work and database I/O only).
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` section 3.2 and row R5
//! of section 7. The four acceptance checks of row R5 are the four tests marked
//! ACCEPTANCE below.
//!
//! Every expected value here is a LITERAL: the status codes, the error codes,
//! the message text, the eight rendered instances with their computed answers,
//! and the row values the writes leave in `content_store`. Nothing is re-read
//! from the code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::sync::Arc;

use axum::Router;
use cadus_core::curriculum::load::{RawCurriculum, RawUnit};
use cadus_core::curriculum::model::{Catalog, Course, Exemplar, KnowledgePoint, Slug, Topic, Unit};
use cadus_core::curriculum::{AnswerKind, Curriculum};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::Content;
use cadus_web::{AppState, create_app};
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};

use common::{
    Answer, SESSION_TOKEN_ONE, SESSION_TOKEN_TWO, get, get_bearer, post_bearer, seed_session, send,
    shift,
};

/// The serving key of the fixture knowledge point.
const KEY: &str = "band/kp1";

/// A second serving key. The fixture curriculum does not name it, and the queue
/// read needs no curriculum.
const OTHER_KEY: &str = "band/kp2";

/// The digest of the pending template under review.
const PENDING: &str = "r5-pending-digest";

/// A digest no row carries.
const ABSENT: &str = "r5-no-such-digest";

/// The four paths of spec section 3.2, with [`PENDING`] in the digest segment.
const LIST_PATH: &str = "/api/admin/content";
const SHOW_PATH: &str = "/api/admin/content/r5-pending-digest";
const APPROVE_PATH: &str = "/api/admin/content/r5-pending-digest/approve";
const REJECT_PATH: &str = "/api/admin/content/r5-pending-digest/reject";

/// The eight instances the show route renders for [`template_body`].
///
/// The draw runs from the constant seed `cadus_core::template::GATE_SEED`, so
/// the list is a function of the document alone and two reviewers of one digest
/// read the same eight problems. Each answer is the difference the statement
/// asks for, and the pair is pinned here character for character.
const INSTANCES: [(&str, &str); 8] = [
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
fn difference_of(text: &str) -> String {
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
fn template_body() -> Value {
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
fn graph() -> Curriculum {
    let catalog = Catalog {
        courses: vec![Course {
            id: Slug::new("c1").unwrap(),
            name: "Foundations".to_string(),
            order: 0,
            mastery_floor: Vec::new(),
            mastery_floor_course: None,
        }],
    };
    Curriculum::build(RawCurriculum {
        catalog,
        units: vec![RawUnit {
            course_id: "c1".to_string(),
            file_name: "00-M1.yaml".to_string(),
            unit: Unit {
                unit: "M1".to_string(),
                course: Slug::new("c1").unwrap(),
                module: "M1".to_string(),
                topics: vec![Topic {
                    id: Slug::new("band").unwrap(),
                    name: "The band topic".to_string(),
                    core: true,
                    difficulty: 0.3,
                    drill: false,
                    answer_kind: AnswerKind::Numeric,
                    expected_time_secs: 30,
                    prerequisites: Vec::new(),
                    encompassings_extra: Vec::new(),
                    knowledge_points: vec![KnowledgePoint {
                        id: Slug::new("kp1").unwrap(),
                        name: "The first point".to_string(),
                        key_prerequisites: Vec::new(),
                        exemplars: vec![Exemplar {
                            problem: "Compute $7 - 2$.".to_string(),
                            answer: "5".to_string(),
                            solution_sketch: None,
                        }],
                        constraints: None,
                    }],
                    diagnostic_exemplar: None,
                    anki_seeds: Vec::new(),
                }],
            },
            first_load_index: 0,
        }],
    })
    .unwrap()
}

/// The router of a test, with the fixture curriculum and the admin path.
///
/// The admin path is the superuser pool of the throwaway database, which is the
/// production `cadus_admin` role in every way this unit reads: it holds the
/// INSERT, UPDATE, and DELETE that `cadus_app` does not.
fn app(db: &TestDb) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(Content::new(graph())))
            .with_admin(Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS)),
    )
}

/// The same router with NO admin path, which is the default of a deployment
/// that sets no `CADUS_ADMIN_DATABASE_URL`.
fn app_without_admin(db: &TestDb) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(Content::new(graph()))),
    )
}

/// Seed one account with a live session on `token_hash`, and return its id.
async fn seed_account(db: &TestDb, email: &str, token_hash: &str, admin: bool) -> Uuid {
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
async fn seed_admin(db: &TestDb) -> Uuid {
    seed_account(db, "r5-admin@example.test", SESSION_TOKEN_ONE.1, true).await
}

/// One `content_store` row a test seeds.
struct Seed<'a> {
    digest: &'a str,
    kp_id: &'a str,
    kind: &'a str,
    status: &'a str,
    body: Value,
    attempts: i32,
    cost: Option<&'a str>,
}

impl<'a> Seed<'a> {
    /// One template row of [`template_body`], with one attempt and no bill.
    fn template(digest: &'a str, kp_id: &'a str, status: &'a str) -> Self {
        Self {
            digest,
            kp_id,
            kind: "template",
            status,
            body: template_body(),
            attempts: 1,
            cost: None,
        }
    }
}

/// Write one `content_store` row with the superuser pool.
async fn seed_row(db: &TestDb, seed: &Seed<'_>) {
    sqlx::query(
        "INSERT INTO content_store
            (digest, kp_id, kind, body, status, authoring_attempts, authoring_cost_usd)
         VALUES ($1, $2, $3, $4::text::jsonb, $5, $6, $7::text::numeric)",
    )
    .bind(seed.digest)
    .bind(seed.kp_id)
    .bind(seed.kind)
    .bind(seed.body.to_string())
    .bind(seed.status)
    .bind(seed.attempts)
    .bind(seed.cost)
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Write the pending template under review, with its T3 bill.
async fn seed_pending(db: &TestDb) {
    let mut seed = Seed::template(PENDING, KEY, "pending");
    seed.attempts = 4;
    seed.cost = Some("0.012500");
    seed_row(db, &seed).await;
}

/// Write `count` approved templates on `kp_id`, with a digest of `prefix<n>`.
async fn seed_approved(db: &TestDb, kp_id: &str, prefix: &str, count: usize) {
    for n in 0..count {
        let digest = format!("{prefix}{n}");
        seed_row(db, &Seed::template(&digest, kp_id, "approved")).await;
    }
}

/// The `status`, `review_reason`, `approved_by`, and `approved_at` of one row.
async fn row_state(db: &TestDb, digest: &str) -> (String, Option<String>, Option<Uuid>, bool) {
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
async fn admin_get(app: &Router, uri: &str) -> Answer {
    send(app, get_bearer(uri, SESSION_TOKEN_ONE.0)).await
}

/// A `POST` that presents the admin session token.
async fn admin_post(app: &Router, uri: &str, body: &Value) -> Answer {
    send(app, post_bearer(uri, SESSION_TOKEN_ONE.0, body)).await
}

/// The queue line of one digest.
fn item_of<'a>(body: &'a Value, digest: &str) -> &'a Value {
    body.get("items")
        .and_then(Value::as_array)
        .expect("the answer carries no items array")
        .iter()
        .find(|item| item.get("digest").and_then(Value::as_str) == Some(digest))
        .unwrap_or_else(|| panic!("the queue names no line for {digest}: {body}"))
}

// --------------------------------------------------------------------------- //
// The two refusals
// --------------------------------------------------------------------------- //

/// A request with no credential is `401 unauthorized` on all four routes.
#[tokio::test]
async fn a_request_with_no_session_is_unauthorized_on_all_four() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_pending(&db).await;

        for answer in [
            send(&app, get(LIST_PATH)).await,
            send(&app, get(SHOW_PATH)).await,
            send(&app, common::post(APPROVE_PATH, &json!({}))).await,
            send(
                &app,
                common::post(REJECT_PATH, &json!({"reason": "no good"})),
            )
            .await,
        ] {
            assert_eq!(answer.status.as_u16(), 401, "{}", answer.body);
            assert_eq!(answer.code(), "unauthorized");
        }
    })
    .await;
}

/// ACCEPTANCE. A live session on an account that is not an admin is
/// `403 forbidden` on all four routes.
///
/// The account exists and the session is live, so the refusal is the admin gate
/// and not the session guard. No answer carries a document field.
#[tokio::test]
async fn a_session_that_is_not_an_admin_is_forbidden_on_all_four() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_pending(&db).await;
        seed_account(&db, "r5-learner@example.test", SESSION_TOKEN_TWO.1, false).await;

        let learner_get = |uri: &'static str| {
            let app = app.clone();
            async move { send(&app, get_bearer(uri, SESSION_TOKEN_TWO.0)).await }
        };
        let learner_post = |uri: &'static str, body: Value| {
            let app = app.clone();
            async move { send(&app, post_bearer(uri, SESSION_TOKEN_TWO.0, &body)).await }
        };

        for answer in [
            learner_get(LIST_PATH).await,
            learner_get(SHOW_PATH).await,
            learner_post(APPROVE_PATH, json!({})).await,
            learner_post(REJECT_PATH, json!({"reason": "no good"})).await,
        ] {
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
            assert!(
                answer.body.get("items").is_none() && answer.body.get("body").is_none(),
                "the refusal carried a document: {}",
                answer.body
            );
        }

        // The refusal wrote nothing.
        assert_eq!(
            row_state(&db, PENDING).await,
            ("pending".to_string(), None, None, false)
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The show route
// --------------------------------------------------------------------------- //

/// ACCEPTANCE. The show route renders eight instances with computed answers.
///
/// 1.0 `scripts/review_templates.py:52` prints `SAMPLE_INSTANCES = 8`, and its
/// docstring gives the reason: a bad corner is obvious in the instances and
/// invisible in the expression. The eight pairs are pinned in [`INSTANCES`].
#[tokio::test]
async fn the_show_route_returns_eight_instances_with_computed_answers() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        let answer = admin_get(&app, SHOW_PATH).await;

        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(answer.body.get("sample_instances"), Some(&json!(8)));
        let instances = answer
            .body
            .get("instances")
            .and_then(Value::as_array)
            .expect("the answer carries no instances array");
        assert_eq!(instances.len(), 8, "{}", answer.body);
        let expected: Vec<Value> = INSTANCES
            .iter()
            .map(|(text, expected)| json!({"text": text, "answer": expected}))
            .collect();
        assert_eq!(instances, &expected);
        assert_eq!(answer.body.get("instances_note"), Some(&Value::Null));

        // Each pinned answer is the answer of its pinned problem, by the test's
        // own arithmetic. The eight statements are distinct, as one pool row set
        // is distinct.
        for (text, expected) in INSTANCES {
            assert_eq!(difference_of(text), expected, "{text}");
        }
        let mut texts: Vec<&str> = INSTANCES.iter().map(|(text, _)| *text).collect();
        texts.sort_unstable();
        texts.dedup();
        assert_eq!(texts.len(), 8, "two pinned instances share one statement");
    })
    .await;
}

/// The show route carries the gate notes, the T3 numbers, and the body.
///
/// The gate note is the sentence `crates/core/tests/template_gate.rs` pins for
/// this document: neither corner of the crossed-corner rule satisfies the band,
/// so the gate skips the rule and says so (A6).
#[tokio::test]
async fn the_show_route_carries_the_gate_notes_and_the_authoring_bill() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        let answer = admin_get(&app, SHOW_PATH).await;

        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(answer.body.get("digest"), Some(&json!("r5-pending-digest")));
        assert_eq!(answer.body.get("kp_id"), Some(&json!("band/kp1")));
        assert_eq!(answer.body.get("kind"), Some(&json!("template")));
        assert_eq!(answer.body.get("status"), Some(&json!("pending")));
        assert_eq!(answer.body.get("authoring_attempts"), Some(&json!(4)));
        assert_eq!(
            answer.body.get("authoring_cost_usd"),
            Some(&json!("0.012500"))
        );
        assert_eq!(answer.body.get("approved_at"), Some(&Value::Null));
        assert_eq!(answer.body.get("review_reason"), Some(&Value::Null));
        assert_eq!(answer.body.get("body"), Some(&template_body()));
        assert_eq!(
            answer.body.get("gate"),
            Some(&json!({
                "kp_id": "band/kp1",
                "digest": "r5-pending-digest",
                "gated": true,
                "exhaustive": true,
                "instances_checked": 17,
                "notes": [
                    "the crossed-corner rule is skipped for a and b: the constraints admit no \
                     tuple at a=2 with b=9, nor at a=10 with b=1"
                ],
            }))
        );
    })
    .await;
}

/// A digest the table does not hold is `404 not_found` on the show route.
#[tokio::test]
async fn the_show_route_of_an_unknown_digest_is_not_found() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;

        let answer = admin_get(&app, &format!("/api/admin/content/{ABSENT}")).await;

        assert_eq!(answer.status.as_u16(), 404, "{}", answer.body);
        assert_eq!(answer.code(), "not_found");
        assert_eq!(
            answer
                .body
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str),
            Some("No stored document carries that digest.")
        );
    })
    .await;
}

/// A body that is not a template renders no instance and says why.
///
/// A teach page has no statement and no answer expression, so the show route
/// carries an empty instance list and a null gate block. A6 refuses a silent
/// empty list, so the kind is visible in the same answer.
#[tokio::test]
async fn a_teach_document_renders_no_instance_and_no_gate() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        let mut seed = Seed::template("r5-teach-digest", KEY, "pending");
        seed.kind = "teach";
        seed.body = json!({"title": "Borrowing", "concept": "Take from the next column."});
        seed_row(&db, &seed).await;

        let answer = admin_get(&app, "/api/admin/content/r5-teach-digest").await;

        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(answer.body.get("kind"), Some(&json!("teach")));
        assert_eq!(answer.body.get("instances"), Some(&json!([])));
        assert_eq!(answer.body.get("gate"), Some(&Value::Null));
        assert_eq!(answer.body.get("summary"), Some(&json!("Borrowing")));
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The queue
// --------------------------------------------------------------------------- //

/// ACCEPTANCE. The list flags a knowledge point with fewer than three approved
/// templates.
///
/// `band/kp1` holds two approved templates, so its queue line carries
/// `approved_templates: 2` and `bank_warning: true`. `band/kp2` holds three, so
/// its line carries `bank_warning: false`. The reason is 1.0's
/// `cmd_list`: a knowledge point serves from its approved slots alone.
#[tokio::test]
async fn the_list_flags_a_knowledge_point_below_the_bank_target() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;
        seed_approved(&db, KEY, "r5-kp1-ok", 2).await;
        seed_row(&db, &Seed::template("r5-kp2-pending", OTHER_KEY, "pending")).await;
        seed_approved(&db, OTHER_KEY, "r5-kp2-ok", 3).await;

        let answer = admin_get(&app, "/api/admin/content?status=pending").await;

        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(answer.body.get("bank_target"), Some(&json!(3)));

        let thin = item_of(&answer.body, PENDING);
        assert_eq!(thin.get("kp_id"), Some(&json!("band/kp1")));
        assert_eq!(thin.get("approved_templates"), Some(&json!(2)));
        assert_eq!(thin.get("bank_warning"), Some(&json!(true)));

        let full = item_of(&answer.body, "r5-kp2-pending");
        assert_eq!(full.get("kp_id"), Some(&json!("band/kp2")));
        assert_eq!(full.get("approved_templates"), Some(&json!(3)));
        assert_eq!(full.get("bank_warning"), Some(&json!(false)));
    })
    .await;
}

/// A knowledge point with no approved template is flagged too.
///
/// 1.0 prints the note only when the count is above zero. 2.0 flags a bank of
/// zero as well: spec section 3.2 says "fewer than 3 approved templates", and no
/// approved template is the worst case of the same fault.
#[tokio::test]
async fn an_empty_bank_is_flagged() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        let answer = admin_get(&app, LIST_PATH).await;

        let line = item_of(&answer.body, PENDING);
        assert_eq!(line.get("approved_templates"), Some(&json!(0)));
        assert_eq!(line.get("bank_warning"), Some(&json!(true)));
    })
    .await;
}

/// One queue line carries the T3 numbers and the first 64 characters of the
/// statement.
#[tokio::test]
async fn a_queue_line_carries_the_summary_and_the_authoring_bill() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        let answer = admin_get(&app, LIST_PATH).await;

        let line = item_of(&answer.body, PENDING);
        assert_eq!(line.get("kind"), Some(&json!("template")));
        assert_eq!(line.get("status"), Some(&json!("pending")));
        assert_eq!(line.get("authoring_attempts"), Some(&json!(4)));
        assert_eq!(line.get("authoring_cost_usd"), Some(&json!("0.012500")));
        assert_eq!(line.get("summary"), Some(&json!("Compute ${a} - {b}$.")));
        assert_eq!(answer.body.get("limit"), Some(&json!(200)));
    })
    .await;
}

/// The three query parameters select the queue.
#[tokio::test]
async fn the_queue_filters_by_status_kind_and_serving_key() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;
        seed_approved(&db, OTHER_KEY, "r5-other", 1).await;

        let pending = admin_get(&app, "/api/admin/content?status=pending").await;
        assert_eq!(
            pending
                .body
                .get("items")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            item_of(&pending.body, PENDING).get("status"),
            Some(&json!("pending"))
        );

        let by_kp = admin_get(&app, "/api/admin/content?kp=band/kp2").await;
        assert_eq!(
            by_kp
                .body
                .get("items")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            item_of(&by_kp.body, "r5-other0").get("kp_id"),
            Some(&json!("band/kp2"))
        );

        let by_kind = admin_get(&app, "/api/admin/content?kind=teach").await;
        assert_eq!(by_kind.body.get("items"), Some(&json!([])));
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The two writes
// --------------------------------------------------------------------------- //

/// ACCEPTANCE. A reject with no reason is `422`, and it writes nothing.
///
/// The three refused bodies are the three ways a reason goes missing: an empty
/// object, a null field, and a field of spaces alone. 1.0 makes `--reason` a
/// required argument of `cmd_reject`.
#[tokio::test]
async fn a_reject_without_a_reason_is_unprocessable() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        for body in [json!({}), json!({"reason": null}), json!({"reason": "   "})] {
            let answer = admin_post(&app, REJECT_PATH, &body).await;

            assert_eq!(answer.status.as_u16(), 422, "{body} gave {}", answer.body);
            assert_eq!(answer.code(), "invalid_request");
            assert_eq!(
                answer
                    .body
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str),
                Some(
                    "A rejection needs a reason: send a JSON object with a non-empty \"reason\" \
                     string."
                )
            );
        }

        assert_eq!(
            row_state(&db, PENDING).await,
            ("pending".to_string(), None, None, false)
        );
    })
    .await;
}

/// A reject with a reason writes the verdict and the reason.
#[tokio::test]
async fn a_reject_with_a_reason_writes_the_verdict() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        let answer = admin_post(
            &app,
            REJECT_PATH,
            &json!({"reason": "  the low edge is missing  "}),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(
            answer.body,
            json!({"digest": "r5-pending-digest", "status": "rejected"})
        );
        assert_eq!(
            row_state(&db, PENDING).await,
            (
                "rejected".to_string(),
                Some("the low edge is missing".to_string()),
                None,
                false
            )
        );
    })
    .await;
}

/// An approve stamps the row with the reviewer, and a second approve is the
/// same answer.
#[tokio::test]
async fn an_approve_stamps_the_reviewer_and_is_idempotent() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let reviewer = seed_admin(&db).await;
        seed_pending(&db).await;

        let first = admin_post(&app, APPROVE_PATH, &json!({})).await;
        assert_eq!(first.status.as_u16(), 200, "{}", first.body);
        assert_eq!(first.body.get("digest"), Some(&json!("r5-pending-digest")));
        assert_eq!(first.body.get("status"), Some(&json!("approved")));
        let stamp = first
            .body
            .get("approved_at")
            .and_then(Value::as_str)
            .expect("the answer carries no approved_at")
            .to_string();

        let second = admin_post(&app, APPROVE_PATH, &json!({})).await;
        assert_eq!(second.status.as_u16(), 200, "{}", second.body);
        assert_eq!(
            second.body.get("approved_at").and_then(Value::as_str),
            Some(stamp.as_str())
        );

        let (status, reason, approved_by, stamped) = row_state(&db, PENDING).await;
        assert_eq!(status, "approved");
        assert_eq!(reason, None);
        assert_eq!(approved_by, Some(reviewer));
        assert!(stamped, "the approved row carries no approved_at");
    })
    .await;
}

/// A write on a digest the table does not hold is `404 not_found`.
#[tokio::test]
async fn a_write_on_an_unknown_digest_is_not_found() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;

        for (path, body) in [
            (format!("/api/admin/content/{ABSENT}/approve"), json!({})),
            (
                format!("/api/admin/content/{ABSENT}/reject"),
                json!({"reason": "no good"}),
            ),
        ] {
            let answer = admin_post(&app, &path, &body).await;

            assert_eq!(answer.status.as_u16(), 404, "{path} gave {}", answer.body);
            assert_eq!(answer.code(), "not_found");
        }
    })
    .await;
}

/// A deployment with no admin connection refuses both writes with `503`.
///
/// `cadus_app` holds SELECT on `content_store` and nothing else, so the write
/// would fail at the server with SQLSTATE 42501 and the reviewer would read
/// `500`. The named path answers before the statement runs.
#[tokio::test]
async fn a_write_with_no_admin_connection_is_service_unavailable() {
    TestDb::with(|db| async move {
        let app = app_without_admin(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        for (path, body) in [
            (APPROVE_PATH, json!({})),
            (REJECT_PATH, json!({"reason": "no good"})),
        ] {
            let answer = admin_post(&app, path, &body).await;

            assert_eq!(answer.status.as_u16(), 503, "{path} gave {}", answer.body);
            assert_eq!(answer.code(), "admin_path_unavailable");
        }

        assert_eq!(
            row_state(&db, PENDING).await,
            ("pending".to_string(), None, None, false)
        );
    })
    .await;
}

/// A reject body that is not a JSON object is `422`, and the handler never
/// panics on it.
#[tokio::test]
async fn a_reject_body_that_is_not_an_object_is_unprocessable() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        for body in [json!("a string"), json!([1, 2, 3]), json!(7)] {
            let answer = admin_post(&app, REJECT_PATH, &body).await;

            assert_eq!(answer.status.as_u16(), 422, "{body} gave {}", answer.body);
            assert_eq!(answer.code(), "invalid_request");
            assert_eq!(
                answer
                    .body
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str),
                Some("The request body must be a JSON object.")
            );
        }
    })
    .await;
}

/// A rejection reason past the bound is `422`.
#[tokio::test]
async fn a_reason_past_the_bound_is_unprocessable() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        let long = "x".repeat(1_001);
        let answer = admin_post(&app, REJECT_PATH, &json!({ "reason": long })).await;

        assert_eq!(answer.status.as_u16(), 422, "{}", answer.body);
        assert_eq!(answer.code(), "invalid_request");
        assert_eq!(
            answer
                .body
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str),
            Some("The rejection reason is too long.")
        );
        assert_eq!(
            row_state(&db, PENDING).await,
            ("pending".to_string(), None, None, false)
        );
    })
    .await;
}
