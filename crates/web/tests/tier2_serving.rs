//! C4 — tier-2 serving: exemplar practice without templates.
//!
//! The tier-2 courses (calculus-1 and its siblings) approve almost no
//! template, so their learners walk the A6 exemplar path of
//! `crates/web/src/serve/draw.rs`. This file pins the two behaviors that path
//! must give a tier-2 learner:
//!
//! 1. A topic with NO approved template but WITH authored exemplars serves
//!    practice from those exemplars — the learner is never told `no content`
//!    for a topic whose YAML carries exemplars.
//! 2. A knowledge point whose answer contract is `none` — or whose authored
//!    answer leaves the decidable grammar (V2) — serves its worked solution
//!    PLAINLY (the exemplar's `solution_sketch` is in the serve payload), and
//!    EVERY attempt on it grades UNGRADED: no correct/wrong verdict, and a
//!    deterministic wrong can never fire — not even for the exact authored
//!    answer. The unassisted self-check answer completes the teach-only point
//!    (the note-84 b completion arm), so a one-point lesson closes with the
//!    standing lesson XP.
//!
//! The header of `serve_routes.rs` gives the requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::{Curriculum, Exemplar};
use common::prelude::*;
use common::{
    app_with_content, events_of_type, kp, one_unit_curriculum, parse, seed_learner,
    seed_open_session, serve_ok, serve_raw, topic,
};
use serde_json::json;

const SETS: &str = "s_2026-01-01a-lesson-sets";

/// A graded exemplar: no declared contract, a decidable answer, a sketch.
fn graded(problem: &str, answer: &str, sketch: &str) -> Exemplar {
    Exemplar {
        answer_contract: None,
        problem: problem.to_string(),
        answer: answer.to_string(),
        solution_sketch: Some(sketch.to_string()),
        visual: None,
    }
}

/// A teach-only exemplar: the author declared `kind: none` — the item has no
/// deterministic assessment — and wrote the worked solution the learner reads.
fn teach_only(problem: &str, answer: &str, sketch: &str) -> Exemplar {
    Exemplar {
        answer_contract: Some(AnswerContract::None),
        problem: problem.to_string(),
        answer: answer.to_string(),
        solution_sketch: Some(sketch.to_string()),
        visual: None,
    }
}

/// A curriculum whose `sets` topic authors one knowledge point with
/// `exemplars` and no template anywhere.
fn sets(exemplars: Vec<Exemplar>) -> Curriculum {
    one_unit_curriculum(vec![topic("sets", vec![kp("kp1", exemplars)])])
}

/// The count of `serving_pool` rows of `user`, with their source tags.
async fn pool_sources(db: &TestDb, user: Uuid) -> Vec<String> {
    sqlx::query_scalar("SELECT source FROM serving_pool WHERE user_id = $1 ORDER BY id")
        .bind(user)
        .fetch_all(&db.admin)
        .await
        .unwrap()
}

// --------------------------------------------------------------------------- //
// (a) a topic with no template serves its exemplars
// --------------------------------------------------------------------------- //

