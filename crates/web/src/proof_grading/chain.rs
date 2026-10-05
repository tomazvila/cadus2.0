//! The revision chain of one written-proof problem (D-PR1): its phase, its
//! versions, and the client views that read it.
//!
//! A chain is the linked list of `proof_grading_jobs` rows of one problem:
//! the first draft, then one row per resubmission, each naming the row it
//! revises in `revision_of`. The HEAD (the row with no successor) decides
//! what the learner may do next:
//!
//! | Head | Phase |
//! |---|---|
//! | `closed_at` set, or the unaided rewrite | [`Phase::Closed`] |
//! | running | [`Phase::Grading`] |
//! | verdict pass (model or human) | [`Phase::Passed`] |
//! | needs revision, fewer than [`REVISION_CAP`] revisions used | [`Phase::Revise`] |
//! | needs revision at the cap, solution not yet shown | [`Phase::Reveal`] |
//! | needs revision at the cap, solution shown once | [`Phase::Rewrite`] |
//! | failed or capped with no human verdict | [`Phase::Unavailable`] |
//!
//! # The reference solution
//!
//! A lesson or review chain shows its reference solution only after a pass
//! or once the chain is closed. The one other moment is the cap: the
//! `seen` route shows it ONCE and stamps `revealed_at`, and from then on the
//! chain asks for the unaided rewrite with the solution hidden (The Math
//! Academy Way, p.427 and p.431: look once, then rework without the
//! reference). A quiz, self-check or legacy row keeps the rule it had before
//! the loop: the solution follows the settled grading.

use std::collections::{BTreeMap, BTreeSet};

use cadus_store::proof_grading::{
    CONTEXT_LESSON, CONTEXT_REVIEW, Check, JOB_CAPPED, JOB_DONE, JOB_PENDING, JOB_RUNNING, JobRow,
    VERDICT_PASS,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

use super::{STATUS_CAPPED, STATUS_FAILED, STATUS_NEEDS_REVISION, STATUS_PASS, STATUS_PENDING};

/// How many revisions a chain allows after its first draft.
pub const REVISION_CAP: i32 = 2;

/// What the learner may do next with one chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The head is being graded.
    Grading,
    /// The head passed; a lesson applies the pass on Continue.
    Passed,
    /// The head needs revision and a revision is left.
    Revise,
    /// The cap is reached; the solution is shown once next.
    Reveal,
    /// The solution was shown; one unaided rewrite closes the chain.
    Rewrite,
    /// The worker could not grade the head; a resubmission is free.
    Unavailable,
    /// The chain is over.
    Closed,
}

impl Phase {
    /// The wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Grading => "grading",
            Self::Passed => "passed",
            Self::Revise => "revise",
            Self::Reveal => "reveal",
            Self::Rewrite => "rewrite",
            Self::Unavailable => "unavailable",
            Self::Closed => "closed",
        }
    }
}

/// Whether a context runs the revision loop.
#[must_use]
pub fn loops(context: &str) -> bool {
    context == CONTEXT_LESSON || context == CONTEXT_REVIEW
}

/// The phase one head puts its chain in.
#[must_use]
pub fn phase_of(head: &JobRow) -> Phase {
    if head.closed_at.is_some() || head.rewrite {
        return Phase::Closed;
    }
    match head.verdict() {
        Some(VERDICT_PASS) => Phase::Passed,
        Some(_) if head.revision >= REVISION_CAP => {
            if head.revealed_at.is_some() {
                Phase::Rewrite
            } else {
                Phase::Reveal
            }
        }
        Some(_) => Phase::Revise,
        None => match head.status.as_str() {
            JOB_PENDING | JOB_RUNNING => Phase::Grading,
            _ => Phase::Unavailable,
        },
    }
}

/// The poll `status` of one row.
#[must_use]
pub fn status_of(row: &JobRow) -> &'static str {
    match row.verdict() {
        Some(VERDICT_PASS) => STATUS_PASS,
        Some(_) => STATUS_NEEDS_REVISION,
        // A settled row whose result does not read is a failed grading too.
        None => match row.status.as_str() {
            JOB_PENDING | JOB_RUNNING => STATUS_PENDING,
            JOB_CAPPED => STATUS_CAPPED,
            _ => STATUS_FAILED,
        },
    }
}

/// The check the learner fixes first: the first unmet major check, else the
/// first unmet minor one.
#[must_use]
pub fn first_unmet(checks: &[Check]) -> Option<&Check> {
    checks
        .iter()
        .find(|check| !check.met && !check.minor)
        .or_else(|| checks.iter().find(|check| !check.met))
}

