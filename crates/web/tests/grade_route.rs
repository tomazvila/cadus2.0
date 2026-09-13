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
//! 3. feedback records help and requires a fresh independent answer —
//!    [`feedback_records_assistance_then_independent_fresh_evidence`];
//! 4. `secs` clamps at `expected_time_secs * 10` with `timing-unreliable` —
//!    [`secs_clamps_at_ten_times_the_expected_time`].
//!
//! The other subjects of the route stand in their own files:
//! `grade_route_shape.rs` (the event shapes), `grade_route_advance.rs` (the
//! lesson advance and the fold), `grade_route_refusals.rs` (the refusals and
//! the counter), `grade_route_numbering.rs` (the attempt number) and
//! `grade_route_next.rs` (the next problem and the broken states).
//!
//! Every expected value is a LITERAL: a literal status code, a literal error
//! code, a literal tier, a literal tag, a literal count, a literal XP total.
//! Nothing here re-reads a constant from the code under test.
//!
//! Every call presents a real session cookie, and `auth::layer::tenant_layer`
//! binds the tenant from it (FIX-M5-C). No test here writes a request extension
//! by hand.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use cadus_core::curriculum::{AnswerKind, review_context_digest};
use cadus_core::event::{AttemptOutcome, TaskType, WorkQuality};
use cadus_store::test_support::TestDb;
use cadus_web::grade::{Grade, deterministic_grade, reference_assisted};
use common::{
    LESSON, PROBLEM_ID, SOLUTION, Verdict, addition_curriculum, answer_lesson as answer,
    answer_lesson_ok, events_of_type, learner_with_kp1, lesson_app as app, lesson_learner,
    lesson_problem, seed_attempt, seed_pool_row, stored_state,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// A learner whose live lesson problem already handed out one hint, so the
/// next attempt is reference-assisted (H3).
async fn hinted_learner(db: &TestDb, email: &str) -> Uuid {
    let hinted = lesson_problem(5.0, "kp1", vec!["Line the decimal points up.".to_string()]);
    lesson_learner(db, email, hinted).await
}

/// The stock re-solve instruction of D-M5-3, spelled out (spec section 5.5).
const RE_SOLVE_TEXT: &str = "Study the worked solution. Select Done studying to hide it, then solve a fresh problem without help.";

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
                outcome: AttemptOutcome::of_correct(true),
                work_quality: WorkQuality::NearlyPerfect,
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
            outcome: AttemptOutcome::of_correct(true),
            work_quality: WorkQuality::NearlyPerfect,
            error_tags: vec!["notation".to_string()],
        }
    );
    assert_eq!(
        deterministic_grade("13.5", "   ", AnswerKind::Numeric),
        Grade {
            correct: false,
            outcome: AttemptOutcome::of_correct(false),
            work_quality: WorkQuality::Poor,
            error_tags: vec!["blank-answer".to_string()],
        }
    );
    assert_eq!(
        deterministic_grade("13.5", "14", AnswerKind::Numeric),
        Grade {
            correct: false,
            outcome: AttemptOutcome::of_correct(false),
            work_quality: WorkQuality::NearlyPassable,
            error_tags: Vec::new(),
        }
    );
    // An answer outside the grammar is UNGRADED, never a pass, never a miss, and
    // never a model verdict (D-F2, audit finding c).
    assert_eq!(
        deterministic_grade("13.5", "about thirteen and a half", AnswerKind::Numeric),
        Grade {
            correct: false,
            outcome: AttemptOutcome::Ungraded {
                reason: "a name that is not a function or variable".to_owned(),
            },
            work_quality: WorkQuality::NearlyPassable,
            error_tags: Vec::new(),
        }
    );
}

/// The same verdicts over HTTP, with the reply fields of section 2.1.
#[tokio::test]
async fn a_correct_answer_replies_with_the_neutral_tier() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "correct@example.com", 5.0).await;
        let curr_digest = review_context_digest(&addition_curriculum(Vec::new())).unwrap();
        seed_pool_row(
            &db,
            user,
            "addition/kp1",
            "Compute 1 + 1.",
            "2",
            "hash-next",
            (&curr_digest, cadus_core::review_engine::DIGEST),
        )
        .await;

        let body = answer_lesson_ok(&app, user, "13.5").await;
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
        let user = learner_with_kp1(&db, "blank@example.com", 5.0).await;

        let body = answer_lesson_ok(&app, user, "").await;
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
        let user = learner_with_kp1(&db, "replay@example.com", 5.0).await;
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

        let body = answer_lesson_ok(&app, user, "13.5").await;
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

/// D-F8 records the helped answer and serves a fresh, unaided same-KP problem.
#[tokio::test]
async fn feedback_records_assistance_then_independent_fresh_evidence() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = hinted_learner(&db, "fresh-after-feedback@example.com").await;
        let curr_digest = review_context_digest(&addition_curriculum(Vec::new())).unwrap();
        seed_pool_row(
            &db,
            user,
            common::KEY,
            "Compute 4 + 5.",
            "9",
            "fresh-answer",
            (&curr_digest, cadus_core::review_engine::DIGEST),
        )
        .await;
        let (status, feedback) = answer(&app, user, "13.5").await;
        assert_eq!(status, StatusCode::OK, "{feedback}");
        assert_eq!(feedback["feedback_practice"], true);
        assert_eq!(feedback["solution"], SOLUTION);
        let live = stored_state(&db, user).await.served[LESSON].clone();
        assert_ne!(live.problem_id, PROBLEM_ID);
        assert_ne!(
            live.text,
            events_of_type(&db, user, "attempt").await[0]["problem"]["text"]
                .as_str()
                .unwrap()
        );
        assert_eq!(live.kp.as_deref(), Some("kp1"));
        assert!(live.hints_given.is_empty());
        let (status, result) = common::answer_task(
            &app,
            user,
            LESSON,
            json!({
                "problem_id": live.problem_id, "answer": live.expected.answer,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{result}");
        let events = events_of_type(&db, user, "attempt").await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["assisted"], true);
        assert_eq!(events[1]["assisted"], false);
        assert_eq!(events[1]["independent_after_feedback"], true);
        assert_ne!(events[0]["item_digest"], events[1]["item_digest"]);
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
        let user = learner_with_kp1(&db, "slow@example.com", 4000.0).await;

        let body = answer_lesson_ok(&app, user, "13.5").await;
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
        let user = learner_with_kp1(&db, "prompt@example.com", 12.0).await;

        let body = answer_lesson_ok(&app, user, "13.5").await;
        assert_eq!(body["error_tags"], json!([]));
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
