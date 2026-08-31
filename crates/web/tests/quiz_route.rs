//! The quiz branch of `POST /api/task/{task_id}/answer`, over HTTP.
//!
//! Requirements: A3, C2, C3, C4, D-S6, R4. Spec
//! `docs/reference/web-service-1.0-spec.md` section 2.1, section 4.3, and trap
//! W7 of section 9: "Quiz batch reveal: no feedback and no `expected` in any
//! pre-reveal body — test by scanning raw JSON, not by reading fields."
//!
//! Fix unit FIX-M5-T, finding F13 of `docs/reviews/M5-review-1.md`. The quiz
//! branch of `crates/web/src/grade.rs` had no test at the route, so the rule
//! that a quiz reveals nothing before its batch reveal stood on one unexecuted
//! guard. This file drives it over HTTP:
//!
//! 1. the bare receipt of one quiz answer —
//!    [`a_quiz_answer_replies_with_a_bare_receipt`];
//! 2. a MISS that hides its verdict, while the log keeps it —
//!    [`a_wrong_quiz_answer_hides_its_verdict_before_the_reveal`];
//! 3. the `assisted` flag that reveals nothing either —
//!    [`an_assisted_flag_on_a_quiz_answer_reveals_nothing`];
//! 4. the last answer of the batch, and the buffer the reveal reads —
//!    [`the_last_quiz_answer_completes_the_batch_and_buffers_the_reveal`].
//!
//! Fix unit FIX2-M6-G, finding V6 of `docs/reviews/M6-review-2.md`, adds the
//! whole-quiz clock (QUIZ-budget) to the same file, because the clock is D-S6
//! state of the quiz task:
//!
//! 5. the first serve stamps the clock —
//!    [`a_quiz_serve_stamps_the_whole_quiz_clock_and_reports_no_time_gone`];
//! 6. a reload resumes it —
//!    [`a_reload_of_a_running_quiz_resumes_the_clock_and_keeps_the_first_stamp`];
//! 7. an open quiz written before the field —
//!    [`an_open_quiz_with_no_stamp_starts_its_clock_at_the_next_serve`].
//!
//! Every check scans the RAW body text, and every expected value is a LITERAL:
//! a literal status code, a literal key count, a literal remaining count, a
//! literal `correct` in the log. Nothing here is read back from the code under
//! test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use cadus_core::config::{Config, QuizConfig};
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, Exemplar, KnowledgePoint, RawCurriculum, RawUnit,
    Slug, Topic, Unit,
};
use cadus_core::event::{
    Event, LessonResult, SchemaVersion, SessionStart, Slug as EventSlug, Timestamp, WorkQuality,
};
use cadus_core::pool::PoolAnswer;
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::{Content, ServedProblem, Tenant, WebState};
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

/// The task id of the quiz (`assign_ids`: `{session}-{type}` with no topic).
const QUIZ: &str = "s_2026-01-01a-quiz";

/// The number of questions the fixture quiz asks.
///
/// The fixture curriculum authors two topics, and a quiz asks one question per
/// sampled topic, so the config asks for two. `quiz_is_due` needs at least this
/// many topics with review history, and the two seeded `lesson_result` events
/// give exactly that.
const QUIZ_QUESTIONS: i64 = 2;

/// The `problem_id` of the first quiz question.
const PROBLEM_ONE: &str = "p0000000000000000000000000000001";

/// The `problem_id` of the second quiz question.
const PROBLEM_TWO: &str = "p0000000000000000000000000000002";

/// The statement of the first quiz question.
const TEXT_ONE: &str = "Compute 8 + 5.5.";

/// The statement of the second quiz question.
const TEXT_TWO: &str = "Compute 40 - 2.5.";

/// The authored answer of the first quiz question.
const ANSWER_ONE: &str = "13.5";

/// The authored answer of the second quiz question.
const ANSWER_TWO: &str = "37.5";

/// The authored solution sketch of the first quiz question.
const SOLUTION_ONE: &str = "Add the parts to reach 13.5.";

/// The authored solution sketch of the second quiz question.
const SOLUTION_TWO: &str = "Take the parts away to reach 37.5.";

/// The re-solve instruction of D-M5-3. A quiz reply never carries it.
const RE_SOLVE_MARK: &str = "Study the worked solution above";

