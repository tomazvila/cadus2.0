//! The quiz fixture: a two-question quiz in the plan of the open session, and
//! the live questions a test puts on screen.

use std::sync::Arc;

use axum::Router;
use cadus_core::config::{Config, QuizConfig};
use cadus_core::event::{Event, LessonResult, SchemaVersion, Slug, Timestamp, WorkQuality};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::{Content, ServedProblem, WebState};
use cadus_web::{AppState, create_app};
use serde_json::{Value, json};
use sqlx::types::Uuid;

use super::{
    BASE_US, EXPECTED_ANSWER, PROBLEM_ID, PROBLEM_TEXT, SESSION, SOLUTION, exemplar,
    exemplar_with_solution, kp, lesson_problem, one_unit_curriculum, put_state, seed_event,
    seed_learner, seed_open_session, stored_state, topic,
};

/// The task id of the quiz (`assign_ids`: `{session}-{type}` with no topic).
pub const QUIZ: &str = "s_2026-01-01a-quiz";

/// The number of questions the fixture quiz asks.
///
/// The fixture curriculum authors two topics, and a quiz asks one question per
/// sampled topic, so the config asks for two. `quiz_is_due` needs at least this
/// many topics with review history, and the two seeded `lesson_result` events
/// give exactly that.
pub const QUIZ_QUESTIONS: i64 = 2;

/// The `problem_id` of the second quiz question. The first is [`PROBLEM_ID`].
pub const PROBLEM_TWO: &str = "p0000000000000000000000000000002";

/// The statement of the second quiz question.
pub const TEXT_TWO: &str = "Compute 40 - 2.5.";

/// The authored answer of the second quiz question.
pub const ANSWER_TWO: &str = "37.5";

/// The authored solution sketch of the second quiz question.
pub const SOLUTION_TWO: &str = "Take the parts away to reach 37.5.";

/// The fixture content: `addition` and `subtraction` with one exemplar each,
/// and a quiz of two questions.
pub fn quiz_content() -> Content {
    let curriculum = one_unit_curriculum(vec![
        topic(
            "addition",
            vec![kp("kp1", vec![exemplar(PROBLEM_TEXT, EXPECTED_ANSWER)])],
        ),
        topic(
            "subtraction",
            vec![kp(
                "kp1",
                vec![exemplar_with_solution(TEXT_TWO, ANSWER_TWO, SOLUTION_TWO)],
            )],
        ),
    ]);
    super::open_content_with(
        curriculum,
        Config {
            quiz: QuizConfig {
                questions: QUIZ_QUESTIONS,
                ..QuizConfig::default()
            },
            ..Config::default()
        },
    )
}

/// The router of a quiz test, with the fixture content loaded.
pub fn quiz_app(db: &TestDb) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(quiz_content())),
    )
}

/// One passed `lesson_result` on `topic`, as the log stores it. It gives the
/// topic review history, so `quiz_is_due` counts it.
pub fn passed_lesson(topic: &str) -> Value {
    serde_json::to_value(Event::LessonResult(LessonResult {
        ts: Timestamp::from_micros(BASE_US),
        session: Some(SESSION.to_string()),
        v: SchemaVersion,
        topic: Slug::new(topic).unwrap(),
        passed: true,
        failed_at_kp: None,
        xp: 10.0,
        quality_tier: WorkQuality::NearlyPerfect,
        assisted: false,
    }))
    .unwrap()
}

/// One live quiz question, five seconds old. A quiz question draws its
/// statement from its own topic, so the two topics agree (M5 review 1,
/// findings F10 and F16).
pub fn question(
    problem_id: &str,
    index: i64,
    topic: &str,
    text: &str,
    expected: &str,
    sketch: &str,
) -> ServedProblem {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.problem_id = problem_id.to_string();
    live.task_id = QUIZ.to_string();
    live.topic = Some(topic.to_string());
    live.serve_topic = Some(topic.to_string());
    live.text = text.to_string();
    live.expected.answer = expected.to_string();
    live.solution_sketch = Some(sketch.to_string());
    live.index = index;
    live
}

/// The first question of the fixture quiz.
pub fn first_question() -> ServedProblem {
    question(
        PROBLEM_ID,
        0,
        "addition",
        PROBLEM_TEXT,
        EXPECTED_ANSWER,
        SOLUTION,
    )
}

/// The second question of the fixture quiz.
pub fn second_question() -> ServedProblem {
    question(
        PROBLEM_TWO,
        1,
        "subtraction",
        TEXT_TWO,
        ANSWER_TWO,
        SOLUTION_TWO,
    )
}

/// Seed a learner whose plan carries the quiz and whose D-S6 row is empty.
///
/// The serve route then draws the first question itself, which is the path the
/// whole-quiz clock is stamped on (M6-review-2, V6).
pub async fn seed_quiz_learner(db: &TestDb, email: &str) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    seed_event(db, user, 2, BASE_US, SESSION, passed_lesson("addition")).await;
    seed_event(db, user, 3, BASE_US, SESSION, passed_lesson("subtraction")).await;
    put_state(db, user, &WebState::for_session(SESSION)).await;
    user
}

/// Seed a learner whose plan carries the quiz, with `live` as its live question.
///
/// The log opens the session and closes two lessons, so two topics have review
/// history and the quiz is due. The state document holds the served question;
/// the answer route installs the progress row itself.
pub async fn quiz_learner(db: &TestDb, email: &str, live: ServedProblem) -> Uuid {
    let user = seed_quiz_learner(db, email).await;
    put_quiz_live(db, user, live).await;
    user
}

/// Put `live` on screen as the quiz question, and keep everything else the
/// route wrote.
pub async fn put_quiz_live(db: &TestDb, user: Uuid, live: ServedProblem) {
    let mut scratch = stored_state(db, user).await;
    scratch.served.insert(QUIZ.to_string(), live);
    put_state(db, user, &scratch).await;
}

/// Write a RAW `web_states.doc` for `user`.
///
/// A test that pins how the route reads an OLDER document writes the JSON
/// itself, because a document built from the current type cannot leave a field
/// out.
pub async fn put_doc(db: &TestDb, user: Uuid, doc: Value) {
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

/// A quiz task whose every question is answered and whose task row is still
/// open, as the D-S6 row spells it.
pub fn exhausted_quiz_progress() -> cadus_web::state::TaskProgress {
    cadus_web::state::TaskProgress {
        task_id: QUIZ.to_string(),
        task_type: "quiz".to_string(),
        total: QUIZ_QUESTIONS,
        served: QUIZ_QUESTIONS,
        answered: QUIZ_QUESTIONS,
        done: false,
        current_kp: None,
    }
}

/// The quiz body has `n` keys, and no other.
pub fn assert_key_count(raw: &str, n: usize) -> Value {
    let body = super::parse(raw);
    assert_eq!(body.as_object().unwrap().len(), n, "{raw}");
    body
}

/// The re-solve instruction of D-M5-3. A quiz reply never carries it.
pub const RE_SOLVE_MARK: &str = "Study the worked solution above";

/// Fail the test if the raw body reveals any part of the graded question.
///
/// Trap W7 asks for a scan of the text, so this reads the body as text: an
/// added field, a nested object, and a stray echo all fail here, and a check of
/// named fields catches none of the three.
pub fn assert_reveals_nothing(raw: &str) {
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
        SOLUTION,
        SOLUTION_TWO,
        EXPECTED_ANSWER,
        ANSWER_TWO,
    ] {
        assert!(
            !raw.contains(secret),
            "the pre-reveal body names {secret}: {raw}"
        );
    }
}
