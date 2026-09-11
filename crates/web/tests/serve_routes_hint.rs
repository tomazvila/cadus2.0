//! M5 U7 acceptance, the hint ladder: acceptance checks 2 and 3 of row U7.
//!
//! 2. a stale id 404s on both answer and hint —
//!    [`a_stale_problem_id_is_404_unknown_problem_on_hint_and_on_answer`];
//! 3. a hint body never contains `expected` —
//!    [`a_hint_never_carries_the_expected_answer`].
//!
//! The header of `serve_routes.rs` gives the requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use cadus_core::instruction::{InstructionSpec, gate_hint_ladder};
use cadus_store::test_support::TestDb;
use common::{
    EXEMPLAR_TEXT_2, EXPECTED_ANSWER, KEY, LESSON, POOL_ANSWER, POOL_TEXT, PROBLEM_TEXT, REVIEW,
    assert_refused, drill_app as app, exemplar, hint_ok, hint_raw, hint_task, lesson_problem,
    lesson_state, model_calls, parse, problem_id_of, put_state, seed_content, seed_due_review,
    seed_learner, seed_open_session, seed_pending_content, serve_ok, stored_state,
};
use serde_json::{Value, json};

/// A problem id no serve ever dealt.
const STALE: &str = "0123456789abcdef0123456789abcdef";

/// Current template-backed fixtures carry the same source stamp as serving.
async fn learner_with_source(db: &TestDb, email: &str) -> sqlx::types::Uuid {
    let user = common::learner_with_pool_row(db, email).await;
    seed_source(db).await;
    sqlx::query(
        "UPDATE serving_pool AS sp
         SET content_digest = cs.digest,
             source_curriculum_digest = cs.approved_curriculum_digest,
             source_review_engine_digest = cs.approved_review_engine_digest
         FROM content_store AS cs
         WHERE sp.user_id = $1 AND cs.digest = 'hint-source'",
    )
    .bind(user)
    .execute(&db.admin)
    .await
    .unwrap();
    user
}

async fn seed_source(db: &TestDb) {
    seed_content(
        db,
        KEY,
        "template",
        "hint-source",
        json!({
            "v":1, "topic_id":"addition", "answer_kind":"numeric",
            "statement":"Compute {a} + 34.75.",
            "params":{"a":{"kind":"int","low":1,"high":30}},
            "answer_expr":"a + 139/4",
            "samples":[{"params":{"a":1},"expected":"35.75"},
                       {"params":{"a":21},"expected":"55.75"}], "space_size":30
        }),
    )
    .await;
}

/// Approve a two-rung ladder for `KEY`.
async fn seed_two_rungs(db: &TestDb) {
    seed_content(
        db,
        KEY,
        "hint_ladder",
        "digest-hints",
        json!({"hints": ["Line the digits up.", "Add the ones first."]}),
    )
    .await;
}

/// Ask for a hint, scan the RAW reply for the answer, and give it back parsed.
async fn hint_scanned(app: &axum::Router, user: sqlx::types::Uuid, problem_id: &str) -> Value {
    let (status, raw) = hint_raw(app, user, LESSON, json!({"problem_id": problem_id})).await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert!(!raw.contains("expected"), "the hint leaked expected: {raw}");
    assert!(
        !raw.contains(POOL_ANSWER),
        "the hint leaked the answer text: {raw}"
    );
    parse(&raw)
}

// --------------------------------------------------------------------------- //
// Acceptance 2: the stale id
// --------------------------------------------------------------------------- //