/// The client view of one check.
fn check_view(check: &Check) -> Value {
    json!({
        "id": check.id,
        "text": check.text,
        "met": check.met,
        "minor": check.minor,
        "evidence": check.evidence,
        "quote_verified": check.quote_verified,
    })
}

/// One problem's drafts, root first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    /// The rows, root first and head last.
    pub rows: Vec<JobRow>,
}

impl Chain {
    /// The newest draft.
    #[must_use]
    pub fn head(&self) -> &JobRow {
        // A chain is built with one row at least.
        &self.rows[self.rows.len() - 1]
    }

    /// The first draft.
    #[must_use]
    pub fn root(&self) -> &JobRow {
        &self.rows[0]
    }

    /// The phase of the head.
    #[must_use]
    pub fn phase(&self) -> Phase {
        phase_of(self.head())
    }

    /// Whether any version passed.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.rows
            .iter()
            .any(|row| row.verdict() == Some(VERDICT_PASS))
    }

    /// Whether the chain may show its reference solution now.
    #[must_use]
    pub fn solution_open(&self) -> bool {
        let head = self.head();
        if !loops(&head.context) {
            return head.status == JOB_DONE || head.override_verdict.is_some();
        }
        matches!(self.phase(), Phase::Passed | Phase::Closed)
    }

    /// The reference solution of the problem, when the payload carries one.
    #[must_use]
    pub fn reference(&self) -> Option<&str> {
        self.root().payload_str("reference")
    }
}

/// Group rows into chains, oldest root first.
///
/// A row whose `revision_of` names a row outside `rows` starts a chain of its
/// own, so a partial read never drops a row.
#[must_use]
pub fn chains(rows: Vec<JobRow>) -> Vec<Chain> {
    let ids: BTreeSet<Uuid> = rows.iter().map(|row| row.id).collect();
    let mut successor: BTreeMap<Uuid, Uuid> = BTreeMap::new();
    for row in &rows {
        if let Some(previous) = row.revision_of.filter(|id| ids.contains(id)) {
            successor.entry(previous).or_insert(row.id);
        }
    }
    let mut by_id: BTreeMap<Uuid, JobRow> = rows.iter().map(|row| (row.id, row.clone())).collect();
    let mut out = Vec::new();
    for row in &rows {
        let root = row.revision_of.is_none_or(|id| !ids.contains(&id));
        if !root {
            continue;
        }
        let mut chain = Vec::new();
        let mut at = Some(row.id);
        while let Some(id) = at {
            let Some(found) = by_id.remove(&id) else {
                break;
            };
            chain.push(found);
            at = successor.get(&id).copied();
        }
        if !chain.is_empty() {
            out.push(Chain { rows: chain });
        }
    }
    out
}

/// The client view of one version of a chain.
#[must_use]
pub fn version_view(row: &JobRow) -> Value {
    let grading = row.grading();
    let mut view = json!({
        "id": row.id,
        "attempt_id": row.attempt_id,
        "revision": row.revision,
        "rewrite": row.rewrite,
        "status": status_of(row),
        "answer": row.payload_str("given_answer").unwrap_or_default(),
        "created_at": row.created_at.to_rfc3339(),
        "seen": row.seen_at.is_some(),
        "disputed": row.disputed_at.is_some(),
        "human_verdict": row.override_verdict,
    });
    if let Some(grading) = &grading {
        view["feedback"] = json!(grading.feedback);
        view["checks"] = json!(grading.checks.iter().map(check_view).collect::<Vec<_>>());
        view["first_unmet"] = first_unmet(&grading.checks).map_or(Value::Null, check_view);
    }
    view
}