// --------------------------------------------------------------------------- //
// The fixture curriculum and config
// --------------------------------------------------------------------------- //

/// One knowledge point with one authored exemplar.
fn kp(id: &str, problem: &str, answer: &str, sketch: &str) -> KnowledgePoint {
    KnowledgePoint {
        id: Slug::new(id).unwrap(),
        name: format!("The {id} point"),
        key_prerequisites: Vec::new(),
        exemplars: vec![Exemplar {
            problem: problem.to_string(),
            answer: answer.to_string(),
            solution_sketch: Some(sketch.to_string()),
        }],
        constraints: None,
    }
}

/// One topic of the fixture curriculum.
fn topic(id: &str, point: KnowledgePoint) -> Topic {
    Topic {
        id: Slug::new(id).unwrap(),
        name: format!("The {id} topic"),
        core: true,
        difficulty: 0.3,
        drill: false,
        answer_kind: AnswerKind::Numeric,
        expected_time_secs: 30,
        prerequisites: Vec::new(),
        encompassings_extra: Vec::new(),
        knowledge_points: vec![point],
        diagnostic_exemplar: None,
        anki_seeds: Vec::new(),
    }
}

/// The fixture curriculum: one course, one module, two topics.
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
                    topic("addition", kp("kp1", TEXT_ONE, ANSWER_ONE, SOLUTION_ONE)),
                    topic("subtraction", kp("kp1", TEXT_TWO, ANSWER_TWO, SOLUTION_TWO)),
                ],
            },
            first_load_index: 0,
        }],
    })
    .unwrap()
}

/// The fixture content: the curriculum, and a quiz of two questions.
fn content() -> Content {
    Content {
        curriculum: graph(),
        cfg: Config {
            quiz: QuizConfig {
                questions: QUIZ_QUESTIONS,
                ..QuizConfig::default()
            },
            ..Config::default()
        },
    }
}

// --------------------------------------------------------------------------- //
// The harness
// --------------------------------------------------------------------------- //

/// The router of a test, with the fixture content loaded.
fn app(db: &TestDb) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(content())),
    )
}

/// Answer the quiz's live question. The reply comes back as RAW text, because
/// trap W7 asks for a scan of the body and not for a read of its fields.
async fn answer(app: &Router, user: Uuid, body: Value) -> (StatusCode, String) {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/task/{QUIZ}/answer"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    // The auth layer of unit U2 binds the tenant this way.
    request.extensions_mut().insert(Tenant(user));
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into())
}

/// Serve the quiz's question over HTTP. The reply comes back as RAW text, for
/// the same reason [`answer`] does.
async fn serve(app: &Router, user: Uuid) -> (StatusCode, String) {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/task/{QUIZ}/serve"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();
    request.extensions_mut().insert(Tenant(user));
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into())
}

/// The parsed JSON body of a call.
fn parse(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|err| panic!("body is not JSON: {err}\n{body}"))
}

/// Append one event to the log of `user` at line `seq`.
async fn seed_event(db: &TestDb, user: Uuid, seq: i64, event: &Event, at_us: i64) {
    let ts = DateTime::<Utc>::from_timestamp_micros(at_us).unwrap();
    sqlx::query!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, payload)
        VALUES ($1, $2, $3, $4, $5, 1, $6)
        "#,
        user,
        seq,
        ts,
        event.type_name(),
        event.session(),
        serde_json::to_value(event).unwrap()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// One passed `lesson_result` on `topic`. It gives the topic review history, so
/// `quiz_is_due` counts it.
fn passed_lesson(topic: &str) -> Event {
    Event::LessonResult(LessonResult {
        ts: Timestamp::from_micros(BASE_US),
        session: Some(SESSION.to_string()),
        v: SchemaVersion,
        topic: EventSlug::new(topic).unwrap(),
        passed: true,
        failed_at_kp: None,
        xp: 10.0,
        quality_tier: WorkQuality::NearlyPerfect,
        assisted: false,
    })
}