/// A `problem_id` that is not the task's current one is `404 unknown_problem` on
/// the hint route, and the SAME rule refuses it on the answer route: both call
/// `WebState::validate`, which unit U8 wires into `POST /answer`.
#[tokio::test]
async fn a_stale_problem_id_is_404_unknown_problem_on_hint_and_on_answer() {
    TestDb::with(|db| async move {
        let user = learner_with_source(&db, "stale@example.com").await;
        let app = app(&db);
        seed_two_rungs(&db).await;

        let live = problem_id_of(&serve_ok(&app, user, LESSON).await);

        assert_refused(
            &hint_task(&app, user, LESSON, STALE).await,
            StatusCode::NOT_FOUND,
            "unknown_problem",
        );

        // The answer path takes the same refusal from the same function.
        let scratch = stored_state(&db, user).await;
        assert_eq!(
            scratch.validate(LESSON, STALE).unwrap_err(),
            cadus_web::state::ValidateError::UnknownProblem
        );
        assert!(scratch.validate(LESSON, &live).is_ok());

        // A hint with no `problem_id` at all is `422 invalid_request`.
        let (status, body) = hint_raw(&app, user, LESSON, json!({})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "invalid_request");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 3: the hint ladder
// --------------------------------------------------------------------------- //

/// The hint comes from the authored ladder, rung by rung, and its body carries
/// no `expected` and no answer text (trap W7). The third hint on a REVIEW adds
/// the reference lesson (`api.py:1856-1864`); a lesson never gets one.
#[tokio::test]
async fn a_hint_never_carries_the_expected_answer() {
    TestDb::with(|db| async move {
        let user = learner_with_source(&db, "hint@example.com").await;
        let app = app(&db);
        seed_two_rungs(&db).await;

        let problem_id = problem_id_of(&serve_ok(&app, user, LESSON).await);

        let first = hint_scanned(&app, user, &problem_id).await;
        assert_eq!(first["hint"], "Line the digits up.");
        assert_eq!(first["hint_number"], 1);
        assert_eq!(first["reference_lesson"], Value::Null);

        let second = hint_scanned(&app, user, &problem_id).await;
        assert_eq!(second["hint"], "Add the ones first.");
        assert_eq!(second["hint_number"], 2);

        // A ladder that runs out repeats its last rung, and no model is asked.
        let third = hint_scanned(&app, user, &problem_id).await;
        assert_eq!(third["hint"], "Add the ones first.");
        assert_eq!(third["hint_number"], 3);
        // A LESSON never escalates; only a review and a multi-step part do.
        assert_eq!(third["reference_lesson"], Value::Null);

        // The three hints are recorded, so the H3 rule of unit U8 sees them.
        let stored = stored_state(&db, user).await;
        assert_eq!(stored.served[LESSON].hints_given.len(), 3);
    })
    .await;
}

/// The third hint on a review points the learner at the reference lesson, with
/// the topic id and the topic name (`api.py:1856-1864`).
#[tokio::test]
async fn the_third_hint_on_a_review_escalates_to_the_reference_lesson() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "escalate@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_source(&db).await;
        seed_content(
            &db,
            KEY,
            "hint_ladder",
            "digest-hints",
            json!({"hints": ["One."]}),
        )
        .await;

        // Two hints already taken: the next one is the third.
        let mut live = lesson_problem(1.0, "kp1", vec!["One.".to_string(), "One.".to_string()]);
        live.problem_id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
        live.task_id = REVIEW.to_string();
        live.text = POOL_TEXT.to_string();
        live.expected.answer = POOL_ANSWER.to_string();
        live.solution_sketch = None;
        let curriculum_digest =
            cadus_core::curriculum::review_context_digest(&common::drill_curriculum()).unwrap();
        live.handoff = Some(cadus_web::state::ProblemHandoff {
            item_digest: cadus_core::learner::problem_text_hash(&live.text),
            item_source: cadus_core::event::ItemSource::Template,
            source_content_digest: Some("hint-source".to_string()),
            source_curriculum_digest: Some(curriculum_digest),
            source_review_engine_digest: Some(cadus_core::review_engine::DIGEST.to_string()),
            finite_case_id: None,
            finite_case_role: None,
            exposure: cadus_core::event::Exposure::First,
        });
        let mut scratch = lesson_state(live, 0, false);
        scratch.tasks.remove(REVIEW);
        put_state(&db, user, &scratch).await;
        seed_due_review(&db, user).await;

        let third = hint_ok(&app, user, REVIEW, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").await;
        assert_eq!(third["hint_number"], 3);
        assert_eq!(third["reference_lesson"]["topic"], "addition");
        assert_eq!(third["reference_lesson"]["name"], "The addition topic");
    })
    .await;
}

/// A knowledge point with no approved ladder is `409 no_hint_ladder`: 2.0 asks
/// no model for one (T1).
#[tokio::test]
async fn a_knowledge_point_with_no_approved_ladder_refuses_the_hint() {
    TestDb::with(|db| async move {
        let user = learner_with_source(&db, "noladder@example.com").await;
        let app = app(&db);

        let problem_id = problem_id_of(&serve_ok(&app, user, LESSON).await);

        assert_refused(
            &hint_task(&app, user, LESSON, &problem_id).await,
            StatusCode::CONFLICT,
            "no_hint_ladder",
        );

        // A `pending` ladder is not an approved one (C6).
        seed_pending_content(
            &db,
            KEY,
            "hint_ladder",
            "digest-pending",
            json!({"hints": ["Never served."]}),
        )
        .await;

        let (status, body) = hint_raw(&app, user, LESSON, json!({"problem_id": problem_id})).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "no_hint_ladder");
        assert!(!body.contains("Never served."), "a pending rung was served");
    })
    .await;
}

