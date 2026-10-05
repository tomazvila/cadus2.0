//! The written proof inside a lesson (D-PR1).
//!
//! # The knowledge-point rule
//!
//! A knowledge point is PROOF-GATED when it authors a written-proof exemplar
//! (an exemplar that gives no verdict and whose statement asks for a proof).
//! Such a point closes when BOTH hold, in this order:
//!
//! 1. its decided items pass the standing rule (`2consec|3of4`); a point with
//!    no decided item (all-`none`) meets this at once;
//! 2. the revision chain of its proof exemplar closes: on a model pass (a
//!    normal close, full XP), or after the revision cap and one unaided
//!    rewrite (an ASSISTED close: reduced XP and the halved FIRe credit of an
//!    assisted pass, so the review comes early).
//!
//! The proof's drafts never enter the `2consec|3of4` sequence. A chain is one
//! item with its own feedback loop: counting each needs-revision draft as a
//! miss would fail the lesson for ordinary drafting, three drafts being three
//! misses. The rule never closes a point the standing rule would not close,
//! because the proof is an extra gate after the rule's own pass.
//!
//! A free explanation (a no-key item whose statement asks for no proof) keeps
//! the note 84 (b) self-check completion.
//!
//! # Where the state lives
//!
//! The chain lives in `proof_grading_jobs`, so it outlives the D-S6 row: a
//! reload, a session end and the day rollover all find it. While a lesson
//! chain is open, the plan carries its lesson FIRST, starting at the chain's
//! knowledge point ([`carry_open`]), and the serve hands back the chain's own
//! problem ([`due_item`]).

use std::collections::BTreeMap;

use cadus_core::curriculum::{Curriculum, Exemplar};
use cadus_core::event::TaskType;
use cadus_core::selector::{SessionPlan, Task};
use cadus_store::proof_grading::{self, CONTEXT_LESSON, JobRow, NewJob};
use serde_json::{Value, json};

use super::chain::{self, Phase, REVISION_CAP};
use super::is_proof_text;
use crate::AppState;
use crate::error::ApiError;
use crate::session::store;
use crate::state::ServedProblem;
use sqlx::{Postgres, Transaction};

/// The code of an answer to a proof whose grading still runs.
pub const PROOF_GRADING_PENDING: &str = "proof_grading_pending";

/// The code of an answer to a proof that already passed.
pub const PROOF_PASSED: &str = "proof_passed";

/// The code of an answer at the cap before the solution was shown.
pub const PROOF_SOLUTION_UNSEEN: &str = "proof_solution_unseen";

/// The code of a Continue on a proof that has not passed.
pub const PROOF_NOT_PASSED: &str = "proof_not_passed";

/// The code of a blank proof.
pub const PROOF_BLANK: &str = "proof_blank";

/// The display prose of a lesson the plan carries for its open revision.
pub const CARRY_WHY: &str = "Finish the proof you are revising.";

/// The written-proof exemplar of one knowledge point, when it authors one.
#[must_use]
pub fn proof_exemplar<'g>(graph: &'g Curriculum, topic: &str, kp: &str) -> Option<&'g Exemplar> {
    let idx = graph.idx_of(topic)?;
    let kind = graph.topic(idx)?.answer_kind;
    graph
        .knowledge_points(idx)
        .iter()
        .find(|point| point.id.as_str() == kp)?
        .exemplars
        .iter()
        .find(|exemplar| exemplar.verdict_policy(kind).is_err() && is_proof_text(&exemplar.problem))
}

/// Whether every exemplar of one knowledge point gives no verdict.
#[must_use]
pub fn kp_all_undecidable(graph: &Curriculum, topic: &str, kp: &str) -> bool {
    let Some(idx) = graph.idx_of(topic) else {
        return false;
    };
    let Some(kind) = graph.topic(idx).map(|found| found.answer_kind) else {
        return false;
    };
    graph
        .knowledge_points(idx)
        .iter()
        .find(|point| point.id.as_str() == kp)
        .is_some_and(|point| {
            !point.exemplars.is_empty()
                && point
                    .exemplars
                    .iter()
                    .all(|exemplar| exemplar.verdict_policy(kind).is_err())
        })
}

