//! M6 R8 acceptance: the authoring CLI, its parser, its selection and its plan
//! text (A2, C6, T3).
//!
//! Every item here is pure but the plan count: the parser, the selection and
//! the rendering take values and give values, so a test asserts literal text
//! with no process. `authoring_cli_process.rs` runs the binary itself.
//!
//! Every expected value is a LITERAL: a literal plan text, a literal refusal
//! sentence, a literal key list. Nothing is read back from the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::curriculum::AnswerKind;
use cadus_store::test_support::TestDb;
use cadus_worker::authoring::cli::{
    self, AuthorArgs, Command, HELP, PlanRow, call_bounds, documents, parse, plan, render_batch,
    render_plan, select,
};
use cadus_worker::authoring::job::{BatchReport, Decline};
use cadus_worker::authoring::prompt::Kind;

use common::{SQUARES as KP_KEY, arena, closed_handle, handle, seed_content};

/// One plan line of this knowledge point, kind, and bank.
fn row(kind: Kind, taken: i64, target: i64) -> PlanRow {
    PlanRow {
        kp_id: KP_KEY.to_owned(),
        kind,
        taken,
        target,
    }
}

// --------------------------------------------------------------------------- //
// The parser
// --------------------------------------------------------------------------- //

/// No argument runs the tick loop, and `--help` prints the help.
#[test]
fn no_argument_serves_and_help_asks_for_the_help() {
    let none: [&str; 0] = [];
    assert_eq!(parse(&none), Ok(Command::Serve));
    assert_eq!(parse(&["--help"]), Ok(Command::Help));
    assert_eq!(parse(&["-h"]), Ok(Command::Help));
    assert_eq!(parse(&["author", "--help"]), Ok(Command::Help));
    assert!(HELP.contains("cadus-worker author [OPTIONS]   run one authoring pass"));
}

/// Every option of `author` reads into the arguments.
#[test]
fn the_author_options_read() {
    let parsed = parse(&[
        "author",
        "--kp",
        "perfect-squares/kp1",
        "--kp",
        "bare/kp1",
        "--kind",
        "teach",
        "--dry-run",
    ])
    .expect("the line parses");

    assert_eq!(
        parsed,
        Command::Author(AuthorArgs {
            kps: vec!["perfect-squares/kp1".to_owned(), "bare/kp1".to_owned()],
            kinds: vec![Kind::Teach],
            dry_run: true,
            stale: false,
        })
    );
}

/// `--stale` reads, and it stands beside `--dry-run` and never inside it.
#[test]
fn the_stale_option_reads() {
    let parsed = parse(&["author", "--kind", "teach", "--stale"]).expect("the line parses");

    assert_eq!(
        parsed,
        Command::Author(AuthorArgs {
            kps: Vec::new(),
            kinds: vec![Kind::Teach],
            dry_run: false,
            stale: true,
        })
    );
    assert!(
        HELP.contains("--stale                 list the approved documents an older prompt wrote,")
    );
}

/// A bad line is refused with the sentence the operator reads on stderr.
#[test]
fn a_bad_line_is_refused_with_its_own_sentence() {
    let refusals = [
        (
            vec!["serve"],
            "unknown argument `serve` — run `cadus-worker --help`",
        ),
        (
            vec!["author", "--all"],
            "unknown option `--all` — run `cadus-worker --help`",
        ),
        (
            vec!["author", "--kind", "worked_example"],
            "unknown kind `worked_example` — the kinds are template, teach, hint_ladder, diagnosis",
        ),
        (
            vec!["author", "--kp", "--dry-run"],
            "the option `--kp` needs a value",
        ),
        (
            vec!["author", "--kind"],
            "the option `--kind` needs a value",
        ),
    ];
    for (line, sentence) in refusals {
        assert_eq!(parse(&line).unwrap_err().to_string(), sentence);
    }
}

/// An empty kind list covers all four kinds, in the order of `KINDS`.
#[test]
fn the_kinds_of_a_pass_keep_the_constant_order() {
    let all = AuthorArgs::default();
    assert_eq!(
        all.kinds(),
        vec![
            Kind::Template,
            Kind::Teach,
            Kind::HintLadder,
            Kind::Diagnosis
        ]
    );

    let two = AuthorArgs {
        kinds: vec![Kind::Diagnosis, Kind::Template],
        ..AuthorArgs::default()
    };
    assert_eq!(two.kinds(), vec![Kind::Template, Kind::Diagnosis]);
}

// --------------------------------------------------------------------------- //
// The selection
// --------------------------------------------------------------------------- //

