//! M5 U8 acceptance: `POST /api/task/{task_id}/answer`.
//!
//! Requirements: A3, A4, C2, C3, C4, D-O2, D-S6, L2, R4, T1. Spec
//! `docs/reference/web-service-1.0-spec.md` sections 2.1, 4.3, 5 and 10, and row
//! U8 of section 11. Rulings D-M5-2, D-M5-3, D-M5-4 and D-M5-7.
//!
//! The four acceptance checks of row U8 land here:
//!
//! 1. the section 10 fast-path cases decide with no model client linked —
//!    [`the_fast_path_cases_decide_with_no_model_call`];
//! 2. a replayed request appends nothing and returns `already_recorded` —
//!    [`a_replayed_request_appends_nothing_and_returns_already_recorded`];
//! 3. an H3 re-solve that fails rewrites the stashed pass to a miss —
//!    [`an_h3_re_solve_that_fails_rewrites_the_stashed_pass_to_a_miss`];
//! 4. `secs` clamps at `expected_time_secs * 10` with `timing-unreliable` —
//!    [`secs_clamps_at_ten_times_the_expected_time`].
//!
//! Every expected value is a LITERAL: a literal status code, a literal error
//! code, a literal tier, a literal tag, a literal count, a literal XP total.
//! Nothing here re-reads a constant from the code under test.
//!
//! Every call presents a real session cookie, and `auth::layer::tenant_layer`
//! binds the tenant from it (FIX-M5-C). No test here writes a request extension
//! by hand.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use cadus_core::config::Config;
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, Exemplar, KnowledgePoint, RawCurriculum, RawUnit,
    Slug, Topic, Unit,
};
use cadus_core::event::{Event, SchemaVersion, SessionStart, TaskType, Timestamp};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_core::pool::{PoolAnswer, PoolProblem};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::grade::{Grade, deterministic_grade, reference_assisted};
use cadus_web::state::{Content, ServedProblem, TaskProgress, WebState};
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use tower::ServiceExt;

/// The Unix microsecond instant of 2026-01-01T00:00:00Z.
const BASE_US: i64 = 1_767_225_600_000_000;

/// The session id every seeded log opens.
const SESSION: &str = "s_2026-01-01a";

/// The task id of the `addition` lesson (`assign_ids`: `{session}-{type}-{topic}`).
const LESSON: &str = "s_2026-01-01a-lesson-addition";

/// The serving key of the first knowledge point of `addition`.
const KEY: &str = "addition/kp1";

/// The statement of the served problem.
const PROBLEM_TEXT: &str = "Compute 8 + 5.5.";

/// The authored answer of the served problem.
const EXPECTED_ANSWER: &str = "13.5";

/// The authored solution sketch of the served problem.
const SOLUTION: &str = "Add the parts to reach 13.5.";

/// The `problem_id` every seeded state hands the client.
const PROBLEM_ID: &str = "p0000000000000000000000000000001";

/// The stock re-solve instruction of D-M5-3, spelled out (spec section 5.5).
const RE_SOLVE_TEXT: &str = "Study the worked solution above until you can see why each step \
                             follows. Then close it and solve the original problem again \
                             yourself, from memory and unaided. Do that before you move on.";

/// The topic's authored solve time. The timing cap is ten times this.
const EXPECTED_TIME_SECS: i64 = 30;

// --------------------------------------------------------------------------- //
// The fixture curriculum
// --------------------------------------------------------------------------- //

/// One knowledge point with its authored exemplars.
fn kp(id: &str, exemplars: Vec<Exemplar>) -> KnowledgePoint {
    KnowledgePoint {
        id: Slug::new(id).unwrap(),
        name: format!("The {id} point"),
        key_prerequisites: Vec::new(),
        exemplars,
        constraints: None,
    }
}

/// One exemplar.
fn exemplar(problem: &str, answer: &str) -> Exemplar {
    Exemplar {
        problem: problem.to_string(),
        answer: answer.to_string(),
        solution_sketch: Some(format!("Add the parts to reach {answer}.")),
    }
}

/// One topic of the fixture curriculum.
fn topic(id: &str, points: Vec<KnowledgePoint>) -> Topic {
    Topic {
        id: Slug::new(id).unwrap(),
        name: format!("The {id} topic"),
        core: true,
        difficulty: 0.3,
        drill: false,
        answer_kind: AnswerKind::Numeric,
        expected_time_secs: EXPECTED_TIME_SECS,
        prerequisites: Vec::new(),
        encompassings_extra: Vec::new(),
        knowledge_points: points,
        diagnostic_exemplar: None,
        anki_seeds: Vec::new(),
    }
}

/// The fixture curriculum: one course, one module, two topics. `addition`
/// authors two knowledge points, and neither one names a key prerequisite.
fn graph() -> Curriculum {
    build_graph(Vec::new())
}

/// The same fixture, with `subtraction` a KEY prerequisite of `addition/kp1`.
///
/// The repeat-fail peel-back queues the key prerequisites of the failed
/// knowledge point, so a fixture with none can never show the event.
fn graph_with_key_prereq() -> Curriculum {
    build_graph(vec![Slug::new("subtraction").unwrap()])
}

/// The fixture curriculum: one course, one module, two topics. `addition`
/// authors two knowledge points, and `kp1` names `key_prereqs`.
fn build_graph(key_prereqs: Vec<Slug>) -> Curriculum {
    let mut first = kp("kp1", vec![exemplar(PROBLEM_TEXT, EXPECTED_ANSWER)]);
    first.key_prerequisites = key_prereqs;
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
                topics: vec![
                    topic(
                        "addition",
                        vec![
                            first,
                            kp("kp2", vec![exemplar("Compute 40 + 2.5.", "42.5")]),
                        ],
                    ),
                    topic("subtraction", vec![kp("kp1", vec![])]),
                ],
            },
            first_load_index: 0,
        }],
    })
    .unwrap()
}

// --------------------------------------------------------------------------- //
// The harness
// --------------------------------------------------------------------------- //

/// The router of a test, with the fixture curriculum loaded.
fn app(db: &TestDb) -> Router {
    router(db, graph())
}

/// The router of a test, with `arena` loaded.
fn router(db: &TestDb, arena: Curriculum) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(Content::new(arena))),
    )
}

/// One request against the router. `tenant` is the bound learner.
async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    tenant: Option<Uuid>,
    body: Option<Value>,
) -> (StatusCode, String) {
    let mut builder = Request::builder().method(method).uri(uri);
    let payload = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let mut request = builder.body(payload).unwrap();
    if let Some(user) = tenant {
        common::present_session(request.headers_mut(), user);
    }
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into())
}

/// The parsed JSON body of a call.
fn parse(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|err| panic!("body is not JSON: {err}\n{body}"))
}

/// Answer the lesson's live problem.
async fn answer(app: &Router, user: Uuid, body: Value) -> (StatusCode, Value) {
    let (status, raw) = call(
        app,
        Method::POST,
        &format!("/api/task/{LESSON}/answer"),
        Some(user),
        Some(body),
    )
    .await;
    (status, parse(&raw))
}

/// Open `SESSION` in the log of `user`.
async fn seed_open_session(db: &TestDb, user: Uuid) {
    let event = Event::SessionStart(SessionStart {
        ts: Timestamp::from_micros(BASE_US),
        session: Some(SESSION.to_string()),
        v: SchemaVersion,
    });
    let ts = DateTime::<Utc>::from_timestamp_micros(BASE_US).unwrap();
    sqlx::query!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, payload)
        VALUES ($1, 1, $2, $3, $4, 1, $5)
        "#,
        user,
        ts,
        event.type_name(),
        event.session(),
        serde_json::to_value(&event).unwrap()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Put one unclaimed row into the pool of `(user, KEY)`, so the reply's `next`
