//! The prerequisite and diagnostic coverage audit over the REAL curriculum
//! (unit f10).
//!
//! The audit reads `curriculum/` and no database, so this test runs the whole
//! inventory the report carries. `CADUS_PREREQ_BLESS=1` writes the report to
//! `docs/reports/prerequisite-coverage-2026-09-06.md`; every other run only
//! reads.
//!
//! The assertions below are the facts an author must not lose by accident. They
//! name shortages, and they never assert that a shortage is gone.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::readiness::{DiagnosticState, PrereqCoverage, ReadinessIndex};
use cadus_worker::render_prereq_markdown;

/// The date the report carries. It is the audit date of the framework plan.
const REPORT_DATE: &str = "2026-09-06";

/// The repository root, two directories above this crate.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate sits two directories below the repository root")
        .to_path_buf()
}

/// The real curriculum of the repository.
fn real_curriculum() -> Curriculum {
    let (curriculum, _findings) =
        load_curriculum(&repo_root().join("curriculum")).expect("the curriculum loads");
    curriculum
}

#[test]
fn the_audit_reads_every_course_of_the_real_curriculum() {
    let curriculum = real_curriculum();
    let index = ReadinessIndex::build(&curriculum);
    let coverage = PrereqCoverage::build(&curriculum, &index);

    assert_eq!(coverage.topics.len(), curriculum.topic_count());
    let courses = coverage.courses();
    assert!(
        courses.iter().any(|course| course == "foundations"),
        "the report names every course, and Foundations is one of them: {courses:?}"
    );
    for course in &courses {
        let counts = coverage.counts(course);
        assert!(counts.topics > 0, "course {course} carries no topic");
        assert_eq!(
            counts.diagnostic_decidable + counts.diagnostic_undecidable + counts.diagnostic_missing,
            counts.topics,
            "every topic of {course} lands in exactly one diagnostic state"
        );
    }
}

#[test]
fn every_prerequisite_edge_of_the_real_curriculum_points_at_a_topic_the_tree_holds() {
    let curriculum = real_curriculum();
    let index = ReadinessIndex::build(&curriculum);
    let coverage = PrereqCoverage::build(&curriculum, &index);

    let dangling: Vec<(&str, &str)> = coverage
        .topics
        .iter()
        .flat_map(|row| {
            row.dangling
                .iter()
                .map(move |id| (row.topic_id.as_str(), id.as_str()))
        })
        .collect();
    assert!(
        dangling.is_empty(),
        "these prerequisite edges name a topic the tree does not hold: {dangling:?}"
    );
}

#[test]
fn the_audit_names_the_topics_the_placement_cannot_ask_about() {
    let curriculum = real_curriculum();
    let index = ReadinessIndex::build(&curriculum);
    let coverage = PrereqCoverage::build(&curriculum, &index);

    let uncovered: Vec<&str> = coverage
        .topics
        .iter()
        .filter(|row| row.diagnostic != DiagnosticState::Decidable)
        .map(|row| row.topic_id.as_str())
        .collect();
    let counted: usize = coverage
        .courses()
        .iter()
        .map(|course| {
            let counts = coverage.counts(course);
            counts.diagnostic_undecidable + counts.diagnostic_missing
        })
        .sum();
    assert_eq!(
        uncovered.len(),
        counted,
        "the per-course counts and the whole list agree"
    );
}

#[test]
fn every_foundations_diagnostic_is_grammar_decidable() {
    let curriculum = real_curriculum();
    let index = ReadinessIndex::build(&curriculum);
    let coverage = PrereqCoverage::build(&curriculum, &index);
    let counts = coverage.counts("foundations");

    assert_eq!(counts.topics, 285);
    assert_eq!(counts.diagnostic_decidable, 285);
    assert_eq!(counts.diagnostic_undecidable, 0);
    assert_eq!(counts.diagnostic_missing, 0);
}

#[test]
fn every_assumed_mastery_topic_of_the_real_curriculum_carries_its_evidence_row() {
    let curriculum = real_curriculum();
    let index = ReadinessIndex::build(&curriculum);
    let coverage = PrereqCoverage::build(&curriculum, &index);

    for row in coverage.topics.iter().filter(|row| row.assumed_mastery()) {
        let evidence = row.floor_evidence();
        assert_eq!(
            evidence.confirmable,
            row.diagnostic == DiagnosticState::Decidable,
            "topic {} confirms only through a decidable diagnostic item",
            row.topic_id
        );
        assert_eq!(
            evidence.remediable,
            row.practicable(),
            "topic {} remediates only through a practicable knowledge point",
            row.topic_id
        );
    }
}

#[test]
fn the_report_names_every_course_and_the_bless_run_writes_it() {
    let curriculum = real_curriculum();
    let index = ReadinessIndex::build(&curriculum);
    let coverage = PrereqCoverage::build(&curriculum, &index);
    let text = render_prereq_markdown(&coverage, REPORT_DATE);

    assert!(text.starts_with("# Prerequisite and diagnostic coverage — 2026-09-06"));
    assert!(text.contains("## Every course"));
    for course in coverage.courses() {
        assert!(
            text.contains(&format!("## {course}")),
            "the report holds no section for course {course}"
        );
    }
    assert!(text.contains("### Diagnostic coverage"));
    assert!(text.contains("### Prerequisite edges that lead nowhere"));
    assert!(text.contains("### Assumed mastery"));

    if std::env::var("CADUS_PREREQ_BLESS").is_ok_and(|value| value == "1") {
        let path = repo_root()
            .join("docs/reports")
            .join(format!("prerequisite-coverage-{REPORT_DATE}.md"));
        std::fs::write(&path, &text).expect("the report writes");
        println!("wrote {}", path.display());
    }
}