/// A named key gives the spec of that knowledge point, from the curriculum.
#[test]
fn a_named_key_gives_the_spec_of_that_knowledge_point() {
    let curriculum = arena();
    let specs = select(&curriculum, &[KP_KEY.to_owned()]).expect("the key is in the tree");

    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].topic_id, "perfect-squares");
    assert_eq!(specs[0].kp_id, "kp1");
    assert_eq!(specs[0].kp_name, "Square a whole number");
    assert_eq!(specs[0].topic_name, "Perfect squares");
    assert_eq!(specs[0].answer_kind, AnswerKind::Numeric);
    assert_eq!(specs[0].difficulty_target, None);
    assert_eq!(specs[0].exemplars.len(), 2);
    assert_eq!(specs[0].exemplars[0].answer, "49");
}

/// No key selects every knowledge point of the tree, in curriculum order.
#[test]
fn no_key_selects_every_knowledge_point() {
    let curriculum = arena();
    let specs = select(&curriculum, &[]).expect("the whole tree selects");

    let keys: Vec<String> = specs
        .iter()
        .map(|spec| format!("{}/{}", spec.topic_id, spec.kp_id))
        .collect();
    assert_eq!(
        keys,
        vec![
            "perfect-squares/kp1",
            "adding-two-digits/kp1",
            "bare/kp1",
            "big-subtraction/kp1",
        ]
    );
}

/// A key the tree does not hold is refused, and the message names the key.
#[test]
fn an_unknown_key_is_refused_by_name() {
    let curriculum = arena();
    assert_eq!(
        select(&curriculum, &["perfect-squares/kp9".to_owned()])
            .unwrap_err()
            .to_string(),
        "the curriculum holds no knowledge point `perfect-squares/kp9`"
    );
    assert_eq!(
        select(&curriculum, &["kp1".to_owned()])
            .unwrap_err()
            .to_string(),
        "the knowledge point `kp1` is not a serving key — write it as `<topic_id>/<kp_id>`"
    );
}

// --------------------------------------------------------------------------- //
// The plan text
// --------------------------------------------------------------------------- //

/// The plan renders one line per pair and one total line.
///
/// A pair whose bank is full plans 0 documents, and a short bank plans 1: one
/// pass authors at most one document per pair.
#[test]
fn the_plan_renders_one_line_per_pair() {
    let rows = vec![row(Kind::Template, 1, 3), row(Kind::Teach, 1, 1)];

    assert_eq!(documents(&rows), 1);
    assert_eq!(call_bounds(&rows), (1, 5));
    assert_eq!(
        render_plan(&rows, false),
        "\
authoring plan
kp_id kind taken target author
perfect-squares/kp1 template 1 3 1
perfect-squares/kp1 teach 1 1 0
plan: pairs 2, documents 1, model calls 1 to 5
"
    );
}

/// A full bank plans nothing, so the pass spends nothing.
#[test]
fn a_full_bank_plans_no_document() {
    let rows = vec![row(Kind::Template, 3, 3)];

    assert_eq!(documents(&rows), 0);
    assert_eq!(call_bounds(&rows), (0, 0));
    assert!(render_plan(&rows, true).ends_with("dry run: no model call and no write\n"));
}

/// The result of one pass renders one count line per kind, then one line per
/// declined knowledge point with the last reason the gate wrote.
#[test]
fn the_batch_renders_its_counts_and_every_decline() {
    let report = BatchReport {
        stored: 2,
        duplicate: 1,
        skipped: 3,
        declined: 2,
        calls: 9,
        alerts: 1,
        declines: vec![
            Decline {
                kp_id: KP_KEY.to_owned(),
                kind: Kind::Template,
                attempts: 5,
                reasons: vec!["first".to_owned(), "edge-coverage: the last one".to_owned()],
            },
            Decline {
                kp_id: "bare/kp1".to_owned(),
                kind: Kind::Template,
                attempts: 0,
                reasons: Vec::new(),
            },
        ],
    };

    assert_eq!(
        render_batch(Kind::Template, &report),
        "\
template: stored 2 skipped 3 declined 2 calls 9 alerts 1
declined perfect-squares/kp1 template after 5 attempts: edge-coverage: the last one
declined bare/kp1 template after 0 attempts: (no reason)
"
    );
}

/// The plan counts the bank slots of every pair through the loop's own read, so
/// the plan states the loop's decision and not a second rule.
#[tokio::test]
async fn the_plan_counts_the_slots_of_every_pair() {
    TestDb::with(|db| async move {
        seed_content(&db.admin, "sha256:one", KP_KEY, "teach", "approved").await;
        let specs = select(&arena(), &[KP_KEY.to_owned()]).unwrap();

        let rows = plan(&handle(&db), &specs, &[Kind::Template, Kind::Teach])
            .await
            .unwrap();

        assert_eq!(
            rows,
            vec![row(Kind::Template, 0, 3), row(Kind::Teach, 1, 1)]
        );

        let closed = closed_handle(&db).await;
        let err = plan(&closed, &specs, &[Kind::Template])
            .await
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("store error: "), "{err}");
    })
    .await;
}

/// The module of the CLI is reachable under its documented path.
#[test]
fn the_subcommand_name_is_author() {
    assert_eq!(cli::AUTHOR, "author");
}