/// has a problem to draw.
async fn seed_pool_row(db: &TestDb, user: Uuid, text: &str, answer: &str, hash: &str) {
    let problem = PoolProblem {
        v: 1,
        text: text.to_string(),
        bindings: BTreeMap::new(),
        seed: 7,
    };
    let expected = PoolAnswer {
        v: 1,
        answer: answer.to_string(),
    };
    sqlx::query!(
        r#"
        INSERT INTO serving_pool
            (user_id, kp_id, source, content_digest, problem, expected_answer, instance_hash)
        VALUES ($1, $2, 'template', NULL, $3::text::jsonb, $4::text::jsonb, $5)
        "#,
        user,
        KEY,
        problem.to_body().unwrap(),
        expected.to_body().unwrap(),
        hash,
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// One live problem of the lesson, `age_secs` old.
fn served(age_secs: f64, kp: &str, hints: Vec<String>) -> ServedProblem {
    let now = Utc::now().timestamp_micros() as f64 / 1_000_000.0;
    ServedProblem {
        problem_id: PROBLEM_ID.to_string(),
        task_id: LESSON.to_string(),
        topic: Some("addition".to_string()),
        serve_topic: Some("addition".to_string()),
        kp: Some(kp.to_string()),
        answer_kind: Some("numeric".to_string()),
        text: PROBLEM_TEXT.to_string(),
        expected: PoolAnswer {
            v: 1,
            answer: EXPECTED_ANSWER.to_string(),
        },
        solution_sketch: Some(SOLUTION.to_string()),
        started_at: now - age_secs,
        hints_given: hints,
        index: 0,
        rework: None,
    }
}

/// A D-S6 document holding one live lesson problem.
fn state_with(served_problem: ServedProblem, answered: i64, done: bool) -> WebState {
    let mut scratch = WebState::for_session(SESSION);
    scratch.tasks.insert(
        LESSON.to_string(),
        TaskProgress {
            task_id: LESSON.to_string(),
            task_type: "lesson".to_string(),
            total: 0,
            served: 1,
            answered,
            done,
            current_kp: served_problem.kp.clone(),
        },
    );
    scratch.served.insert(LESSON.to_string(), served_problem);
    scratch
}

/// Write a D-S6 document for `user`.
async fn put_state(db: &TestDb, user: Uuid, scratch: &WebState) {
    sqlx::query!(
        r#"
        INSERT INTO web_states (user_id, doc) VALUES ($1, $2)
        ON CONFLICT (user_id) DO UPDATE SET doc = excluded.doc
        "#,
        user,
        scratch.to_doc().unwrap()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// The stored D-S6 document of `user`.
async fn stored_state(db: &TestDb, user: Uuid) -> WebState {
    let doc = sqlx::query_scalar!(
        r#"SELECT doc AS "doc!" FROM web_states WHERE user_id = $1"#,
        user
    )
    .fetch_one(&db.admin)
    .await
    .unwrap();
    WebState::from_doc(&doc).unwrap()
}

/// Every event of `user` of one type, oldest first.
async fn events_of_type(db: &TestDb, user: Uuid, kind: &str) -> Vec<Value> {
    sqlx::query_scalar!(
        r#"SELECT payload AS "payload!" FROM events WHERE user_id = $1 AND type = $2 ORDER BY seq"#,
        user,
        kind
    )
    .fetch_all(&db.admin)
    .await
    .unwrap()
}

/// Seed a learner with an open session and one live lesson problem.
async fn learner(db: &TestDb, email: &str, live: ServedProblem) -> Uuid {
    let user = common::seed_learner(db, email).await;
    seed_open_session(db, user).await;
    put_state(db, user, &state_with(live, 0, false)).await;
    user
}

// --------------------------------------------------------------------------- //
// Acceptance 1: the section 10 fast-path cases
// --------------------------------------------------------------------------- //

/// Spec section 10, row "Fast-path cases / error tags": `12`/`12`, `12`/`12.0`,
/// `12`/`sqrt(144)`, `7329`/`7,329`, `7400`/`7400` and `2*x+1`/`1 + 2x` all
/// decide with no engine. Each one is a pass at the NEUTRAL tier with no tag
/// (D-M5-2, D-M5-4).
///
/// `cadus_web` cannot link a model client at all — `tests/purity.rs` proves that
/// from the resolved dependency graph — so "no model client linked" is a
/// property of the crate and these six cases are the verdicts it decides alone.
#[test]
fn the_fast_path_cases_decide_with_no_model_call() {
    let cases = [
        ("12", "12", AnswerKind::Numeric),
        ("12", "12.0", AnswerKind::Numeric),
        ("12", "sqrt(144)", AnswerKind::Numeric),
        ("7329", "7,329", AnswerKind::Numeric),
        ("7400", "7400", AnswerKind::Numeric),
        ("2*x+1", "1 + 2x", AnswerKind::Expression),
    ];
    for (expected, learner, kind) in cases {
        let grade = deterministic_grade(expected, learner, kind);
        assert_eq!(
            grade,
            Grade {
                correct: true,
                work_quality: cadus_core::event::WorkQuality::NearlyPerfect,
                error_tags: Vec::new(),
            },
            "{expected} / {learner}"
        );
    }
}

/// The three tiers of D-M5-2, and the two server-produced tags.
///
/// A correct answer written with periods for thousands is a pass that carries
/// `notation`, a claim about form and not about the mathematics. A blank is
/// `poor` with `blank-answer` (D-M5-7, hyphenated). A decided miss is
/// `nearly_passable` with NO tag (D-M5-4).
#[test]
fn the_three_deterministic_tiers_are_the_d_m5_2_ruling() {
    assert_eq!(
        deterministic_grade("7329", "7.329", AnswerKind::Numeric),
        Grade {
            correct: true,
            work_quality: cadus_core::event::WorkQuality::NearlyPerfect,
            error_tags: vec!["notation".to_string()],
        }
    );
    assert_eq!(
        deterministic_grade("13.5", "   ", AnswerKind::Numeric),
        Grade {
            correct: false,
            work_quality: cadus_core::event::WorkQuality::Poor,
            error_tags: vec!["blank-answer".to_string()],
        }
    );
    assert_eq!(
        deterministic_grade("13.5", "14", AnswerKind::Numeric),
        Grade {
            correct: false,
            work_quality: cadus_core::event::WorkQuality::NearlyPassable,
            error_tags: Vec::new(),
        }
    );
    // An answer outside the grammar is a MISS, never a pass and never a model
    // verdict (spec section 5.1).
    assert_eq!(
        deterministic_grade("13.5", "about thirteen and a half", AnswerKind::Numeric),
        Grade {
            correct: false,
            work_quality: cadus_core::event::WorkQuality::NearlyPassable,
            error_tags: Vec::new(),
        }
    );
}

/// The same verdicts over HTTP, with the reply fields of section 2.1.
#[tokio::test]
async fn a_correct_answer_replies_with_the_neutral_tier() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "correct@example.com", served(5.0, "kp1", Vec::new())).await;
        seed_pool_row(&db, user, "Compute 1 + 1.", "2", "hash-next").await;

        let (status, body) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["attempt_id"], "s_2026-01-01a-lesson-addition-1");
        assert_eq!(body["correct"], true);
        assert_eq!(body["work_quality"], "nearly_perfect");
        assert_eq!(body["error_tags"], json!([]));
        assert_eq!(body["task_status"], "continue");
        assert_eq!(body["remediation"], json!([]));
        assert_eq!(body["solution"], SOLUTION);
        // A pass owes no re-solve instruction (spec section 5.5).
        assert_eq!(body.get("re_solve"), None);
        // The next problem came from the pool inside the same transaction.
        assert_eq!(body["next"]["text"], "Compute 1 + 1.");
    })
    .await;
}

