//! The placement probe rule (D-DG4).
//!
//! The placement start drops each topic whose `diagnostic_exemplar` gives no
//! verdict under [`Exemplar::verdict_policy`]. This rule fails the tree when
//! that happens, except for a topic on the allow-list. A stale allow-list line
//! fails too, so the list can only get shorter.

use std::collections::BTreeSet;

use super::super::super::finding::Finding;
use super::super::super::model::{AnswerKind, Exemplar};
use super::super::Lint;
use crate::answer::AnswerContract;

/// The fix that every `diagnostic_probe_undecidable` message ends with.
const FIX: &str = "Write a short typed answer under a decidable contract \
(docs/reference/curriculum-1.0-spec.md, section 6).";

/// Read the allow-list text: one topic id per line. A `#` starts a comment.
/// A blank line is ignored. Each id is trimmed.
pub(in crate::curriculum::lint) fn parse_allow(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The cause of a probe that gives no verdict, or `None` when it gives one.
fn cause(kind: AnswerKind, probe: &Exemplar) -> Option<String> {
    let Err(reason) = probe.verdict_policy(kind) else {
        return None;
    };
    Some(match (&probe.answer_contract, kind) {
        (Some(AnswerContract::None), _) => "the probe has contract `none`".to_owned(),
        (Some(AnswerContract::Written), _) => "the probe has contract `written`".to_owned(),
        (None, AnswerKind::Proof) => {
            "the topic kind is `proof` and the probe has no contract".to_owned()
        }
        (None, AnswerKind::MultiStep) => {
            "the topic kind is `multi-step` and the probe has no contract".to_owned()
        }
        _ => format!("the contract refuses the key: {}", reason.reason),
    })
}

/// File `diagnostic_probe_undecidable` and `diagnostic_probe_allow_stale`.
pub(in crate::curriculum::lint) fn check_probes(lint: &mut Lint<'_>, allow: &BTreeSet<String>) {
    for position in 0..lint.table.topics.len() {
        let topic = lint.table.topics[position];
        let Some(probe) = &topic.diagnostic_exemplar else {
            continue;
        };
        let Some(why) = cause(topic.answer_kind, probe) else {
            continue;
        };
        if allow.contains(topic.id.as_str()) {
            continue;
        }
        lint.findings.push(
            Finding::new(
                "diagnostic_probe_undecidable",
                format!("{}: {why}. {FIX}", topic.id),
            )
            .with_topic(topic.id.as_str())
            .with_file(lint.table.file(position)),
        );
    }
    for id in allow {
        let stale = match lint.table.by_id.get(id.as_str()) {
            None => Some("the topic does not exist".to_owned()),
            Some(&position) => {
                let topic = lint.table.topics[position];
                match &topic.diagnostic_exemplar {
                    Some(probe) if cause(topic.answer_kind, probe).is_none() => {
                        Some("the probe gives a verdict now".to_owned())
                    }
                    _ => None,
                }
            }
        };
        if let Some(why) = stale {
            lint.findings.push(
                Finding::new(
                    "diagnostic_probe_allow_stale",
                    format!("{id}: {why}. Remove this line from the allow-list."),
                )
                .with_topic(id.as_str()),
            );
        }
    }
}