/// The chain step one lesson submission takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonStep {
    /// The chain columns of the new job.
    pub job: NewJob<'static>,
    /// The job's attempt id when the submission revises a draft. A revision
    /// records no `attempt` event (the FIRST draft is the problem's one
    /// scheduling signal), so its id is the root's with a draft number.
    pub revision_attempt: Option<String>,
}

impl LessonStep {
    /// The first draft of a problem.
    const fn first() -> Self {
        Self {
            job: NewJob::first(CONTEXT_LESSON),
            revision_attempt: None,
        }
    }

    /// Whether the submission is the unaided rewrite that closes the chain.
    #[must_use]
    pub const fn rewrite(&self) -> bool {
        self.job.rewrite
    }
}

/// A `409` with `code`.
fn conflict(code: &'static str, message: &'static str) -> ApiError {
    ApiError::new(axum::http::StatusCode::CONFLICT, code, message)
}

/// Decide the chain step of one lesson submission of a written proof.
///
/// # Errors
///
/// - `400 proof_blank` — the text is blank;
/// - `409 proof_grading_pending` — the last draft is still being graded;
/// - `409 proof_passed` — the proof already passed (Continue closes the point);
/// - `409 proof_solution_unseen` — the cap is reached and the solution was
///   not shown yet;
/// - a store failure.
pub async fn lesson_step(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    served: &ServedProblem,
    answer: &str,
) -> Result<LessonStep, ApiError> {
    if answer.trim().is_empty() {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            PROOF_BLANK,
            "Write your proof before you submit it.",
        ));
    }
    let topic = served.serving_topic().unwrap_or_default();
    let kp = served.kp.as_deref().unwrap_or_default();
    let head = store(state, proof_grading::open_lesson_head(&mut **tx, topic, kp)).await?;
    let Some(head) = head else {
        return Ok(LessonStep::first());
    };
    let rows = store(state, proof_grading::chain_rows(&mut **tx, head.id)).await?;
    let attempt = rows.map_or_else(
        || format!("{}-r1", head.attempt_id),
        |rows| format!("{}-r{}", rows[0].attempt_id, rows.len()),
    );
    let next = |revision: i32, rewrite: bool| LessonStep {
        job: NewJob {
            context: CONTEXT_LESSON,
            revision_of: Some(head.id),
            revision,
            rewrite,
        },
        revision_attempt: Some(attempt.clone()),
    };
    match chain::phase_of(&head) {
        Phase::Grading => Err(conflict(
            PROOF_GRADING_PENDING,
            "Your last draft is still being checked.",
        )),
        Phase::Passed => Err(conflict(
            PROOF_PASSED,
            "This proof passed. Continue the lesson.",
        )),
        Phase::Reveal => Err(conflict(
            PROOF_SOLUTION_UNSEEN,
            "Read the solution once before you rewrite the proof.",
        )),
        Phase::Revise => Ok(next(head.revision.saturating_add(1), false)),
        // A grading that never landed costs no revision.
        Phase::Unavailable => Ok(next(head.revision, false)),
        Phase::Rewrite => Ok(next(head.revision, true)),
        Phase::Closed => Ok(LessonStep::first()),
    }
}

/// The written proof a lesson serves next at `(topic, kp)`, when one is due.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueItem {
    /// The statement.
    pub text: String,
    /// The stored key (a placeholder such as "See the solution.").
    pub answer: String,
    /// The reference solution.
    pub solution: Option<String>,
}