/// A blank submission is `poor` with the hyphenated tag, and it carries the
/// pinned re-solve instruction (D-M5-3, D-M5-7).
#[tokio::test]
async fn a_blank_answer_is_poor_and_carries_the_re_solve_text() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "blank@example.com", served(5.0, "kp1", Vec::new())).await;

        let (status, body) =
            answer(&app, user, json!({"problem_id": PROBLEM_ID, "answer": ""})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["correct"], false);
        assert_eq!(body["work_quality"], "poor");
        assert_eq!(body["error_tags"], json!(["blank-answer"]));
        assert_eq!(body["re_solve"], RE_SOLVE_TEXT);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 2: the replay
// --------------------------------------------------------------------------- //

/// Spec section 4.3 step 6. The `attempt_id` is `{task_id}-{n}`, `n` being the
/// 1-based position of the attempt in the LOG, so a request whose id already
/// stands appends nothing: the partial unique index makes the INSERT a no-op and
/// the reply is `already_recorded`.
///
/// The reply is then the state READ, never the verdict this request graded. The
/// submission below is right and the standing attempt is a blank miss, so every
/// verdict field of the reply must be the stored one.
///
/// The log holds `-2` and no `-1`, which is a log this build did not write: an
/// operator repair, or a 1.0 log whose ids came from the problem id (spec
/// section 4.1). A log this build writes is dense, so the branch is a guard.
#[tokio::test]
async fn a_replayed_request_appends_nothing_and_returns_already_recorded() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "replay@example.com", served(5.0, "kp1", Vec::new())).await;
        seed_attempt(
            &db,
            user,
            2,
            "s_2026-01-01a-lesson-addition-2",
            Verdict {
                given_answer: "",
                correct: false,
                work_quality: "poor",
                error_tags: json!(["blank-answer"]),
                secs: 41,
            },
        )
        .await;

        let (status, body) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["task_status"], "already_recorded");
        assert_eq!(body["attempt_id"], "s_2026-01-01a-lesson-addition-2");
        assert_eq!(body["correct"], false);
        assert_eq!(body["work_quality"], "poor");
        assert_eq!(body["error_tags"], json!(["blank-answer"]));
        assert_eq!(body["secs"], 41);
        assert_eq!(body["next"], Value::Null);
        assert_eq!(body["remediation"], json!([]));
        // Nothing was appended (C2, FR-14).
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 1);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 3: the H3 re-solve
// --------------------------------------------------------------------------- //

/// Spec section 5.4. An assisted attempt that grades correct is NOT recorded: it
/// is stashed and the problem stays live. The next submission is the unaided
/// re-solve, and a re-solve that FAILS rewrites the stashed pass to a miss with
/// its `assisted` flag dropped, under the `-rework` id.
#[tokio::test]
async fn an_h3_re_solve_that_fails_rewrites_the_stashed_pass_to_a_miss() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let hinted = served(5.0, "kp1", vec!["Line the decimal points up.".to_string()]);
        let user = learner(&db, "rework@example.com", hinted).await;

        // The assisted pass: stashed, not recorded.
        let (status, stash) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{stash}");
        assert_eq!(stash["rework_required"], true);
        assert_eq!(stash["problem_id"], PROBLEM_ID);
        assert_eq!(stash["solution"], SOLUTION);
        assert_eq!(stash["expected"], EXPECTED_ANSWER);
        assert_eq!(stash["re_solve"], RE_SOLVE_TEXT);
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 0);
        assert!(
            stored_state(&db, user).await.served[LESSON]
                .rework
                .is_some()
        );

        // The unaided re-solve fails, so the assisted pass does not stand.
        let (status, body) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "14"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["attempt_id"], "s_2026-01-01a-lesson-addition-1-rework");
        assert_eq!(body["correct"], false);

        let recorded = events_of_type(&db, user, "attempt").await;
        assert_eq!(recorded.len(), 1);
        assert_eq!(
            recorded[0]["attempt_id"],
            "s_2026-01-01a-lesson-addition-1-rework"
        );
        assert_eq!(recorded[0]["correct"], false);
        assert_eq!(recorded[0]["assisted"], false);
        // The STASHED submission is what the log holds, not the failed re-solve.
        assert_eq!(recorded[0]["given_answer"], "13.5");
        assert_eq!(recorded[0]["work_quality"], "nearly_perfect");
    })
    .await;
}

/// An assisted attempt whose unaided re-solve SUCCEEDS records the stashed pass
/// as a pass, with its `assisted` flag kept.
#[tokio::test]
async fn an_h3_re_solve_that_succeeds_records_the_stashed_pass() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let hinted = served(5.0, "kp1", vec!["Line the decimal points up.".to_string()]);
        let user = learner(&db, "rework-ok@example.com", hinted).await;

        answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        let (status, body) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "13.50"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");

        let recorded = events_of_type(&db, user, "attempt").await;
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0]["correct"], true);
        assert_eq!(recorded[0]["assisted"], true);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 4: the timing clamp
// --------------------------------------------------------------------------- //

/// Spec section 5.6 and section 10, row "Timing cap". The topic's
/// `expected_time_secs` is 30, so the cap is 300 and an older hand-off records
/// 300 with `timing-unreliable`.
#[tokio::test]
async fn secs_clamps_at_ten_times_the_expected_time() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "slow@example.com", served(4000.0, "kp1", Vec::new())).await;

        let (status, body) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["secs"], 300);
        assert_eq!(body["error_tags"], json!(["timing-unreliable"]));

        let recorded = events_of_type(&db, user, "attempt").await;
        assert_eq!(recorded[0]["secs"], 300);
        assert_eq!(recorded[0]["error_tags"], json!(["timing-unreliable"]));
    })
    .await;
}

/// An answer inside the cap carries no timing tag at all.
#[tokio::test]
async fn an_answer_inside_the_cap_carries_no_timing_tag() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "prompt@example.com", served(12.0, "kp1", Vec::new())).await;

        let (status, body) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["error_tags"], json!([]));
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The event shape (oracle parity, HANDOVER section 2.5)
// --------------------------------------------------------------------------- //