/// One live quiz question, five seconds old.
fn question(
    problem_id: &str,
    index: i64,
    topic: &str,
    text: &str,
    expected: &str,
    sketch: &str,
) -> ServedProblem {
    let now = Utc::now().timestamp_micros() as f64 / 1_000_000.0;
    ServedProblem {
        problem_id: problem_id.to_string(),
        task_id: QUIZ.to_string(),
        topic: Some(topic.to_string()),
        // A quiz question draws its statement from its own topic, so the two
        // topics agree here (M5 review 1, findings F10 and F16).
        serve_topic: Some(topic.to_string()),
        kp: Some("kp1".to_string()),
        answer_kind: Some("numeric".to_string()),
        text: text.to_string(),
        expected: PoolAnswer {
            v: 1,
            answer: expected.to_string(),
        },
        solution_sketch: Some(sketch.to_string()),
        started_at: now - 5.0,
        hints_given: Vec::new(),
        index,
        rework: None,
    }
}

/// Write a D-S6 document for `user`.
async fn put_state(db: &TestDb, user: Uuid, scratch: &WebState) {
    put_doc(db, user, scratch.to_doc().unwrap()).await;
}

/// Write a RAW `web_states.doc` for `user`.
///
/// A test that pins how the route reads an OLDER document writes the JSON
/// itself, because a document built from the current type cannot leave a field
/// out.
async fn put_doc(db: &TestDb, user: Uuid, doc: Value) {
    sqlx::query!(
        r#"
        INSERT INTO web_states (user_id, doc) VALUES ($1, $2)
        ON CONFLICT (user_id) DO UPDATE SET doc = excluded.doc
        "#,
        user,
        doc
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

/// Every `attempt` event of `user`, oldest first.
async fn attempts(db: &TestDb, user: Uuid) -> Vec<Value> {
    sqlx::query_scalar!(
        r#"SELECT payload AS "payload!" FROM events WHERE user_id = $1 AND type = $2 ORDER BY seq"#,
        user,
        "attempt"
    )
    .fetch_all(&db.admin)
    .await
    .unwrap()
}

/// Seed a learner whose plan carries the quiz, with `live` as its live question.
///
/// The log opens the session and closes two lessons, so two topics have review
/// history and the quiz is due. The state document holds the served question;
/// the answer route installs the progress row itself.
async fn learner(db: &TestDb, email: &str, live: ServedProblem) -> Uuid {
    let user = seed_quiz_learner(db, email).await;
    let mut scratch = WebState::for_session(SESSION);
    scratch.served.insert(QUIZ.to_string(), live);
    put_state(db, user, &scratch).await;
    user
}

/// Seed a learner whose plan carries the quiz and whose D-S6 row is empty.
///
/// The serve route then draws the first question itself, which is the path the
/// whole-quiz clock is stamped on (M6-review-2, V6).
async fn seed_quiz_learner(db: &TestDb, email: &str) -> Uuid {
    let user = db.seed_user(email).await;
    seed_event(
        db,
        user,
        1,
        &Event::SessionStart(SessionStart {
            ts: Timestamp::from_micros(BASE_US),
            session: Some(SESSION.to_string()),
            v: SchemaVersion,
        }),
        BASE_US,
    )
    .await;
    seed_event(db, user, 2, &passed_lesson("addition"), BASE_US).await;
    seed_event(db, user, 3, &passed_lesson("subtraction"), BASE_US).await;
    put_state(db, user, &WebState::for_session(SESSION)).await;
    user
}

/// The Unix instant of now, in seconds, as the D-S6 document spells it.
fn now_secs() -> f64 {
    Utc::now().timestamp_micros() as f64 / 1_000_000.0
}

/// Put the next question of the quiz live, keeping everything the route wrote.
async fn re_serve(db: &TestDb, user: Uuid, live: ServedProblem) {
    let mut scratch = stored_state(db, user).await;
    scratch.served.insert(QUIZ.to_string(), live);
    put_state(db, user, &scratch).await;
}

/// The first question of the fixture quiz.
fn first_question() -> ServedProblem {
    question(
        PROBLEM_ONE,
        0,
        "addition",
        TEXT_ONE,
        ANSWER_ONE,
        SOLUTION_ONE,
    )
}

/// The second question of the fixture quiz.
fn second_question() -> ServedProblem {
    question(
        PROBLEM_TWO,
        1,
        "subtraction",
        TEXT_TWO,
        ANSWER_TWO,
        SOLUTION_TWO,
    )
}

/// Fail the test if the raw body reveals any part of the graded question.
///
/// Trap W7 asks for a scan of the text, so this reads the body as text: an
/// added field, a nested object, and a stray echo all fail here, and a check of
/// named fields catches none of the three.
fn assert_reveals_nothing(raw: &str) {
    for secret in [
        "correct",
        "work_quality",
        "error_tags",
        "solution",
        "expected",
        "re_solve",
        "attempt_id",
        "diagnosis",
        "next",
        RE_SOLVE_MARK,
        SOLUTION_ONE,
        SOLUTION_TWO,
        ANSWER_ONE,
        ANSWER_TWO,
    ] {
        assert!(
            !raw.contains(secret),
            "the pre-reveal body names {secret}: {raw}"
        );
    }
}

// --------------------------------------------------------------------------- //
// 1. The bare receipt
// --------------------------------------------------------------------------- //

/// One quiz answer gets the bare receipt of three fields and nothing else.
///
/// The answer is CORRECT. Every other task type replies to that with a verdict,
/// a solution sketch and the next problem. A quiz replies with `accepted`, the
/// remaining count, and the batch flag.
#[tokio::test]
async fn a_quiz_answer_replies_with_a_bare_receipt() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "quiz-receipt@example.com", first_question()).await;

        let (status, raw) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ONE, "answer": ANSWER_ONE}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_reveals_nothing(&raw);

        let body = parse(&raw);
        let fields = body.as_object().unwrap();
        assert_eq!(fields.len(), 3, "{raw}");
        assert_eq!(body["accepted"], true);
        assert_eq!(body["remaining"], 1);
        assert_eq!(body["quiz_complete"], false);

        // The attempt IS recorded, and the verdict lives in the log alone.
        let log = attempts(&db, user).await;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0]["attempt_id"], "s_2026-01-01a-quiz-1");
        assert_eq!(log[0]["task_type"], "quiz");
        assert_eq!(log[0]["correct"], true);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 2. The hidden verdict