/// The client view of one chain. `solution` is the reference the caller may
/// show; [`Chain::solution_open`] or the one reveal decides it.
#[must_use]
pub fn chain_view(chain: &Chain, solution: Option<&str>) -> Value {
    let head = chain.head();
    let phase = chain.phase();
    let used = head.revision;
    let mut view = json!({
        "root_id": chain.root().id,
        "head_id": head.id,
        "context": head.context,
        "task_id": chain.root().payload_str("task_id"),
        "topic": chain.root().payload_str("topic"),
        "kp": chain.root().payload_str("kp"),
        "problem": chain.root().payload_str("problem"),
        "phase": phase.as_str(),
        "revision": used,
        "cap": REVISION_CAP,
        "revisions_left": (REVISION_CAP - used).max(0),
        "passed": chain.passed(),
        "draft": head.payload_str("given_answer").unwrap_or_default(),
        "versions": chain.rows.iter().map(version_view).collect::<Vec<_>>(),
    });
    if let Some(grading) = head.grading() {
        view["feedback"] = json!(grading.feedback);
        view["first_unmet"] = first_unmet(&grading.checks).map_or(Value::Null, check_view);
    }
    if let Some(solution) = solution {
        view["solution"] = json!(solution);
    }
    view
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use sqlx::types::chrono::Utc;

    /// A row of `context` with `status`, `verdict` and `revision`.
    pub(crate) fn row(
        id: u128,
        previous: Option<u128>,
        verdict: Option<&str>,
        revision: i32,
    ) -> JobRow {
        JobRow {
            id: Uuid::from_u128(id),
            attempt_id: format!("a-{id}"),
            status: if verdict.is_some() { "done" } else { "pending" }.to_owned(),
            payload: json!({"reference": "The worked proof.", "given_answer": format!("draft {id}"),
                            "topic": "parity", "kp": "kp1", "problem": "Prove it.", "task_id": "t"}),
            result: verdict.map(|verdict| {
                json!({"v": 1, "verdict": verdict, "model": "m", "feedback": "Fix step 2.",
                       "checks": [
                           {"id": "G1", "text": "The claim is stated.", "minor": true, "met": false, "evidence": "not found"},
                           {"id": "G3", "text": "Each step is justified.", "minor": false, "met": false, "evidence": "so it is even"}
                       ]})
            }),
            created_at: Utc::now(),
            revision_of: previous.map(Uuid::from_u128),
            revision,
            context: CONTEXT_LESSON.to_owned(),
            rewrite: false,
            seen_at: None,
            revealed_at: None,
            closed_at: None,
            disputed_at: None,
            dispute_note: None,
            override_verdict: None,
        }
    }

    /// The phase table of the module header.
    #[test]
    fn the_head_decides_the_phase() {
        assert_eq!(phase_of(&row(1, None, None, 0)), Phase::Grading);
        assert_eq!(phase_of(&row(1, None, Some("pass"), 0)), Phase::Passed);
        assert_eq!(
            phase_of(&row(1, None, Some("needs_revision"), 0)),
            Phase::Revise
        );
        assert_eq!(
            phase_of(&row(1, None, Some("needs_revision"), 1)),
            Phase::Revise
        );
        let mut capped = row(1, None, Some("needs_revision"), REVISION_CAP);
        assert_eq!(phase_of(&capped), Phase::Reveal);
        capped.revealed_at = Some(Utc::now());
        assert_eq!(phase_of(&capped), Phase::Rewrite);
        let mut rewrite = row(1, None, None, REVISION_CAP);
        rewrite.rewrite = true;
        assert_eq!(phase_of(&rewrite), Phase::Closed);
        let mut failed = row(1, None, None, 0);
        failed.status = "failed".to_owned();
        assert_eq!(phase_of(&failed), Phase::Unavailable);
        // A human verdict wins over the model's.
        let mut overridden = row(1, None, Some("needs_revision"), 0);
        overridden.override_verdict = Some("pass".to_owned());
        assert_eq!(phase_of(&overridden), Phase::Passed);
    }

    /// Rows group into chains by `revision_of`, root first.
    #[test]
    fn rows_group_into_chains() {
        let rows = vec![
            row(1, None, Some("needs_revision"), 0),
            row(2, None, Some("pass"), 0),
            row(3, Some(1), Some("needs_revision"), 1),
            row(4, Some(3), None, 2),
        ];
        let grouped = chains(rows);
        assert_eq!(grouped.len(), 2);
        let ids: Vec<u128> = grouped[0].rows.iter().map(|r| r.id.as_u128()).collect();
        assert_eq!(ids, vec![1, 3, 4]);
        assert_eq!(grouped[0].phase(), Phase::Grading);
        assert_eq!(grouped[1].phase(), Phase::Passed);
    }

    /// The solution stays closed until a pass or the end of the chain.
    #[test]
    fn the_solution_opens_on_a_pass_or_a_closed_chain() {
        let open = Chain {
            rows: vec![row(1, None, Some("needs_revision"), 0)],
        };
        assert!(!open.solution_open());
        let view = chain_view(&open, None);
        assert!(view.get("solution").is_none());
        assert_eq!(
            view["first_unmet"]["id"],
            json!("G3"),
            "the major check comes first"
        );
        let passed = Chain {
            rows: vec![row(1, None, Some("pass"), 0)],
        };
        assert!(passed.solution_open());
        let mut closed = row(1, None, Some("needs_revision"), 2);
        closed.closed_at = Some(Utc::now());
        assert!(Chain { rows: vec![closed] }.solution_open());
        let mut quiz = row(1, None, Some("needs_revision"), 0);
        quiz.context = "quiz".to_owned();
        assert!(
            Chain { rows: vec![quiz] }.solution_open(),
            "a quiz keeps the old rule"
        );
    }
}