/// The recorded `attempt` event, diffed field by field against the 1.0 shape
/// (`projector-1.0-spec.md:43`). `attempt_id` is excepted: 2.0 defines its own
/// deterministic rule (trap T12).
#[tokio::test]
async fn a_recorded_attempt_matches_the_1_0_event_shape() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "shape@example.com", served(7.0, "kp1", Vec::new())).await;

        let (status, body) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "14", "work": "8 + 5.5 = 14"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");

        let recorded = events_of_type(&db, user, "attempt").await;
        assert_eq!(recorded.len(), 1);
        let event = &recorded[0];
        let mut keys: Vec<&str> = event
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "answer_kind",
                "assisted",
                "attempt_id",
                "correct",
                "error_tags",
                "given_answer",
                "grader_note",
                "kp",
                "problem",
                "secs",
                "session",
                "task_id",
                "task_type",
                "topic",
                "ts",
                "type",
                "v",
                "work",
                "work_quality",
            ]
        );
        assert_eq!(event["task_id"], LESSON);
        assert_eq!(event["topic"], "addition");
        assert_eq!(event["kp"], "kp1");
        assert_eq!(event["task_type"], "lesson");
        assert_eq!(event["answer_kind"], "numeric");
        assert_eq!(event["problem"]["text"], PROBLEM_TEXT);
        assert_eq!(event["problem"]["expected"], EXPECTED_ANSWER);
        assert_eq!(event["given_answer"], "14");
        assert_eq!(event["work"], "8 + 5.5 = 14");
        assert_eq!(event["correct"], false);
        assert_eq!(event["work_quality"], "nearly_passable");
        assert_eq!(event["error_tags"], json!([]));
        assert_eq!(event["grader_note"], "deterministic");
        assert_eq!(event["assisted"], false);
        assert_eq!(event["session"], SESSION);
        assert_eq!(event["type"], "attempt");
        assert_eq!(event["v"], 1);
        // The row's own columns carry the idempotency key and the type.
        let key = sqlx::query_scalar!(
            r#"SELECT attempt_id FROM events WHERE user_id = $1 AND type = 'attempt'"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(key.as_deref(), Some("s_2026-01-01a-lesson-addition-1"));
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The lesson advance
// --------------------------------------------------------------------------- //

/// Two correct answers in a row pass a knowledge point (`2consec`). The lesson
/// stands at its LAST knowledge point, so it closes: `task_passed`, one
/// `lesson_result` event, and the XP the core priced.
///
/// The literal XP: a lesson's base is 3.5 per knowledge point and `addition`
/// authors two, so the base is 7.0; `nearly_perfect` multiplies by 1.0.
#[tokio::test]
async fn a_second_correct_answer_at_the_last_kp_passes_the_lesson() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = common::seed_learner(&db, "pass@example.com").await;
        seed_open_session(&db, user).await;
        put_state(
            &db,
            user,
            &state_with(served(5.0, "kp2", Vec::new()), 1, false),
        )
        .await;

        // One earlier correct answer at kp2 already stands in the log.
        let prior = json!({
            "type": "attempt",
            "ts": "2026-01-01T00:00:10Z",
            "session": SESSION,
            "v": 1,
            "attempt_id": "s_2026-01-01a-lesson-addition-0",
            "task_id": LESSON,
            "topic": "addition",
            "kp": "kp2",
            "task_type": "lesson",
            "problem": {"text": "Compute 40 + 2.5.", "expected": "42.5"},
            "given_answer": "42.5",
            "correct": true,
            "secs": 9,
            "error_tags": [],
            "work_quality": "nearly_perfect",
        });
        let ts = DateTime::<Utc>::from_timestamp_micros(BASE_US + 10_000_000).unwrap();
        sqlx::query!(
            r#"
            INSERT INTO events (user_id, seq, ts, type, session_id, v, attempt_id, payload)
            VALUES ($1, 2, $2, 'attempt', $3, 1, 's_2026-01-01a-lesson-addition-0', $4)
            "#,
            user,
            ts,
            SESSION,
            prior
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let (status, body) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["task_status"], "task_passed");
        assert_eq!(body["xp"], 7.0);
        assert_eq!(body["next"], Value::Null);

        let closes = events_of_type(&db, user, "lesson_result").await;
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["topic"], "addition");
        assert_eq!(closes[0]["passed"], true);
        assert_eq!(closes[0]["xp"], 7.0);
        assert_eq!(closes[0]["quality_tier"], "nearly_perfect");

        // The closed task drops its whole scratch.
        let scratch = stored_state(&db, user).await;
        assert!(scratch.tasks[LESSON].done);
        assert!(!scratch.served.contains_key(LESSON));
    })
    .await;
}

/// Five wrong answers fail the knowledge point (`lesson.fail_after` is 5). The
/// lesson closes `task_failed`, prices 0.0 XP at the `nearly_passable` tier
/// (3.5 × 1 knowledge point × 0.3 is 1.05), and queues one lesson-fail
/// remediation.
#[tokio::test]
async fn the_fifth_wrong_answer_fails_the_lesson_and_queues_remediation() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = common::seed_learner(&db, "fail@example.com").await;
        seed_open_session(&db, user).await;
        put_state(
            &db,
            user,
            &state_with(served(20.0, "kp1", Vec::new()), 4, false),
        )
        .await;

        for n in 0..4_i64 {
            let payload = json!({
                "type": "attempt",
                "ts": "2026-01-01T00:00:10Z",
                "session": SESSION,
                "v": 1,
                "attempt_id": format!("{LESSON}-seed-{n}"),
                "task_id": LESSON,
                "topic": "addition",
                "kp": "kp1",
                "task_type": "lesson",
                "problem": {"text": PROBLEM_TEXT, "expected": EXPECTED_ANSWER},
                "given_answer": "14",
                "correct": false,
                "secs": 20,
                "error_tags": [],
                "work_quality": "nearly_passable",
            });
            let ts = DateTime::<Utc>::from_timestamp_micros(BASE_US + 10_000_000).unwrap();
            let key = format!("{LESSON}-seed-{n}");
            sqlx::query!(
                r#"
                INSERT INTO events (user_id, seq, ts, type, session_id, v, attempt_id, payload)
                VALUES ($1, $2, $3, 'attempt', $4, 1, $5, $6)
                "#,
                user,
                n + 2,
                ts,
                SESSION,
                key,
                payload
            )
            .execute(&db.admin)
            .await
            .unwrap();
        }

        let (status, body) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "14"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["task_status"], "task_failed");
        assert_eq!(body["xp"], 1.05);
        assert_eq!(
            body["remediation"],
            json!([{"kind": "lesson_fail", "targets": []}])
        );

        let closes = events_of_type(&db, user, "lesson_result").await;
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["passed"], false);
        assert_eq!(closes[0]["failed_at_kp"], "kp1");
        assert_eq!(
            events_of_type(&db, user, "remediation_triggered")
                .await
                .len(),
            1
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The refusals
// --------------------------------------------------------------------------- //

/// Spec section 10. The caps are 4,000 answer characters and 20,000 work
/// characters; over either one the route is `413 answer_too_large`.
#[tokio::test]
async fn the_input_caps_are_413_answer_too_large() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "large@example.com", served(5.0, "kp1", Vec::new())).await;

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/answer"),
            Some(user),
            Some(json!({"problem_id": PROBLEM_ID, "answer": "1".repeat(4_001)})),
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "answer_too_large");

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/answer"),
            Some(user),
            Some(json!({
                "problem_id": PROBLEM_ID,
                "answer": "13.5",
                "work": "w".repeat(20_001),
            })),
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "answer_too_large");
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 0);
    })
    .await;
}

/// Spec section 10. A superseded `problem_id` is `404 unknown_problem` and a
/// closed task is `409 task_complete`. A body with no `problem_id` is
/// `422 invalid_request`, and a request with no tenant is `401 unauthorized`.
#[tokio::test]
async fn the_validate_refusals_are_the_pinned_literals() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "refuse@example.com", served(5.0, "kp1", Vec::new())).await;

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/answer"),
            None,
            Some(json!({"problem_id": PROBLEM_ID, "answer": "13.5"})),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "unauthorized");

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/answer"),
            Some(user),
            Some(json!({"answer": "13.5"})),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "invalid_request");

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/answer"),
            Some(user),
            Some(json!({"problem_id": "p0000000000000000000000000000009"})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "unknown_problem");

        put_state(
            &db,
            user,
            &state_with(served(5.0, "kp1", Vec::new()), 0, true),
        )
        .await;
        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/answer"),
            Some(user),
            Some(json!({"problem_id": PROBLEM_ID, "answer": "13.5"})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "task_complete");
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 0);
    })
    .await;
}

/// Spec section 5.1. A `multi-step` or a `proof` answer gets NO synchronous
/// verdict: the route refuses it, records nothing, and asks no model.
#[tokio::test]
async fn an_undecidable_kind_gets_no_verdict_and_records_nothing() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let mut live = served(5.0, "kp1", Vec::new());
        live.answer_kind = Some("proof".to_string());
        let user = learner(&db, "proof@example.com", live).await;

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/answer"),
            Some(user),
            Some(json!({"problem_id": PROBLEM_ID, "answer": "Assume the contrary."})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "undecidable_kind");
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 0);
    })
    .await;
}