// --------------------------------------------------------------------------- //

/// A WRONG quiz answer gets the same bare receipt: no verdict, no re-solve text,
/// no solution sketch. The log records the miss, and the buffer holds the
/// hidden sketch for the batch reveal.
#[tokio::test]
async fn a_wrong_quiz_answer_hides_its_verdict_before_the_reveal() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "quiz-miss@example.com", first_question()).await;

        let (status, raw) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ONE, "answer": "14"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_reveals_nothing(&raw);

        let body = parse(&raw);
        assert_eq!(body.as_object().unwrap().len(), 3, "{raw}");
        assert_eq!(body["accepted"], true);
        assert_eq!(body["quiz_complete"], false);

        let log = attempts(&db, user).await;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0]["correct"], false);
        assert_eq!(log[0]["given_answer"], "14");

        // The question left the live slot, and the buffer took the answer with
        // the sketch the reveal hands out later.
        let scratch = stored_state(&db, user).await;
        assert!(!scratch.served.contains_key(QUIZ));
        let buffered = &scratch.quizzes.get(QUIZ).unwrap().answers;
        assert_eq!(buffered.len(), 1);
        assert_eq!(buffered[0]["problem_id"], PROBLEM_ONE);
        assert_eq!(buffered[0]["correct"], false);
        assert_eq!(buffered[0]["solution_sketch"], SOLUTION_ONE);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 3. The `assisted` flag
// --------------------------------------------------------------------------- //

/// A client that sends `"assisted": true` with a CORRECT quiz answer gets the
/// bare receipt too.
///
/// On every other task type that pair is the H3 first branch, and its reply
/// names `expected` and `solution` (spec section 5.4). A quiz is never assisted,
/// so the branch does not run and the authored answer stays hidden.
#[tokio::test]
async fn an_assisted_flag_on_a_quiz_answer_reveals_nothing() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "quiz-assisted@example.com", first_question()).await;

        let (status, raw) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ONE, "answer": ANSWER_ONE, "assisted": true}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_reveals_nothing(&raw);

        let body = parse(&raw);
        assert_eq!(body.as_object().unwrap().len(), 3, "{raw}");
        assert_eq!(body["remaining"], 1);

        // The attempt is recorded once, and it is NOT flagged as assisted.
        let log = attempts(&db, user).await;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0]["assisted"], false);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 4. The last answer of the batch
