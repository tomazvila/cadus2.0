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
//! Unit U2 writes the credential reader, so this file puts the `Tenant` into the
//! request extensions the way the auth layer will.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, Exemplar, KnowledgePoint, RawCurriculum, RawUnit,
    Slug, Topic, Unit,
};
use cadus_core::event::{Event, SchemaVersion, SessionStart, Timestamp};
use cadus_core::pool::{PoolAnswer, PoolProblem};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::grade::{Grade, deterministic_grade};
use cadus_web::state::{Content, ServedProblem, TaskProgress, Tenant, WebState};
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
/// authors two knowledge points.
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
                topics: vec![
                    topic(
                        "addition",
                        vec![
                            kp("kp1", vec![exemplar(PROBLEM_TEXT, EXPECTED_ANSWER)]),
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
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(Content::new(graph()))),
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
        request.extensions_mut().insert(Tenant(user));
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
    let user = db.seed_user(email).await;
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

/// Spec section 4.3 step 6. The `attempt_id` is `{task_id}-{n}` over the SERVED
/// problem, so a replayed request computes the same id, the partial unique index
/// makes the INSERT a no-op, and the reply says `already_recorded` with nothing
/// appended.
#[tokio::test]
async fn a_replayed_request_appends_nothing_and_returns_already_recorded() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "replay@example.com", served(5.0, "kp1", Vec::new())).await;

        let (status, first) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(first["task_status"], "continue");
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 1);

        // The request comes again against the same served problem: a stale tab,
        // or a client that lost the first reply.
        put_state(
            &db,
            user,
            &state_with(served(5.0, "kp1", Vec::new()), 0, false),
        )
        .await;
        let (status, second) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{second}");
        assert_eq!(second["task_status"], "already_recorded");
        assert_eq!(second["attempt_id"], "s_2026-01-01a-lesson-addition-1");
        assert_eq!(second["next"], Value::Null);
        // Nothing was appended, before or after (C2, FR-14).
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
        let user = db.seed_user("pass@example.com").await;
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
        let user = db.seed_user("fail@example.com").await;
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
