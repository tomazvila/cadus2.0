//! The `diagnostic-verified-cover` claim on the live tree (D-DG4).
//!
//! Every topic of every course has a placement probe that gives a verdict,
//! except a topic on the allow-list `curriculum/diag-probe-allow.txt`. The path
//! of the allow-list is `$CADUS_DIAG_PROBE_ALLOW` when set.
//!
//! The claim `diagnostic-ungraded-zero-weight` stays in
//! `crates/core/tests/diagnostic_outcome.rs` and
//! `crates/web/tests/diag_route_outcome.rs`. Both pass unchanged.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use cadus_core::curriculum::{
    DIAG_PROBE_ALLOW_ENV, DIAG_PROBE_ALLOW_FILE, lint_curriculum_probes, load_curriculum,
};

fn tree() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../curriculum")
}

fn allow_list() -> BTreeSet<String> {
    let path = std::env::var_os(DIAG_PROBE_ALLOW_ENV)
        .map_or_else(|| tree().join(DIAG_PROBE_ALLOW_FILE), PathBuf::from);
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| line.split('#').next().unwrap_or("").trim().to_owned())
        .filter(|id| !id.is_empty())
        .collect()
}

/// For each course, and for the whole tree: the topics whose probe gives a
/// verdict (the rule that the placement start uses) plus the allow-list equal
/// the topics of the course.
#[test]
fn askable_plus_allow_list_equals_the_topic_count() {
    let (graph, findings) = load_curriculum(&tree()).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let allow = allow_list();
    let mut all_topics = 0usize;
    let mut all_askable = 0usize;
    for course_row in graph.courses() {
        let course = course_row.id.as_str();
        let topics = graph.topics_in_course(course).len();
        let askable = graph
            .topics_in_course(course)
            .iter()
            .filter(|idx| {
                let topic = graph.topic(**idx).unwrap();
                topic
                    .diagnostic_exemplar
                    .as_ref()
                    .is_some_and(|probe| probe.verdict_policy(topic.answer_kind).is_ok())
            })
            .count();
        let allowed = graph
            .topics_in_course(course)
            .iter()
            .filter(|idx| allow.contains(graph.topic(**idx).unwrap().id.as_str()))
            .count();
        assert_eq!(askable + allowed, topics, "{course}");
        all_topics += topics;
        all_askable += askable;
    }
    assert_eq!(all_askable + allow.len(), all_topics);
}

/// The lint rule is clean on the live tree.
#[test]
fn the_probe_lint_is_clean_on_the_live_tree() {
    let findings = lint_curriculum_probes(&tree());
    assert!(findings.is_empty(), "{findings:#?}");
}