// --------------------------------------------------------------------------- //

/// The last answer closes the batch: `remaining` reaches 0 and `quiz_complete`
/// turns true, still with no verdict in the body.
///
/// The buffer then holds both answers, each with its outcome and its hidden
/// sketch. That buffer is what the batch reveal reads, so it is the reveal that
/// the pre-reveal bodies withheld.
#[tokio::test]
async fn the_last_quiz_answer_completes_the_batch_and_buffers_the_reveal() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "quiz-last@example.com", first_question()).await;

        let (status, first) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_ONE, "answer": ANSWER_ONE}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(parse(&first)["remaining"], 1);
        assert_eq!(parse(&first)["quiz_complete"], false);

        re_serve(&db, user, second_question()).await;
        let (status, last) = answer(
            &app,
            user,
            json!({"problem_id": PROBLEM_TWO, "answer": "12"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{last}");
        assert_reveals_nothing(&last);

        let body = parse(&last);
        assert_eq!(body.as_object().unwrap().len(), 3, "{last}");
        assert_eq!(body["accepted"], true);
        assert_eq!(body["remaining"], 0);
        assert_eq!(body["quiz_complete"], true);

        // Two attempts stand in the log, in serve order.
        let log = attempts(&db, user).await;
        assert_eq!(log.len(), 2);
        assert_eq!(log[0]["attempt_id"], "s_2026-01-01a-quiz-1");
        assert_eq!(log[1]["attempt_id"], "s_2026-01-01a-quiz-2");

        // The task is closed, and the buffer outlives the close: it carries the
        // whole reveal.
        let scratch = stored_state(&db, user).await;
        assert!(scratch.tasks.get(QUIZ).unwrap().done);
        let buffered = &scratch.quizzes.get(QUIZ).unwrap().answers;
        assert_eq!(buffered.len(), 2);
        assert_eq!(buffered[0]["text"], TEXT_ONE);
        assert_eq!(buffered[0]["correct"], true);
        assert_eq!(buffered[0]["solution_sketch"], SOLUTION_ONE);
        assert_eq!(buffered[1]["text"], TEXT_TWO);
        assert_eq!(buffered[1]["given_answer"], "12");
        assert_eq!(buffered[1]["correct"], false);
        assert_eq!(buffered[1]["solution_sketch"], SOLUTION_TWO);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 5. The whole-quiz clock (QUIZ-budget, M6-review-2 finding V6)
// --------------------------------------------------------------------------- //

/// Overwrite the whole-quiz stamp of `user`, and give back the value written.
///
/// It is how a test moves the clock: the route reads the stamp out of the D-S6
/// row, so a stamp `secs` in the past is a quiz that has run for `secs`.
async fn move_the_quiz_clock_back(db: &TestDb, user: Uuid, secs: f64) -> f64 {
    let started_at = now_secs() - secs;
    let mut scratch = stored_state(db, user).await;
    scratch
        .quizzes
        .get_mut(QUIZ)
        .unwrap()
        .started_at
        .replace(started_at);
    put_state(db, user, &scratch).await;
    started_at
}

/// The whole-quiz clock is SERVER state, and the serve payload carries it.
///
/// The first serve of the quiz stamps the clock and reports zero seconds gone.
/// Both values come from the one instant the handler takes, so the count is an
/// exact literal and not a measurement.
///
/// The payload gains this EIGHTH key and loses none:
/// `crates/web/tests/serve_routes.rs` pins the seven keys of a lesson serve, so
/// a key emitted off a quiz fails there.
#[tokio::test]
async fn a_quiz_serve_stamps_the_whole_quiz_clock_and_reports_no_time_gone() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = seed_quiz_learner(&db, "quiz-clock-start@example.com").await;

        let (status, raw) = serve(&app, user).await;
        assert_eq!(status, StatusCode::OK, "{raw}");

        let body = parse(&raw);
        let keys: Vec<&str> = body
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            vec![
                "countdown",
                "index",
                "kp",
                "problem_id",
                "quiz_elapsed_secs",
                "text",
                "time_budget_secs",
                "total",
            ]
        );
        assert_eq!(body["quiz_elapsed_secs"], 0);
        assert_eq!(body["index"], 1);
        assert_eq!(body["total"], 2);

        // The stamp went into the quiz buffer, which is where a later serve
        // reads it from. The buffer holds no answer yet.
        let scratch = stored_state(&db, user).await;
        let buffer = scratch.quizzes.get(QUIZ).unwrap();
        assert_eq!(buffer.answers.len(), 0);
        assert!(buffer.started_at.is_some(), "the serve stamped no clock");
    })
    .await;
}