/// The proof a lesson owes at `(topic, kp)`: the open chain's own problem,
/// else the point's proof exemplar when the point's decided items passed
/// (the durable `proof_owed` row) or the point has none.
///
/// # Errors
///
/// Returns the store failure.
pub async fn due_item(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    graph: &Curriculum,
    topic: &str,
    kp: &str,
) -> Result<Option<DueItem>, ApiError> {
    let exemplar = proof_exemplar(graph, topic, kp);
    let head = store(state, proof_grading::open_lesson_head(&mut **tx, topic, kp)).await?;
    if let Some(head) = head {
        let text = head.payload_str("problem").unwrap_or_default().to_owned();
        let answer = exemplar.filter(|found| found.problem == text).map_or_else(
            || "See the solution.".to_owned(),
            |found| found.answer.clone(),
        );
        return Ok(Some(DueItem {
            text,
            answer,
            solution: head.payload_str("reference").map(str::to_owned),
        }));
    }
    let Some(exemplar) = exemplar else {
        return Ok(None);
    };
    let due = kp_all_undecidable(graph, topic, kp)
        || store(state, proof_grading::is_owed(&mut **tx, topic, kp)).await?;
    Ok(due.then(|| DueItem {
        text: exemplar.problem.clone(),
        answer: exemplar.answer.clone(),
        solution: exemplar.solution_sketch.clone(),
    }))
}

/// The `proof` field of a served written proof: the chain state the client
/// draws its screen from. No reference solution ever rides here.
#[must_use]
pub fn proof_field(context: &str, head: Option<&JobRow>) -> Value {
    let Some(head) = head else {
        return json!({
            "context": context,
            "phase": "draft",
            "revision": 0,
            "cap": REVISION_CAP,
            "revisions_left": REVISION_CAP,
        });
    };
    let mut field = json!({
        "context": context,
        "phase": chain::phase_of(head).as_str(),
        "job_id": head.id,
        "status": chain::status_of(head),
        "revision": head.revision,
        "cap": REVISION_CAP,
        "revisions_left": (REVISION_CAP - head.revision).max(0),
        "draft": head.payload_str("given_answer").unwrap_or_default(),
        "seen": head.seen_at.is_some(),
        "disputed": head.disputed_at.is_some(),
    });
    if let Some(grading) = head.grading() {
        field["feedback"] = json!(grading.feedback);
        field["first_unmet"] = chain::first_unmet(&grading.checks).map_or(Value::Null, |check| {
            json!({"id": check.id, "text": check.text, "evidence": check.evidence,
                   "quote_verified": check.quote_verified})
        });
    }
    field
}

/// Stamp the `proof` field on the serve payload of a written proof, and keep
/// the solution off it.
///
/// # Errors
///
/// Returns the store failure.
pub async fn stamp(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    task: &Task,
    served: &ServedProblem,
    payload: &mut Value,
) -> Result<(), ApiError> {
    if !super::is_proof_item(served) {
        return Ok(());
    }
    if let Some(map) = payload.as_object_mut() {
        map.remove("solution");
    }
    // A quiz payload keeps its exact key set: the quiz screen reads no chain.
    if task.task_type == TaskType::Quiz {
        return Ok(());
    }
    if task.task_type != TaskType::Lesson {
        payload["proof"] = proof_field(super::context_of(served), None);
        return Ok(());
    }
    let topic = served.serving_topic().unwrap_or_default();
    let kp = served.kp.as_deref().unwrap_or_default();
    let head = store(state, proof_grading::open_lesson_head(&mut **tx, topic, kp)).await?;
    payload["proof"] = proof_field(CONTEXT_LESSON, head.as_ref());
    Ok(())
}

/// The lesson points that owe or revise a written proof, as
/// `(topic, kp)`, oldest first: the open lesson chains, then the owed proofs
/// whose first draft is still to come.
///
/// # Errors
///
/// Returns the store failure.
pub async fn open_lesson_points(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
) -> Result<Vec<(String, String)>, ApiError> {
    let heads = store(
        state,
        proof_grading::open_heads(&mut **tx, &[CONTEXT_LESSON]),
    )
    .await?;
    let owed = store(state, proof_grading::owed(&mut **tx)).await?;
    Ok(lesson_points(&heads, &owed))
}

/// The `(topic, kp)` of each open lesson head, then each owed proof, once.
#[must_use]
pub fn lesson_points(heads: &[JobRow], owed: &[(String, String)]) -> Vec<(String, String)> {
    let mut points: Vec<(String, String)> = heads
        .iter()
        .filter(|head| head.context == CONTEXT_LESSON)
        .filter_map(|head| {
            Some((
                head.payload_str("topic")?.to_owned(),
                head.payload_str("kp")?.to_owned(),
            ))
        })
        .collect();
    for point in owed {
        if !points.contains(point) {
            points.push(point.clone());
        }
    }
    points
}

