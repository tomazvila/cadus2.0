//! The exemplar content rules of the full lint (F-grind-lint).

use super::super::super::finding::Finding;
use super::super::super::model::{AnswerKind, Exemplar};
use super::super::Lint;
use crate::readiness::HELD_OUT_MINIMUM;

/// The task verbs that mark a problem statement as asking something
/// (F-grind-lint). A statement with none of these and no question mark
/// never says what to compute — the trapezoid-formula defect of the grind.
const ASK_VERBS: &[&str] = &[
    "find",
    "compute",
    "evaluate",
    "simplify",
    "give",
    "calculate",
    "solve",
    "list",
    "order",
    "classify",
    "convert",
    "express",
    "determine",
    "name",
    "translate",
    "plot",
    "round",
    "estimate",
    "expand",
    "factor",
    "reduce",
    "write",
    "state",
    "check",
    "identify",
    "match",
    "choose",
    "reflect",
    "draw",
    "label",
    "measure",
    "construct",
    "describe",
    "explain",
    "compare",
];

/// F-grind-lint: curriculum exemplars are learner-facing content that no
/// `content_store` gate reviews. Templates, teach pages and hint ladders pass
/// the authoring gates; exemplars go straight from the YAML to the practice
/// pool, the held-out assessment and the diagnostic — and every content defect
/// the grind surfaced in them lived in exactly that gap: statements that never
/// state an ask, answers undecidable under the topic's own `answer_kind`
/// (ISSUE-11), missing sketches, and KP exemplar counts below the held-out
/// minimum. One advisory finding per defect, per exemplar.
pub(in crate::curriculum::lint) fn check_exemplars(lint: &mut Lint<'_>) {
    for position in 0..lint.table.topics.len() {
        let topic = lint.table.topics[position];
        let file = lint.table.file(position);
        for kp in &topic.knowledge_points {
            let mut decidable = 0usize;
            for exemplar in &kp.exemplars {
                check_one_exemplar(
                    &mut lint.findings,
                    topic.id.as_str(),
                    file,
                    &topic.answer_kind,
                    kp.id.as_str(),
                    exemplar,
                    &mut decidable,
                );
            }
            if decidable < HELD_OUT_MINIMUM {
                lint.findings.push(
                    Finding::advisory(
                        "exemplar_count",
                        format!(
                            "{}/{}: {} decidable exemplar(s), but the held-out assessment and the practice floor need {}",
                            topic.id, kp.id, decidable, HELD_OUT_MINIMUM
                        ),
                    )
                    .with_topic(topic.id.as_str())
                    .with_file(file),
                );
            }
        }
        if let Some(dx) = &topic.diagnostic_exemplar {
            check_one_exemplar(
                &mut lint.findings,
                topic.id.as_str(),
                file,
                &topic.answer_kind,
                "diagnostic",
                dx,
                &mut 0,
            );
        }
    }
}

/// The four exemplar rules for one item. `decidable` accumulates the KP count.
///
/// [`Exemplar::verdict_policy`] decides if the item gives a verdict. An item
/// that gives no verdict gets one finding, because the sketch rule and the ask
/// rule apply only to an item that the checker grades.
fn check_one_exemplar(
    findings: &mut Vec<Finding>,
    topic_id: &str,
    file: &str,
    answer_kind: &AnswerKind,
    kp_id: &str,
    exemplar: &Exemplar,
    decidable: &mut usize,
) {
    let where_at = format!("{}/{}", topic_id, kp_id);
    if let Err(reason) = exemplar.verdict_policy(*answer_kind) {
        findings.push(
            no_verdict_finding(&where_at, answer_kind, exemplar, reason.reason)
                .with_topic(topic_id)
                .with_file(file),
        );
        return;
    }
    *decidable += 1;
    if exemplar
        .solution_sketch
        .as_deref()
        .is_none_or(|sketch| sketch.trim().is_empty())
    {
        findings.push(
            Finding::advisory(
                "exemplar_sketch",
                format!(
                    "{}: the decidable exemplar {:?} carries no solution_sketch",
                    where_at, exemplar.problem
                ),
            )
            .with_topic(topic_id)
            .with_file(file),
        );
    }
    if !states_ask(&exemplar.problem) {
        findings.push(
            Finding::advisory(
                "exemplar_no_ask",
                format!(
                    "{}: the problem states no ask (no question mark, no task verb): {:?}",
                    where_at, exemplar.problem
                ),
            )
            .with_topic(topic_id)
            .with_file(file),
        );
    }
}

/// Whether a problem statement states an ask: a question mark, or a task verb
/// anywhere in the text (the formula-substitution family asks through "to
/// find", so a bare word scan is the right shape — "Use $A=lw$ to find the
/// area…" passes, "Use $A=(a+b)h/2$ for a trapezoid with…" does not).
fn states_ask(problem: &str) -> bool {
    if problem.contains('?') {
        return true;
    }
    let lower = problem.to_lowercase();
    lower
        .split(|c: char| !c.is_ascii_alphabetic())
        .any(|token| ASK_VERBS.contains(&token))
}

/// The finding for an exemplar that gives no verdict.
///
/// If the item has no contract on a `multi-step` or `proof` topic, the cause
/// is the topic kind (ISSUE-11). If not, the cause is the authored answer.
fn no_verdict_finding(
    where_at: &str,
    answer_kind: &AnswerKind,
    exemplar: &Exemplar,
    reason: &str,
) -> Finding {
    if exemplar.answer_contract.is_none()
        && matches!(answer_kind, AnswerKind::MultiStep | AnswerKind::Proof)
    {
        return Finding::advisory(
            "answer_kind_mismatch",
            format!(
                "{}: the topic declares answer_kind {}, which cannot grade a single answer — the exemplar {:?} would grade ungraded every time (ISSUE-11)",
                where_at,
                answer_kind.as_str(),
                exemplar.problem
            ),
        );
    }
    Finding::advisory(
        "exemplar_answer",
        format!(
            "{}: the exemplar answer {:?} is undecidable: {}",
            where_at, exemplar.answer, reason
        ),
    )
}