/// A RELOAD resumes the running clock, because the clock is on the wire.
///
/// A reload builds a new client, so the client-side registry of unit FIX2-M6-D
/// is empty and every clock it holds is gone. The serve answers with the seconds
/// the quiz has run, and the second serve keeps the FIRST stamp: a reload that
/// re-stamped the clock hands the whole budget back, which is the defect
/// (M6-review-2, V6).
///
/// The elapsed count is a literal 90. The handler takes one instant and the
/// stamp is 90.0 seconds before it, so the floor reads 90 unless the request
/// takes a whole second; one serve against the test database takes tens of
/// milliseconds.
#[tokio::test]
async fn a_reload_of_a_running_quiz_resumes_the_clock_and_keeps_the_first_stamp() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = seed_quiz_learner(&db, "quiz-clock-reload@example.com").await;

        let (status, first) = serve(&app, user).await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(parse(&first)["quiz_elapsed_secs"], 0);
        let problem_id = parse(&first)["problem_id"].clone();

        // 90 seconds of the quiz are gone, and the learner reloads the page.
        let started_at = move_the_quiz_clock_back(&db, user, 90.0).await;
        let (status, second) = serve(&app, user).await;
        assert_eq!(status, StatusCode::OK, "{second}");

        let body = parse(&second);
        assert_eq!(body["quiz_elapsed_secs"], 90);
        // The reload re-serves the SAME question (section 5.6), so the reload
        // spends no question either.
        assert_eq!(body["problem_id"], problem_id);
        assert_eq!(body["index"], 1);

        // The stamp did NOT move. Only the per-problem clock is re-stamped.
        let scratch = stored_state(&db, user).await;
        assert_eq!(
            scratch.quizzes.get(QUIZ).unwrap().started_at,
            Some(started_at)
        );
        assert!(scratch.served.get(QUIZ).unwrap().started_at > started_at);
    })
    .await;
}

/// An OPEN quiz written before this field keeps working, and its clock starts at
/// the next serve.
///
/// The D-S6 document is jsonb, so there is no migration: a row whose buffer
/// names no `started_at` reads as an unstamped clock. The serve stamps it and
/// reports no time gone, which is what the learner sees today.
#[tokio::test]
async fn an_open_quiz_with_no_stamp_starts_its_clock_at_the_next_serve() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = seed_quiz_learner(&db, "quiz-clock-legacy@example.com").await;

        // The row of an older build: one buffered answer, and NO clock key at
        // all. The key is removed from the raw document, not set to null.
        let mut scratch = WebState::for_session(SESSION);
        scratch
            .quizzes
            .entry(QUIZ.to_string())
            .or_default()
            .answers
            .push(json!({"problem_id": PROBLEM_ONE, "correct": true}));
        let mut doc = scratch.to_doc().unwrap();
        let removed = doc["quizzes"][QUIZ]
            .as_object_mut()
            .unwrap()
            .remove("started_at");
        assert!(removed.is_some(), "the buffer wrote no started_at key");
        assert!(!doc.to_string().contains("started_at"), "{doc}");
        put_doc(&db, user, doc).await;

        let (status, raw) = serve(&app, user).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(parse(&raw)["quiz_elapsed_secs"], 0);

        // The stamp is in now, and the buffered answer survived it.
        let stored = stored_state(&db, user).await;
        let buffer = stored.quizzes.get(QUIZ).unwrap();
        assert!(buffer.started_at.is_some());
        assert_eq!(buffer.answers.len(), 1);
        assert_eq!(buffer.answers[0]["problem_id"], PROBLEM_ONE);
    })
    .await;
}
