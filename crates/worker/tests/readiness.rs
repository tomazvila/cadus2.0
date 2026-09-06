//! Unit f7 acceptance: `cadus-worker readiness` — the parser, the audit against
//! a real database, the three contracts, and the two reports.
//!
//! Every expected value is a LITERAL: a literal count, a literal blocker name,
//! a literal line of the report.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::readiness::Blocker;
use cadus_store::test_support::TestDb;
use cadus_worker::authoring::cli::{Command, HELP, ReadinessArgs, parse};
use cadus_worker::{readiness_run, render_readiness_json, render_readiness_markdown};

use common::{ADDING, SQUARES, arena, handle, seed_content};

/// The `readiness` subcommand and its three options read.
#[test]
fn the_readiness_options_read() {
    assert_eq!(
        parse(&["readiness"]),
        Ok(Command::Readiness(ReadinessArgs::default()))
    );
    assert_eq!(
        parse(&[
            "readiness",
            "--course",
            "foundations",
            "--json",
            "out.json",
            "--md",
            "out.md",
        ]),
        Ok(Command::Readiness(ReadinessArgs {
            course: Some("foundations".to_owned()),
            json: Some("out.json".to_owned()),
            md: Some("out.md".to_owned()),
        }))
    );
    assert_eq!(parse(&["readiness", "--help"]), Ok(Command::Help));
    assert_eq!(
        parse(&["readiness", "--course"]).unwrap_err().0,
        "the option `--course` needs a value"
    );
    assert_eq!(
        parse(&["readiness", "--wat"]).unwrap_err().0,
        "unknown option `--wat` — run `cadus-worker --help`"
    );
    assert!(
        HELP.contains("cadus-worker readiness [OPTS]   audit the content and write the report")
    );
}

/// A fresh database approves no document, so every knowledge point is blocked
/// on `teachable` and the report says so (audit findings h and j).
#[tokio::test]
async fn an_empty_content_store_blocks_every_knowledge_point_on_the_teach_page() {
    TestDb::with(|db| async move {
        let curriculum = arena();
        let run = readiness_run(&handle(&db), &curriculum, None)
            .await
            .unwrap();

        let (ready, blocked) = run.report.totals();
        assert_eq!(ready, 0);
        assert!(blocked > 0);
        let histogram = run.report.histogram();
        assert_eq!(histogram.get(&Blocker::Teachable), Some(&blocked));
        assert!(run.approved_documents.is_empty());

        let text = render_readiness_markdown(&run, "2026-09-06");
        assert!(text.starts_with("# Readiness report — every course\n"));
        assert!(text.contains("Generated 2026-09-06 by `cadus-worker readiness`."));
        assert!(text.contains("| Approved documents in `content_store` | 0 |"));
        assert!(text.contains("`content_store` holds no approved document."));
        assert!(text.contains("| `teachable` |"));
    })
    .await;
}

/// The report counts the approved documents by kind, and one approved teach
/// page clears the `teachable` blocker of its knowledge point alone.
#[tokio::test]
async fn one_approved_teach_page_clears_one_teachable_blocker() {
    TestDb::with(|db| async move {
        seed_content(&db.admin, "digest-teach", SQUARES, "teach", "approved").await;
        // A pending document never counts (C6).
        seed_content(&db.admin, "digest-pending", ADDING, "teach", "pending").await;

        let curriculum = arena();
        let run = readiness_run(&handle(&db), &curriculum, None)
            .await
            .unwrap();
        assert_eq!(run.approved_documents.get("teach"), Some(&1));

        let blocked = run.report.totals().1;
        let histogram = run.report.histogram();
        assert_eq!(histogram.get(&Blocker::Teachable), Some(&(blocked - 1)));

        let json = render_readiness_json(&run);
        assert_eq!(json["histogram"]["teachable"], blocked - 1);
        assert_eq!(json["ready"], 0);
        assert_eq!(json["approved_documents"]["teach"], 1);
        assert!(!json["contracts"].as_array().unwrap().is_empty());
    })
    .await;
}

/// The three contracts run over every knowledge point. The fixture tree authors
/// answers the checker decides, so every knowledge point passes, and the one
/// with no exemplar at all reports the serve refusal.
#[tokio::test]
async fn the_serve_render_and_grade_contracts_run_over_every_knowledge_point() {
    TestDb::with(|db| async move {
        let curriculum = arena();
        let run = readiness_run(&handle(&db), &curriculum, None)
            .await
            .unwrap();

        let squares = run.contracts.get(SQUARES).unwrap();
        assert_eq!(squares.instances, 2);
        assert_eq!(squares.refusals, 0);
        assert_eq!(squares.empty_statements, 0);
        assert!(squares.grade_failures.is_empty());
        assert!(squares.ok());

        // `bare/kp1` authors no exemplar, so the serve path builds nothing.
        let bare = run.contracts.get("bare/kp1").unwrap();
        assert_eq!(bare.instances, 0);
        assert_eq!(
            bare.no_problem.as_deref(),
            Some("no exemplar of this knowledge point has an answer the checker can decide")
        );
        assert!(!bare.ok());

        let text = render_readiness_markdown(&run, "2026-09-06");
        assert!(text.contains("| `bare/kp1` | serve: "));
    })
    .await;
}

/// `--course` scopes the report to one course, and a course the tree does not
/// hold reports nothing.
#[tokio::test]
async fn the_course_option_scopes_the_report() {
    TestDb::with(|db| async move {
        let curriculum = arena();
        let scoped = readiness_run(&handle(&db), &curriculum, Some("demo"))
            .await
            .unwrap();
        assert_eq!(scoped.course.as_deref(), Some("demo"));
        assert_eq!(scoped.report.courses.len(), 1);
        assert_eq!(scoped.report.courses[0].course_id, "demo");

        let ghost = readiness_run(&handle(&db), &curriculum, Some("ghost"))
            .await
            .unwrap();
        assert!(ghost.report.courses.is_empty());
        assert_eq!(ghost.report.totals(), (0, 0));
        assert!(ghost.contracts.is_empty());
        let text = render_readiness_markdown(&ghost, "2026-09-06");
        assert!(text.starts_with("# Readiness report — ghost\n"));
    })
    .await;
}