/// M6 R6 acceptance, the third check, the L5 half: an APPROVED hint ladder
/// serves through the M5 hint route with no model call.
///
/// The teach test of `serve_routes_teach.rs` proves the L4 half. This one
/// proves the L5 half over the same rule, because one gate output feeds one
/// route reader: `gate_hint_ladder` writes the ladder, the route reads it with
/// `deny_unknown_fields`, and the two are one type
/// (`cadus_core::instruction::HintLadder`).
///
/// It also proves the give-away rule end to end. The gate refuses a rung that
/// names an exemplar's answer at authoring time; here the SERVED problem is a
/// pool row whose answer is `POOL_ANSWER`, and the rungs the route hands back
/// carry neither that answer nor `expected`.
#[tokio::test]
async fn an_approved_hint_ladder_from_the_gate_serves_with_no_model_call() {
    TestDb::with(|db| async move {
        let user = learner_with_source(&db, "authoredladder@example.com").await;
        let app = app(&db);

        // The tool arguments of one authoring attempt, as the model emits them.
        let arguments = r#"{
            "hints": [
                "Which column do you line up first?",
                "Write the whole number with a decimal point and two zeros after it.",
                "Add the hundredths, then the tenths, then the ones."
            ]
        }"#;
        let exemplars = vec![
            exemplar(PROBLEM_TEXT, EXPECTED_ANSWER),
            exemplar(EXEMPLAR_TEXT_2, "13.25"),
        ];
        let ladder = gate_hint_ladder(
            arguments,
            &InstructionSpec {
                exemplars: &exemplars,
                instance_answers: Vec::new(),
            },
        )
        .expect("the gate accepts the ladder");
        let body = serde_json::to_value(&ladder).unwrap();
        seed_content(&db, KEY, "hint_ladder", "sha256:authored-ladder", body).await;

        assert_eq!(model_calls(&db).await, 0);

        let problem_id = problem_id_of(&serve_ok(&app, user, LESSON).await);

        let first = hint_scanned(&app, user, &problem_id).await;
        assert_eq!(first["hint"], "Which column do you line up first?");
        assert_eq!(first["hint_number"], 1);

        let second = hint_scanned(&app, user, &problem_id).await;
        assert_eq!(
            second["hint"],
            "Write the whole number with a decimal point and two zeros after it."
        );
        assert_eq!(second["hint_number"], 2);

        // T1: the two rungs cost no model token.
        assert_eq!(model_calls(&db).await, 0);
    })
    .await;
}