/// The A6 fallback is the tier-2 practice path: no approved template, two
/// decidable authored exemplars, and the serve deals one of THEM. The served
/// item keeps the historical shape: no solution leaves the serve (Hard Rule
/// 1); its sketch waits for the grade reply.
#[tokio::test]
async fn a_topic_with_no_template_serves_practice_from_its_exemplars() {
    TestDb::with(|db| async move {
        let router = app_with_content(
            &db,
            sets(vec![
                graded(
                    "How many elements has the union of {1, 2} and {2, 3}?",
                    "3",
                    "The union holds 1, 2 and 3.",
                ),
                graded(
                    "How many elements has the intersection of {1, 2} and {2, 3}?",
                    "1",
                    "Only 2 is in both sets.",
                ),
            ]),
        );
        let user = seed_learner(&db, "tier2-serve@example.com").await;
        seed_open_session(&db, user).await;

        let served = serve_ok(&router, user, SETS).await;
        assert!(
            served["text"] == "How many elements has the union of {1, 2} and {2, 3}?"
                || served["text"] == "How many elements has the intersection of {1, 2} and {2, 3}?",
            "the serve dealt a problem that is no authored exemplar: {served}"
        );
        // The problems came from the exemplars: the pool rows say so.
        let sources = pool_sources(&db, user).await;
        assert_eq!(sources.len(), 2);
        assert!(sources.iter().all(|source| source == "exemplar"));
        // Hard Rule 1: a graded item reveals nothing on the serve route.
        assert!(served.get("solution").is_none(), "{served}");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// (b) a contract-`none` KP shows the worked solution and never a verdict
// --------------------------------------------------------------------------- //

/// The `kind: none` exemplar serves as a SELF-CHECK: the worked solution is in
/// the serve payload plainly, and every attempt — a wrong one, the exact
/// authored one — grades UNGRADED. No correct/wrong verdict and no
/// deterministic wrong can ever fire; the self-check answer completes the
/// teach-only point and closes the one-point lesson (note 84 b completion arm).
#[tokio::test]
async fn a_contract_none_kp_shows_the_worked_solution_and_never_a_verdict() {
    TestDb::with(|db| async move {
        let router = app_with_content(
            &db,
            sets(vec![teach_only(
                "Is the empty set a member of itself? Answer yes or no.",
                "no",
                "No set is a member of itself: membership would make it a member of itself.",
            )]),
        );
        let user = seed_learner(&db, "tier2-none@example.com").await;
        seed_open_session(&db, user).await;

        // The serve: the KP with contract `none` is NOT a dark knowledge
        // point — the exemplar serves, and the worked solution is plainly
        // shown, because there is no verdict to wait for.
        let served = serve_ok(&router, user, SETS).await;
        assert_eq!(
            served["solution"],
            "No set is a member of itself: membership would make it a member of itself.",
            "the self-check serve carried no worked solution: {served}"
        );
        assert_eq!(
            served["text"],
            "Is the empty set a member of itself? Answer yes or no."
        );
        let sources = pool_sources(&db, user).await;
        assert_eq!(sources, vec!["exemplar".to_string()]);

        // A WRONG answer takes no deterministic wrong: the attempt is
        // UNGRADED and the reply claims no correctness.
        let problem_id = served["problem_id"].as_str().unwrap().to_string();
        let graded = common::answer_task(
            &router,
            user,
            SETS,
            json!({"problem_id": problem_id, "answer": "yes"}),
        )
        .await;
        assert_eq!(graded.0, StatusCode::OK, "{}", graded.1);
        let body = graded.1;
        assert_eq!(body["outcome"], "ungraded", "{body}");
        assert!(
            body.get("correct").is_none(),
            "the self-check reply claimed a correctness: {body}"
        );
        assert!(
            body.get("re_solve").is_none(),
            "an ungraded attempt is not a miss, so no re-solve text leaves"
        );
        // The completion arm of note 84 b: the unassisted self-check answer
        // completes the teach-only point, the lesson closes at its last point,
        // and the close pays the standing lesson XP. The attempt itself stays
        // UNGRADED.
        assert_eq!(body["task_status"], "task_passed", "{body}");
        assert!(
            body["xp"].as_f64().is_some_and(|xp| xp > 0.0),
            "the lesson close paid no XP: {body}"
        );
        let closes = events_of_type(&db, user, "lesson_result").await;
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["passed"], true);

        // The EXACT authored answer takes no verdict either: the `none`
        // contract refuses before the checker reads the pair. A second learner
        // answers it, because the first one's lesson is closed.
        let other = seed_learner(&db, "tier2-none-exact@example.com").await;
        seed_open_session(&db, other).await;
        let again = serve_ok(&router, other, SETS).await;
        let problem_id = again["problem_id"].as_str().unwrap().to_string();
        let graded = common::answer_task(
            &router,
            other,
            SETS,
            json!({"problem_id": problem_id, "answer": "no"}),
        )
        .await;
        assert_eq!(graded.0, StatusCode::OK, "{}", graded.1);
        assert_eq!(graded.1["outcome"], "ungraded", "{}", graded.1);
        assert!(graded.1.get("correct").is_none());

        // The log holds each attempt as UNGRADED: no verdict event.
        for learner in [user, other] {
            let attempts = events_of_type(&db, learner, "attempt").await;
            assert_eq!(attempts.len(), 1);
            assert!(
                attempts[0]["outcome"]["ungraded"]["reason"].is_string(),
                "the attempt was not recorded as ungraded: {}",
                attempts[0]
            );
        }
    })
    .await;
}

// --------------------------------------------------------------------------- //
// (b') a missing contract on an undecidable answer is a self-check too
// --------------------------------------------------------------------------- //

/// An exemplar with NO declared contract whose answer leaves the decidable
/// grammar (V2) — the shape most tier-2 exemplars take — used to be skipped by
/// the fill and left its knowledge point dark (`409 pool_unavailable`). It now
/// serves as a self-check with the same guarantees as `kind: none`.
#[tokio::test]
async fn a_kp_with_no_decidable_authored_answer_still_serves_its_exemplar() {
    TestDb::with(|db| async move {
        let router = app_with_content(
            &db,
            sets(vec![graded(
                "Do the two sides of the table suggest the same limiting value? Answer yes or no.",
                "yes — both sides approach 2",
                "The left side closes in on 2 and so does the right.",
            )]),
        );
        let user = seed_learner(&db, "tier2-prose@example.com").await;
        seed_open_session(&db, user).await;

        let served = serve_ok(&router, user, SETS).await;
        assert_eq!(
            served["solution"], "The left side closes in on 2 and so does the right.",
            "the self-check serve carried no worked solution: {served}"
        );

        // Any answer — a wrong one included — gets no verdict at all.
        let problem_id = served["problem_id"].as_str().unwrap().to_string();
        let graded = common::answer_task(
            &router,
            user,
            SETS,
            json!({"problem_id": problem_id, "answer": "no, only the left side"}),
        )
        .await;
        assert_eq!(graded.0, StatusCode::OK, "{}", graded.1);
        assert_eq!(graded.1["outcome"], "ungraded", "{}", graded.1);
        assert!(graded.1.get("correct").is_none());
        // The point holds no decidable exemplar, so the self-check answer
        // completes it and the one-point lesson closes (note 84 b completion
        // arm).
        assert_eq!(graded.1["task_status"], "task_passed", "{}", graded.1);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// the refusals keep their edges
// --------------------------------------------------------------------------- //

/// A knowledge point that authors NO exemplar still refuses with
/// `pool_unavailable` — the self-check path never invents content (A6).
#[tokio::test]
async fn a_kp_with_no_exemplar_at_all_still_refuses_with_pool_unavailable() {
    TestDb::with(|db| async move {
        let router = app_with_content(
            &db,
            one_unit_curriculum(vec![topic("sets", vec![kp("kp1", Vec::new())])]),
        );
        let user = seed_learner(&db, "tier2-empty@example.com").await;
        seed_open_session(&db, user).await;

        let (status, body) = serve_raw(&router, user, SETS).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "pool_unavailable");
    })
    .await;
}