/// Carry every lesson point that owes or revises a written proof into
/// `plan`, FIRST, at its knowledge point.
///
/// A plan that already lists the topic's lesson moves it to the front and
/// starts it at the chain's point; one that does not gains the lesson. A
/// topic already learned carries nothing. The listing stays a pure read: this
/// changes the composed plan only (trap W3).
pub fn carry_open(
    plan: &mut SessionPlan,
    points: &[(String, String)],
    graph: &Curriculum,
    learned: &BTreeMap<String, i64>,
) {
    // Oldest last, so the oldest open point ends up first.
    for (topic, kp) in points.iter().rev() {
        let (topic, kp) = (topic.as_str(), kp.as_str());
        if learned.contains_key(topic) || graph.idx_of(topic).is_none() {
            continue;
        }
        let at = plan.tasks.iter().position(|task| {
            task.task_type == TaskType::Lesson && task.topic.as_deref() == Some(topic)
        });
        let task = match at {
            Some(at) => {
                let mut task = plan.tasks.remove(at);
                task.start_at_kp = Some(kp.to_owned());
                task
            }
            None => Task {
                task_id: format!("{}-lesson-{topic}", plan.session),
                task_type: TaskType::Lesson,
                topic: Some(topic.to_owned()),
                start_at_kp: Some(kp.to_owned()),
                why: CARRY_WHY.to_owned(),
                ..Task::default()
            },
        };
        plan.tasks.insert(0, task);
    }
}

/// The plan task that carries the lesson of `topic`.
fn lesson_task_of(plan: &SessionPlan, topic: &str) -> Option<String> {
    plan.tasks
        .iter()
        .find(|task| task.task_type == TaskType::Lesson && task.topic.as_deref() == Some(topic))
        .map(|task| task.task_id.clone())
}

/// The display name of one topic.
fn name_of(graph: &Curriculum, topic: &str) -> Option<String> {
    graph
        .idx_of(topic)
        .and_then(|idx| graph.topic(idx))
        .map(|found| found.name.clone())
}