/// The handler never panics on any body. Each one of these is an envelope, never
/// a 500 and never a dropped connection.
#[tokio::test]
async fn a_hostile_body_never_panics() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "hostile@example.com", served(5.0, "kp1", Vec::new())).await;

        let bodies = [
            json!([]),
            json!("text"),
            json!(7),
            json!({"problem_id": 5}),
            json!({"problem_id": PROBLEM_ID, "answer": 5}),
            json!({"problem_id": PROBLEM_ID, "answer": "13.5", "work": []}),
            json!({"problem_id": PROBLEM_ID, "answer": "13.5", "assisted": "yes"}),
        ];
        for body in bodies {
            let (status, raw) = call(
                &app,
                Method::POST,
                &format!("/api/task/{LESSON}/answer"),
                Some(user),
                Some(body.clone()),
            )
            .await;
            assert!(
                status == StatusCode::OK || status.is_client_error(),
                "{body} gave {status}: {raw}"
            );
        }

        // A request with no body at all is the same refusal, not a panic.
        let (status, raw) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/answer"),
            Some(user),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{raw}");
        assert_eq!(parse(&raw)["error"]["code"], "invalid_request");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The recorded session stream (oracle parity, HANDOVER section 2.5)
// --------------------------------------------------------------------------- //

/// Every event of `user`, oldest first, as `(seq, type, payload)`.
async fn event_stream(db: &TestDb, user: Uuid) -> Vec<(i64, String, Value)> {
    sqlx::query!(
        r#"
        SELECT seq AS "seq!", type AS "type!", payload AS "payload!"
        FROM events WHERE user_id = $1 ORDER BY seq
        "#,
        user
    )
    .fetch_all(&db.admin)
    .await
    .unwrap()
    .into_iter()
    .map(|row| (row.seq, row.r#type, row.payload))
    .collect()
}

/// The sorted key list of one event payload.
fn keys_of(event: &Value) -> Vec<String> {
    let mut keys: Vec<String> = event.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

/// Put four wrong answers at `kp1` of the lesson into the log, at `seq` 2 to 5.
async fn seed_four_misses(db: &TestDb, user: Uuid) {
    for n in 0..4_i64 {
        let key = format!("{LESSON}-seed-{n}");
        let payload = json!({
            "type": "attempt",
            "ts": "2026-01-01T00:00:10Z",
            "session": SESSION,
            "v": 1,
            "attempt_id": key,
            "task_id": LESSON,
            "topic": "addition",
            "kp": "kp1",
            "task_type": "lesson",
            "problem": {"text": PROBLEM_TEXT, "expected": EXPECTED_ANSWER},
            "given_answer": "14",
            "correct": false,
            "secs": 20,
            "error_tags": [],
            "work_quality": "nearly_passable",
        });
        let ts = DateTime::<Utc>::from_timestamp_micros(BASE_US + 10_000_000).unwrap();
        sqlx::query!(
            r#"
            INSERT INTO events (user_id, seq, ts, type, session_id, v, attempt_id, payload)
            VALUES ($1, $2, $3, 'attempt', $4, 1, $5, $6)
            "#,
            user,
            n + 2,
            ts,
            SESSION,
            key,
            payload
        )
        .execute(&db.admin)
        .await
        .unwrap();
    }
}

/// The WHOLE stream one recorded session writes, diffed field by field against
/// the 1.0 shape (`docs/reference/projector-1.0-spec.md:34-46`).
///
/// The closing miss appends three events in one transaction, so this case reads
/// all three 1.0 shapes the grade path produces: `attempt`, `lesson_result` and
/// `remediation_triggered`. The envelope of every one of them is `ts`, `session`
/// and `v` (`model.py:212-218`), and `type` is the union discriminator.
/// `attempt_id` is excepted from the diff: 2.0 defines its own deterministic
/// rule (trap T12).
#[tokio::test]
async fn the_recorded_session_stream_matches_the_1_0_event_shapes() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = common::seed_learner(&db, "stream@example.com").await;
        seed_open_session(&db, user).await;
        put_state(
            &db,
            user,
            &state_with(served(20.0, "kp1", Vec::new()), 4, false),
        )
        .await;
        seed_four_misses(&db, user).await;

        let (status, body) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "14", "work": "8 + 5.5 = 14"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");

        let stream = event_stream(&db, user).await;
        let shape: Vec<(i64, &str)> = stream
            .iter()
            .map(|(seq, kind, _)| (*seq, kind.as_str()))
            .collect();
        assert_eq!(
            shape,
            vec![
                (1, "session_start"),
                (2, "attempt"),
                (3, "attempt"),
                (4, "attempt"),
                (5, "attempt"),
                (6, "attempt"),
                (7, "lesson_result"),
                (8, "remediation_triggered"),
            ]
        );

        // The three events this ONE request appended, in the order it appended
        // them.
        let attempt = &stream[5].2;
        let result = &stream[6].2;
        let remediation = &stream[7].2;

        assert_eq!(
            keys_of(attempt),
            vec![
                "answer_kind",
                "assisted",
                "attempt_id",
                "correct",
                "error_tags",
                "given_answer",
                "grader_note",
                "kp",
                "problem",
                "secs",
                "session",
                "task_id",
                "task_type",
                "topic",
                "ts",
                "type",
                "v",
                "work",
                "work_quality",
            ]
        );
        assert_eq!(attempt["type"], "attempt");
        assert_eq!(attempt["task_id"], LESSON);
        assert_eq!(attempt["topic"], "addition");
        assert_eq!(attempt["kp"], "kp1");
        assert_eq!(attempt["task_type"], "lesson");
        assert_eq!(attempt["answer_kind"], "numeric");
        assert_eq!(attempt["problem"]["text"], PROBLEM_TEXT);
        assert_eq!(attempt["problem"]["expected"], EXPECTED_ANSWER);
        assert_eq!(attempt["given_answer"], "14");
        assert_eq!(attempt["work"], "8 + 5.5 = 14");
        assert_eq!(attempt["correct"], false);
        assert_eq!(attempt["work_quality"], "nearly_passable");
        assert_eq!(attempt["error_tags"], json!([]));
        assert_eq!(attempt["grader_note"], "deterministic");
        assert_eq!(attempt["assisted"], false);
        assert_eq!(attempt["session"], SESSION);
        assert_eq!(attempt["v"], 1);

        assert_eq!(
            keys_of(result),
            vec![
                "assisted",
                "failed_at_kp",
                "passed",
                "quality_tier",
                "session",
                "topic",
                "ts",
                "type",
                "v",
                "xp",
            ]
        );
        assert_eq!(result["type"], "lesson_result");
        assert_eq!(result["topic"], "addition");
        assert_eq!(result["passed"], false);
        assert_eq!(result["failed_at_kp"], "kp1");
        assert_eq!(result["xp"], 1.05);
        assert_eq!(result["quality_tier"], "nearly_passable");
        assert_eq!(result["assisted"], false);
        assert_eq!(result["session"], SESSION);
        assert_eq!(result["v"], 1);

        assert_eq!(
            keys_of(remediation),
            vec![
                "kind",
                "session",
                "source_topic",
                "targets",
                "ts",
                "type",
                "v",
            ]
        );
        assert_eq!(remediation["type"], "remediation_triggered");
        assert_eq!(remediation["kind"], "lesson_fail");
        assert_eq!(remediation["source_topic"], "addition");
        assert_eq!(remediation["targets"], json!([]));
        assert_eq!(remediation["session"], SESSION);
        assert_eq!(remediation["v"], 1);

        // The envelope instant of all three is the RFC 3339 `Z` spelling of 1.0.
        for event in [attempt, result, remediation] {
            let ts = event["ts"].as_str().unwrap();
            assert!(ts.ends_with('Z'), "{ts}");
            DateTime::parse_from_rfc3339(ts).unwrap();
        }

        // The three rows carry the session on the COLUMN too, so the session
        // reader of unit U6 finds them without reading the payload.
        let tagged = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM events WHERE user_id = $1 AND session_id = $2"#,
            user,
            SESSION
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(tagged, 8);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The fold: incremental, and the full replay a `regraded` forces
// --------------------------------------------------------------------------- //

/// The second topic of the fixture curriculum. No event of these two logs ever
/// names it, so a saved model that carries it proves the fold started FROM the
/// cache, and a saved model without it proves the fold threw the cache away and
/// replayed the whole log.
const SENTINEL_TOPIC: &str = "subtraction";

/// The ability the cached model stamps on [`SENTINEL_TOPIC`]. It is a value no
/// fold of these logs can produce.
const SENTINEL_ABILITY: f64 = 0.75;

/// Cache a learner model at `through_seq` that carries [`SENTINEL_TOPIC`].
async fn poison_cache(db: &TestDb, user: Uuid, through_seq: i64) {
    let mut topics = BTreeMap::new();
    topics.insert(
        SENTINEL_TOPIC.to_string(),
        TopicState {
            ability: SENTINEL_ABILITY,
            ..TopicState::default()
        },
    );
    let model = LearnerModel {
        topics,
        ..LearnerModel::default()
    };
    sqlx::query!(
        r#"
        INSERT INTO learner_models
            (user_id, model, through_seq, projector_version, config_hash)
        VALUES ($1, $2, $3, 3, $4)
        "#,
        user,
        serde_json::to_value(&model).unwrap(),
        through_seq,
        Config::default().config_hash().unwrap()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// The cached model of `user` and the cursor it stands at.
async fn cached_model(db: &TestDb, user: Uuid) -> (Value, i64) {
    let row = sqlx::query!(
        r#"
        SELECT model AS "model!", through_seq AS "through_seq!"
        FROM learner_models WHERE user_id = $1
        "#,
        user
    )
    .fetch_one(&db.admin)
    .await
    .unwrap();
    (row.model, row.through_seq)
}

/// One `regraded` of an earlier attempt, at `seq`.
async fn seed_regraded(db: &TestDb, user: Uuid, seq: i64, attempt_id: &str) {
    let payload = json!({
        "type": "regraded",
        "ts": "2026-01-01T00:00:20Z",
        "session": SESSION,
        "v": 1,
        "task_id": LESSON,
        "topic": "addition",
        "attempts": [{
            "attempt_id": attempt_id,
            "work_quality": "poor",
            "error_tags": ["arithmetic-slip"],
            "grader_note": "operator repair",
        }],
        "reason": "an operator repair",
    });
    let ts = DateTime::<Utc>::from_timestamp_micros(BASE_US + 20_000_000).unwrap();
    sqlx::query!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, payload)
        VALUES ($1, $2, $3, 'regraded', $4, 1, $5)
        "#,
        user,
        seq,
        ts,
        SESSION,
        payload
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Spec section 4.3 step 7. The grade path folds ONE event forward from the
/// cache, and it replays the WHOLE log when the events after the cursor hold a
/// `regraded`.
///
/// Both learners start from the same poisoned cache at `seq` 1. The learner
/// whose log holds no `regraded` keeps the sentinel topic, because the fold
/// started from the cache. The learner whose log holds one loses it, because the
/// fold threw the cache away.
#[tokio::test]
async fn a_regraded_in_the_log_makes_the_grade_path_replay_the_whole_fold() {
    TestDb::with(|db| async move {
        let app = app(&db);

        // The incremental arm: session_start at 1, the new attempt at 2.
        let plain = learner(
            &db,
            "fold-plain@example.com",
            served(5.0, "kp1", Vec::new()),
        )
        .await;
        poison_cache(&db, plain, 1).await;
        let (status, body) = answer(
            &app,
            plain,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (model, cursor) = cached_model(&db, plain).await;
        assert_eq!(cursor, 2);
        assert_eq!(
            model["topics"][SENTINEL_TOPIC]["ability"],
            json!(0.75),
            "the incremental fold must start from the cached model: {model}"
        );

        // The replay arm: session_start at 1, a `regraded` at 2, the new attempt
        // at 3.
        let repaired = learner(
            &db,
            "fold-regrade@example.com",
            served(5.0, "kp1", Vec::new()),
        )
        .await;
        poison_cache(&db, repaired, 1).await;
        seed_regraded(&db, repaired, 2, "s_2026-01-01a-lesson-addition-0").await;
        let (status, body) = answer(
            &app,
            repaired,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (model, cursor) = cached_model(&db, repaired).await;
        assert_eq!(cursor, 3);
        assert!(
            model["topics"].get(SENTINEL_TOPIC).is_none(),
            "a regraded must force the full replay, which drops the cache: {model}"
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The H3 rule and the quiz reveal (trap W7)
// --------------------------------------------------------------------------- //

/// Spec section 5.4 and trap W7. A hint on the problem makes an attempt
/// reference-assisted, and so does the client flag — but never inside a quiz.
///
/// 1.0 answers a quiz in `_quiz_answer` and returns from it before the assisted
/// rule runs (`api.py:1349-1358`), so no quiz answer of 1.0 carries the flag.
/// The H3 reply names `expected` and `solution`; a quiz that could reach it
/// would hand the authored answer to any client that sends `"assisted": true`,
/// which is the pre-reveal leak trap W7 forbids.
#[test]
fn a_quiz_attempt_is_never_reference_assisted() {
    // Outside a quiz the two sources both set the flag.
    assert!(reference_assisted(TaskType::Lesson, true, 0));
    assert!(reference_assisted(TaskType::Lesson, false, 1));
    assert!(reference_assisted(TaskType::Review, true, 0));
    assert!(reference_assisted(TaskType::Review, false, 3));
    assert!(!reference_assisted(TaskType::Lesson, false, 0));

    // Inside a quiz neither source sets it.
    assert!(!reference_assisted(TaskType::Quiz, true, 0));
    assert!(!reference_assisted(TaskType::Quiz, false, 2));
    assert!(!reference_assisted(TaskType::Quiz, true, 2));
}

// --------------------------------------------------------------------------- //
// M5 U11: the deterministic-grade counter (T6, spec section 7)
// --------------------------------------------------------------------------- //

/// Read `/metrics` from the same router and return the exposition text.
async fn scrape(app: &Router) -> String {
    let (status, body) = call(app, Method::GET, "/metrics", None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// Assert that `text` holds `line`, and print the whole scrape when it does not.
fn holds(text: &str, line: &str) {
    assert!(
        text.contains(&format!("{line}\n")),
        "the scrape carries no line {line:?}:\n{text}"
    );
}

/// Four verdicts, four `result` labels, read from `GET /metrics`.
///
/// The counter is the T6 half that costs no token: it says how many decisions
/// this service took with no model call at all. Each learner below answers once,
/// so each label carries the count 1.
#[tokio::test]
async fn every_deterministic_verdict_counts_its_own_grade_label() {
    TestDb::with(|db| async move {
        let app = app(&db);

        let right = learner(
            &db,
            "count-correct@example.com",
            served(5.0, "kp1", Vec::new()),
        )
        .await;
        let (status, _) = answer(
            &app,
            right,
            json!({"problem_id": PROBLEM_ID, "answer": EXPECTED_ANSWER}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let wrong = learner(
            &db,
            "count-wrong@example.com",
            served(5.0, "kp1", Vec::new()),
        )
        .await;
        answer(
            &app,
            wrong,
            json!({"problem_id": PROBLEM_ID, "answer": "14"}),
        )
        .await;

        let empty = learner(
            &db,
            "count-blank@example.com",
            served(5.0, "kp1", Vec::new()),
        )
        .await;
        answer(
            &app,
            empty,
            json!({"problem_id": PROBLEM_ID, "answer": "   "}),
        )
        .await;

        let mut grouped = served(5.0, "kp1", Vec::new());
        grouped.expected.answer = "7329".to_string();
        let form = learner(&db, "count-notation@example.com", grouped).await;
        answer(
            &app,
            form,
            json!({"problem_id": PROBLEM_ID, "answer": "7.329"}),
        )
        .await;

        let text = scrape(&app).await;
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"correct\"} 1",
        );
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"incorrect\"} 1",
        );
        holds(&text, "cadus_deterministic_grade_total{result=\"blank\"} 1");
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"notation\"} 1",
        );
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"undecidable\"} 0",
        );
    })
    .await;
}

/// An answer kind with no deterministic verdict counts `undecidable`.
///
/// The route refuses the kind with `409 undecidable_kind` and asks no model, so
/// the refusal is the decision and the counter records it. Nothing is graded and
/// nothing is recorded, so no other label moves.
#[tokio::test]
async fn an_undecidable_kind_counts_one_undecidable_grade() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let mut live = served(5.0, "kp1", Vec::new());
        live.answer_kind = Some("proof".to_string());
        let user = learner(&db, "count-proof@example.com", live).await;

        let (status, body) = call(
            &app,
            Method::POST,
            &format!("/api/task/{LESSON}/answer"),
            Some(user),
            Some(json!({"problem_id": PROBLEM_ID, "answer": "Assume the contrary."})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");

        let text = scrape(&app).await;
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"undecidable\"} 1",
        );
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"correct\"} 0",
        );
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"incorrect\"} 0",
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The attempt number: `{task_id}-{n}` with `n` read from the log (F1, F12)
// --------------------------------------------------------------------------- //

/// Serve the lesson's problem over the route that owns the draw.
async fn serve(app: &Router, user: Uuid) -> (StatusCode, Value) {
    let (status, raw) = call(
        app,
        Method::POST,
        &format!("/api/task/{LESSON}/serve"),
        Some(user),
        None,
    )
    .await;
    (status, parse(&raw))
}

/// Switch the enrolled course. The route clears the D-S6 row and leaves the
/// session open (`api.py:833`).
async fn enroll(app: &Router, user: Uuid) -> (StatusCode, Value) {
    let (status, raw) = call(
        app,
        Method::POST,
        "/api/enroll",
        Some(user),
        Some(json!({"course": "c1"})),
    )
    .await;
    (status, parse(&raw))
}

/// The `attempt_id` column of every attempt row of `user`, oldest first.
async fn attempt_ids(db: &TestDb, user: Uuid) -> Vec<String> {
    sqlx::query_scalar!(
        r#"
        SELECT attempt_id AS "attempt_id!" FROM events
        WHERE user_id = $1 AND type = 'attempt' ORDER BY seq
        "#,
        user
    )
    .fetch_all(&db.admin)
    .await
    .unwrap()
}

/// The verdict one seeded attempt carries. Every field is a literal of the
/// caller, so a test can pin what the stored event holds and read it back from a
/// reply.
struct Verdict<'a> {
    given_answer: &'a str,
    correct: bool,
    work_quality: &'a str,
    error_tags: Value,
    secs: i64,
}

/// A wrong answer of the lesson at the `nearly_passable` tier.
fn a_miss() -> Verdict<'static> {
    Verdict {
        given_answer: "14",
        correct: false,
        work_quality: "nearly_passable",
        error_tags: json!([]),
        secs: 20,
    }
}

/// Put one attempt of the lesson into the log, at `seq`, under `attempt_id`.
async fn seed_attempt(db: &TestDb, user: Uuid, seq: i64, attempt_id: &str, verdict: Verdict<'_>) {
    seed_task_attempt(db, user, seq, LESSON, attempt_id, verdict).await;
}

/// Put one attempt of `task_id` into the log, at `seq`, under `attempt_id`.
async fn seed_task_attempt(
    db: &TestDb,
    user: Uuid,
    seq: i64,
    task_id: &str,
    attempt_id: &str,
    verdict: Verdict<'_>,
) {
    let payload = json!({
        "type": "attempt",
        "ts": "2026-01-01T00:00:10Z",
        "session": SESSION,
        "v": 1,
        "attempt_id": attempt_id,
        "task_id": task_id,
        "topic": "addition",
        "kp": "kp1",
        "task_type": "lesson",
        "problem": {"text": PROBLEM_TEXT, "expected": EXPECTED_ANSWER},
        "given_answer": verdict.given_answer,
        "correct": verdict.correct,
        "secs": verdict.secs,
        "error_tags": verdict.error_tags,
        "work_quality": verdict.work_quality,
    });
    let ts = DateTime::<Utc>::from_timestamp_micros(BASE_US + 10_000_000).unwrap();
    sqlx::query!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, attempt_id, payload)
        VALUES ($1, $2, $3, 'attempt', $4, 1, $5, $6)
        "#,
        user,
        seq,
        ts,
        SESSION,
        attempt_id,
        payload
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// F1. `POST /api/enroll` clears the D-S6 row and leaves the session open, so
/// the serve counter restarts while the task ids stay. `n` comes from the LOG,
/// so the answer after the enroll takes `-2` and stands in the log.
///
/// The 1-based attempt index of the task is `docs/plans/M3.md` trap T12 and spec
/// section 4.3 step 6. A counter that lives in the deletable scratch repeats
/// `-1`, and the partial unique index then discards the whole second attempt.
#[tokio::test]
async fn an_enroll_between_two_answers_numbers_the_second_attempt_from_the_log() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(
            &db,
            "enroll-clear@example.com",
            served(5.0, "kp1", Vec::new()),
        )
        .await;
        seed_pool_row(&db, user, "Compute 2 + 2.", "4", "hash-a").await;
        seed_pool_row(&db, user, "Compute 3 + 3.", "6", "hash-b").await;

        let (status, first) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "14"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(first["attempt_id"], "s_2026-01-01a-lesson-addition-1");

        let (status, switched) = enroll(&app, user).await;
        assert_eq!(status, StatusCode::OK, "{switched}");
        assert_eq!(switched["enrolled"], "c1");

        let (status, drawn) = serve(&app, user).await;
        assert_eq!(status, StatusCode::OK, "{drawn}");
        let problem_id = drawn["problem_id"].as_str().unwrap().to_string();

        let (status, second) =
            answer(&app, user, json!({"problem_id": problem_id, "answer": "0"})).await;
        assert_eq!(status, StatusCode::OK, "{second}");
        assert_eq!(second["attempt_id"], "s_2026-01-01a-lesson-addition-2");
        assert_eq!(second["task_status"], "continue");
        assert_eq!(
            attempt_ids(&db, user).await,
            vec![
                "s_2026-01-01a-lesson-addition-1".to_string(),
                "s_2026-01-01a-lesson-addition-2".to_string(),
            ]
        );
    })
    .await;
}

/// A topic id may hold a hyphen, so one task id can be the prefix of another.
/// The number of a task counts ITS OWN attempts: an attempt of the sibling task
/// `{LESSON}-extra` carries the `{LESSON}-` prefix, and it must not move this
/// lesson's number.
///
/// `assign_ids` builds `{session}-{type}-{topic}`, so the two task ids below are
/// the topics `addition` and `addition-extra` of one session.
#[tokio::test]
async fn an_attempt_of_a_sibling_task_does_not_move_this_number() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "sibling@example.com", served(5.0, "kp1", Vec::new())).await;
        seed_task_attempt(
            &db,
            user,
            2,
            "s_2026-01-01a-lesson-addition-extra",
            "s_2026-01-01a-lesson-addition-extra-1",
            a_miss(),
        )
        .await;

        let (status, body) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["attempt_id"], "s_2026-01-01a-lesson-addition-1");
        assert_eq!(body["task_status"], "continue");
    })
    .await;
}

/// F12. The chain serve -> answer -> serve -> answer, over the real routes, on a
/// log that already holds one attempt of the task and a D-S6 row that holds no
/// counter at all. The two answers take `-2` and `-3`: the number is the
/// position in the LOG, never the position in the scratch.
#[tokio::test]
async fn a_serve_answer_chain_numbers_the_attempts_from_the_log() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = common::seed_learner(&db, "chain@example.com").await;
        seed_open_session(&db, user).await;
        // The learner answered one problem of this lesson already. The scratch
        // was cleared after it, so the D-S6 row carries no serve counter.
        seed_attempt(&db, user, 2, "s_2026-01-01a-lesson-addition-1", a_miss()).await;
        put_state(&db, user, &WebState::for_session(SESSION)).await;
        seed_pool_row(&db, user, "Compute 2 + 2.", "4", "hash-a").await;
        seed_pool_row(&db, user, "Compute 3 + 3.", "6", "hash-b").await;

        let (status, first_problem) = serve(&app, user).await;
        assert_eq!(status, StatusCode::OK, "{first_problem}");
        let first_id = first_problem["problem_id"].as_str().unwrap().to_string();
        let (status, first) =
            answer(&app, user, json!({"problem_id": first_id, "answer": "0"})).await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(first["attempt_id"], "s_2026-01-01a-lesson-addition-2");

        let (status, second_problem) = serve(&app, user).await;
        assert_eq!(status, StatusCode::OK, "{second_problem}");
        let second_id = second_problem["problem_id"].as_str().unwrap().to_string();
        // The chain answers two DIFFERENT problems, so the two attempts are two
        // attempts and not one request sent twice.
        assert_ne!(first_id, second_id);
        let (status, second) =
            answer(&app, user, json!({"problem_id": second_id, "answer": "0"})).await;
        assert_eq!(status, StatusCode::OK, "{second}");
        assert_eq!(second["attempt_id"], "s_2026-01-01a-lesson-addition-3");

        assert_eq!(
            attempt_ids(&db, user).await,
            vec![
                "s_2026-01-01a-lesson-addition-1".to_string(),
                "s_2026-01-01a-lesson-addition-2".to_string(),
                "s_2026-01-01a-lesson-addition-3".to_string(),
            ]
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The repeat-fail peel-back reads HISTORY (M5 review 2, finding V2)
// --------------------------------------------------------------------------- //

/// The session that closed one day before [`SESSION`].
const EARLIER: &str = "s_2025-12-31a";

/// The Unix microsecond instant of 2025-12-31T00:00:00Z.
const EARLIER_US: i64 = BASE_US - 86_400_000_000;

/// Put one literal event of `user` into the log, at `seq`.
async fn seed_event(db: &TestDb, user: Uuid, seq: i64, ts_us: i64, session: &str, payload: Value) {
    let ts = DateTime::<Utc>::from_timestamp_micros(ts_us).unwrap();
    let kind = payload["type"].as_str().unwrap().to_owned();
    sqlx::query!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, payload)
        VALUES ($1, $2, $3, $4, $5, 1, $6)
        "#,
        user,
        seq,
        ts,
        kind,
        session,
        payload
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// V2. A lesson that failed at `kp1` in an EARLIER session peels back to the key
/// prerequisites of that knowledge point when it fails a second time.
///
/// The second failure of one lesson is only reachable in a later session: a
/// lesson task id is `{session}-lesson-{topic}`, and the failed task is `done`
/// in the D-S6 row for the rest of its own session. The open-session window
/// therefore never holds the earlier `lesson_result`, and the repeat test has to
/// read the whole-log map of the session view.
///
/// The literals: the reply queues `repeat_fail` on `subtraction`, the log holds
/// ONE `remediation_triggered` of that kind, and the second `lesson_result`
/// stands beside the first.
#[tokio::test]
async fn a_lesson_failed_in_an_earlier_session_peels_back_on_the_second_failure() {
    TestDb::with(|db| async move {
        let app = router(&db, graph_with_key_prereq());
        let user = common::seed_learner(&db, "repeat-fail@example.com").await;

        // Session one: the lesson failed at `kp1`, and the session closed.
        seed_event(
            &db,
            user,
            1,
            EARLIER_US,
            EARLIER,
            json!({
                "type": "session_start",
                "ts": "2025-12-31T00:00:00Z",
                "session": EARLIER,
                "v": 1,
            }),
        )
        .await;
        seed_event(
            &db,
            user,
            2,
            EARLIER_US + 60_000_000,
            EARLIER,
            json!({
                "type": "lesson_result",
                "ts": "2025-12-31T00:01:00Z",
                "session": EARLIER,
                "v": 1,
                "topic": "addition",
                "passed": false,
                "failed_at_kp": "kp1",
                "xp": 1.05,
                "quality_tier": "nearly_passable",
                "assisted": false,
            }),
        )
        .await;
        seed_event(
            &db,
            user,
            3,
            EARLIER_US + 120_000_000,
            EARLIER,
            json!({
                "type": "session_end",
                "ts": "2025-12-31T00:02:00Z",
                "session": EARLIER,
                "v": 1,
                "xp_earned": 1.05,
                "minutes": 2.0,
            }),
        )
        .await;

        // Session two: the same lesson, with four misses at `kp1` behind it.
        seed_event(
            &db,
            user,
            4,
            BASE_US,
            SESSION,
            json!({
                "type": "session_start",
                "ts": "2026-01-01T00:00:00Z",
                "session": SESSION,
                "v": 1,
            }),
        )
        .await;
        for n in 0..4_i64 {
            seed_attempt(&db, user, n + 5, &format!("{LESSON}-seed-{n}"), a_miss()).await;
        }
        put_state(
            &db,
            user,
            &state_with(served(20.0, "kp1", Vec::new()), 4, false),
        )
        .await;

        // The fifth miss fails `kp1` a SECOND time.
        let (status, body) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "14"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["task_status"], "task_failed");
        assert_eq!(
            body["remediation"],
            json!([{"kind": "repeat_fail", "targets": ["subtraction"]}])
        );

        let queued = events_of_type(&db, user, "remediation_triggered").await;
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0]["kind"], "repeat_fail");
        assert_eq!(queued[0]["source_topic"], "addition");
        assert_eq!(queued[0]["targets"], json!(["subtraction"]));

        let closes = events_of_type(&db, user, "lesson_result").await;
        assert_eq!(closes.len(), 2);
        assert_eq!(closes[1]["passed"], false);
        assert_eq!(closes[1]["failed_at_kp"], "kp1");
    })
    .await;
}

/// The FIRST failure of a lesson still queues the plain `lesson_fail`, even when
/// the failed knowledge point names a key prerequisite. The peel-back is the
/// SECOND failure and nothing else.
#[tokio::test]
async fn a_first_failure_queues_the_plain_lesson_fail() {
    TestDb::with(|db| async move {
        let app = router(&db, graph_with_key_prereq());
        let user = common::seed_learner(&db, "first-fail@example.com").await;
        seed_open_session(&db, user).await;
        for n in 0..4_i64 {
            seed_attempt(&db, user, n + 2, &format!("{LESSON}-seed-{n}"), a_miss()).await;
        }
        put_state(
            &db,
            user,
            &state_with(served(20.0, "kp1", Vec::new()), 4, false),
        )
        .await;

        let (status, body) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "14"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["task_status"], "task_failed");
        assert_eq!(
            body["remediation"],
            json!([{"kind": "lesson_fail", "targets": []}])
        );
    })
    .await;
}