/// The open revisions the plan names: one entry per open lesson or review
/// chain, with the plan task that carries a lesson chain, then one entry per
/// owed proof whose first draft is still to come (phase `draft`).
#[must_use]
pub fn open_items(
    heads: &[JobRow],
    owed: &[(String, String)],
    plan: &SessionPlan,
    graph: &Curriculum,
) -> Vec<Value> {
    let owed_items = owed.iter().map(|(topic, kp)| {
        json!({
            "job_id": null,
            "context": CONTEXT_LESSON,
            "topic": topic,
            "topic_name": name_of(graph, topic),
            "kp": kp,
            "phase": "draft",
            "revision": 0,
            "seen": true,
            "task_id": lesson_task_of(plan, topic),
        })
    });
    heads
        .iter()
        .map(|head| {
            let topic = head.payload_str("topic").unwrap_or_default();
            let task_id = (head.context == CONTEXT_LESSON)
                .then(|| lesson_task_of(plan, topic))
                .flatten();
            let name = name_of(graph, topic);
            json!({
                "job_id": head.id,
                "context": head.context,
                "topic": topic,
                "topic_name": name,
                "kp": head.payload_str("kp"),
                "phase": chain::phase_of(head).as_str(),
                "revision": head.revision,
                "seen": head.seen_at.is_some(),
                "task_id": task_id,
            })
        })
        .chain(owed_items)
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::proof_grading::chain::tests::row;
    use cadus_core::curriculum::{Catalog, RawCurriculum, RawUnit, Unit};

    /// The proof statement of the fixture.
    pub(crate) const PROOF: &str = "Write the full proof: the sum of two odd integers is even.";

    /// `parity`: kp1 mixes a decided exemplar with a written proof, kp2 is
    /// one written proof alone, kp3 one free explanation alone.
    pub(crate) fn graph() -> Curriculum {
        let none = json!({"kind": "none"});
        let topic = json!({
            "id": "parity", "name": "Parity", "difficulty": 0.3, "answer_kind": "numeric",
            "expected_time_secs": 30,
            "knowledge_points": [
                {"id": "kp1", "name": "kp1", "exemplars": [
                    {"problem": "Give 7.", "answer": "7"},
                    {"problem": PROOF, "answer": "See the solution.", "answer_contract": none,
                     "solution_sketch": "Write a = 2k+1, b = 2m+1."}]},
                {"id": "kp2", "name": "kp2", "exemplars": [
                    {"problem": "Prove that the square of an even integer is even.",
                     "answer": "See the solution.", "answer_contract": none}]},
                {"id": "kp3", "name": "kp3", "exemplars": [
                    {"problem": "Explain why 0 is even.", "answer": "See the solution.",
                     "answer_contract": none}]}
            ]
        });
        let catalog: Catalog =
            serde_json::from_value(json!({"courses": [{"id": "c1", "name": "c1", "order": 0}]}))
                .unwrap();
        let unit: Unit = serde_json::from_value(
            json!({"unit": "M1", "course": "c1", "module": "M1", "topics": [topic]}),
        )
        .unwrap();
        Curriculum::build(RawCurriculum {
            catalog,
            units: vec![RawUnit {
                course_id: "c1".to_owned(),
                file_name: "00-M1.yaml".to_owned(),
                unit,
                first_load_index: 0,
            }],
        })
        .unwrap()
    }

    fn plan_of(tasks: Vec<Task>) -> SessionPlan {
        SessionPlan {
            session: "s_2026-01-02a".to_owned(),
            tasks,
            ..SessionPlan::default()
        }
    }

    fn lesson(topic: &str) -> Task {
        Task {
            task_id: format!("s_2026-01-02a-lesson-{topic}"),
            topic: Some(topic.to_owned()),
            ..Task::default()
        }
    }

    /// The proof exemplar and the all-undecidable test read the authored list.
    #[test]
    fn the_proof_gate_reads_the_authored_exemplars() {
        let graph = graph();
        assert_eq!(
            proof_exemplar(&graph, "parity", "kp1").unwrap().problem,
            PROOF
        );
        assert!(!kp_all_undecidable(&graph, "parity", "kp1"));
        assert!(proof_exemplar(&graph, "parity", "kp2").is_some());
        assert!(kp_all_undecidable(&graph, "parity", "kp2"));
        assert!(
            proof_exemplar(&graph, "parity", "kp3").is_none(),
            "an explanation is no proof"
        );
    }

    /// The carry puts the open lesson first at its point, and inserts the
    /// lesson when the plan lacks it; a learned topic carries nothing.
    #[test]
    fn an_open_revision_carries_first() {
        let graph = graph();
        let head = row(1, None, Some("needs_revision"), 0);
        let points = lesson_points(std::slice::from_ref(&head), &[]);
        assert_eq!(points, [("parity".to_owned(), "kp1".to_owned())]);
        let mut plan = plan_of(vec![lesson("other"), lesson("parity")]);
        carry_open(&mut plan, &points, &graph, &BTreeMap::new());
        assert_eq!(plan.tasks[0].topic.as_deref(), Some("parity"));
        assert_eq!(plan.tasks[0].start_at_kp.as_deref(), Some("kp1"));
        assert_eq!(plan.tasks.len(), 2);
        let mut empty = plan_of(Vec::new());
        // An owed proof with no chain yet carries the same way.
        let owed = lesson_points(&[], &[("parity".to_owned(), "kp1".to_owned())]);
        carry_open(&mut empty, &owed, &graph, &BTreeMap::new());
        assert_eq!(empty.tasks[0].task_id, "s_2026-01-02a-lesson-parity");
        assert_eq!(empty.tasks[0].why, CARRY_WHY);
        let learned = BTreeMap::from([("parity".to_owned(), 1_i64)]);
        let mut done = plan_of(Vec::new());
        carry_open(&mut done, &points, &graph, &learned);
        assert!(done.tasks.is_empty());
    }
}
