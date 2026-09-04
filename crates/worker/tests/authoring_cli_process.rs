//! M6 R8 acceptance: the `cadus-worker author` process (A2, C6, T3).
//!
//! The section 7 row of the spec names two checks, and both are here:
//!
//! 1. a dry run makes zero model calls and prints the plan
//!    ([`dry_run_prints_the_plan_and_calls_no_model`]);
//! 2. the runbook commands run against the test database
//!    ([`the_runbook_author_command_stores_a_pending_document`] and the process
//!    tests around it).
//!
//! Every expected value is a LITERAL: a literal plan text, a literal refusal
//! sentence, a literal exit code, a literal row count. Nothing is read back from
//! the code under test.
//!
//! The endpoint is a fake OpenAI-compatible server (`common::FakeModel`). No
//! test reaches a real provider, and the dry-run test proves the fake saw NO
//! request at all.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use cadus_worker::authoring::cli::HELP;
use serde_json::json;

use common::{
    FakeModel, SQUARES as KP_KEY, Seed, content_rows, good_arguments, run_binary, run_binary_with,
    seed_content, superuser_dsn, tool_reply,
};

/// The whole plan of `--kp perfect-squares/kp1 --kind template` on an empty
/// table, byte for byte.
const EMPTY_TEMPLATE_PLAN: &str = "\
authoring plan
kp_id kind taken target author
perfect-squares/kp1 template 0 3 1
plan: pairs 1, documents 1, model calls 1 to 5
dry run: no model call and no write
";

/// Run `author --kp perfect-squares/kp1` with these further arguments against
/// this database and endpoint, and demand exit code 0.
async fn author_ok(db: &TestDb, fake: &FakeModel, rest: &[&str]) -> common::Run {
    let mut args = vec!["author", "--kp", KP_KEY];
    args.extend_from_slice(rest);
    let run = run_binary(&superuser_dsn(&db.name), &fake.base_url, &args).await;
    assert_eq!(run.code, Some(0), "stderr:\n{}", run.stderr);
    run
}

/// The `content_store` rows of the fixture knowledge point, as `(kind, status)`.
async fn kinds_and_statuses(db: &TestDb) -> Vec<(String, String)> {
    content_rows(&db.admin, KP_KEY)
        .await
        .into_iter()
        .map(|row| (row.kind, row.status))
        .collect()
}

/// ACCEPTANCE 1: a dry run prints the plan and makes ZERO model calls.
///
/// The endpoint of this test answers one complete template. The pass never asks
/// for it: the fake's counter is 0, the exit code is 0, the plan on stdout is
/// the literal text above, and the table holds no row.
#[tokio::test]
async fn dry_run_prints_the_plan_and_calls_no_model() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;

        let run = author_ok(&db, &fake, &["--kind", "template", "--dry-run"]).await;

        assert_eq!(run.stdout, EMPTY_TEMPLATE_PLAN);
        assert_eq!(fake.call_count(), 0, "a dry run must call no model");
        assert!(content_rows(&db.admin, KP_KEY).await.is_empty());
    })
    .await;
}

/// FIX-M6-A2: `author --stale` lists the approved rows an older prompt wrote,
/// and it makes ZERO model calls.
///
/// Spec section 2.2, "Prompt digest": a prompt edit marks the affected rows for
/// re-authoring and never unapproves one, so the operator reads the mark before
/// a pass spends a token (M6 review finding F4). The seeded row names a prompt
/// digest no kind of this checkout carries.
#[tokio::test]
async fn the_stale_command_lists_the_rows_of_an_older_prompt() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        Seed::new("sha256:old-row", KP_KEY, "teach", "approved")
            .prompt("sha256:0000000000000000")
            .insert(&db.admin)
            .await;

        let run = author_ok(&db, &fake, &["--kind", "teach", "--stale"]).await;

        assert_eq!(
            run.stdout,
            "stale documents\n\
             kp_id kind digest prompt_digest\n\
             perfect-squares/kp1 teach sha256:old-row sha256:0000000000000000\n\
             stale: rows 1\n"
        );
        assert_eq!(fake.call_count(), 0, "a stale listing must call no model");
        assert_eq!(content_rows(&db.admin, KP_KEY).await.len(), 1);
    })
    .await;
}

/// ACCEPTANCE 2: the runbook `author` command stores one pending document.
///
/// It is the second command of the runbook section, run against the test
/// database with the fake endpoint in place of the provider. The pass makes one
/// model call, stores one `pending` row, and prints the plan and the result.
#[tokio::test]
async fn the_runbook_author_command_stores_a_pending_document() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;

        let run = author_ok(&db, &fake, &["--kind", "template"]).await;

        assert!(
            run.stdout.contains("perfect-squares/kp1 template 0 3 1"),
            "stdout:\n{}",
            run.stdout
        );
        assert!(
            run.stdout
                .contains("template: stored 1 skipped 0 declined 0 calls 1 alerts 0"),
            "stdout:\n{}",
            run.stdout
        );
        assert_eq!(fake.call_count(), 1);
        assert_eq!(
            kinds_and_statuses(&db).await,
            vec![("template".to_owned(), "pending".to_owned())]
        );
    })
    .await;
}

/// An authoring call carries the AUTHORING output budget, never the diagnosis
/// one (T3, T5, finding F18).
///
/// The run below sets both diagnosis knobs to their shipped 600 and sets no
/// `AUTHORING_*` knob. The body on the wire must still carry `max_tokens` 4000,
/// and the configuration line must name 4000 and 2000. The old code handed
/// `ModelConfig::from_env()` to the pass, so the body carried 600 with a
/// reasoning ceiling of 600 beside it, which leaves zero visible tokens.
///
/// The second run names `AUTHORING_OUTPUT_TOKENS`, so the operator knob is read
/// under its own name and not by its default alone.
#[tokio::test]
async fn an_authoring_call_takes_the_authoring_output_budget() {
    TestDb::with(|db| async move {
        let dsn = superuser_dsn(&db.name);
        let diagnosis_knobs = [
            ("DIAGNOSIS_OUTPUT_TOKENS", "600"),
            ("DIAGNOSIS_REASONING_MAX_TOKENS", "600"),
        ];
        let mut wider_knobs = diagnosis_knobs.to_vec();
        wider_knobs.push(("AUTHORING_OUTPUT_TOKENS", "1234"));
        wider_knobs.push(("AUTHORING_REASONING_MAX_TOKENS", "567"));
        let runs = [
            (
                &diagnosis_knobs[..],
                4000,
                "output_tokens=4000 reasoning_max_tokens=2000",
            ),
            (
                &wider_knobs[..],
                1234,
                "output_tokens=1234 reasoning_max_tokens=567",
            ),
        ];

        for (knobs, max_tokens, line) in runs {
            let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
            let run = run_binary_with(
                &dsn,
                &fake.base_url,
                &["author", "--kp", KP_KEY, "--kind", "template"],
                knobs,
            )
            .await;

            assert_eq!(run.code, Some(0), "stderr:\n{}", run.stderr);
            let sent = fake.calls();
            assert_eq!(sent.len(), 1);
            assert_eq!(sent[0]["max_tokens"], json!(max_tokens));
            assert!(
                run.stderr.contains(line),
                "the configuration line must name both authoring ceilings; stderr:\n{}",
                run.stderr
            );
        }
    })
    .await;
}

/// A second pass over a full bank calls no model (spec section 2.2, step 1).
///
/// The teach bank holds one document, so the pass skips the pair and the
/// endpoint sees nothing.
#[tokio::test]
async fn a_pass_over_a_full_bank_calls_no_model() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        seed_content(&db.admin, "sha256:aa11", KP_KEY, "teach", "approved").await;

        let run = author_ok(&db, &fake, &["--kind", "teach"]).await;

        assert!(
            run.stdout.contains("perfect-squares/kp1 teach 1 1 0"),
            "stdout:\n{}",
            run.stdout
        );
        assert!(
            run.stdout
                .contains("teach: stored 0 skipped 1 declined 0 calls 0 alerts 0"),
            "stdout:\n{}",
            run.stdout
        );
        assert_eq!(fake.call_count(), 0);
        assert_eq!(content_rows(&db.admin, KP_KEY).await.len(), 1);
    })
    .await;
}

/// A knowledge point the tree does not hold ends the process with code 2.
#[tokio::test]
async fn an_unknown_knowledge_point_exits_two() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(Vec::new()).await;
        let dsn = superuser_dsn(&db.name);

        let run = run_binary(
            &dsn,
            &fake.base_url,
            &["author", "--kp", "perfect-squares/kp9", "--dry-run"],
        )
        .await;

        assert_eq!(run.code, Some(2));
        assert!(
            run.stderr.contains(
                "cadus-worker: configuration error: the curriculum holds no knowledge point \
                 `perfect-squares/kp9`"
            ),
            "stderr:\n{}",
            run.stderr
        );
        assert_eq!(fake.call_count(), 0);
    })
    .await;
}

/// `--help` prints the help text on stdout and exits 0, and it needs no
/// database and no `RUST_LOG`: the log takes its default level.
#[tokio::test]
async fn help_prints_the_help_text_and_exits_zero() {
    let mut command = common::worker_command("postgresql://x@127.0.0.1:1/x");
    command.arg("--help").env_remove("RUST_LOG");

    let run = common::wait_output(common::spawn(&mut command), 30).await;

    assert_eq!(run.code, Some(0), "stderr:\n{}", run.stderr);
    assert_eq!(run.stdout, HELP);
}

/// An argument the parser refuses ends the process with code 2 and the
/// parser's own sentence on stderr.
#[tokio::test]
async fn a_bad_argument_exits_two_with_the_parser_sentence() {
    let run = run_binary(
        "postgresql://x@127.0.0.1:1/x",
        "http://127.0.0.1:1/v1",
        &["serve"],
    )
    .await;

    assert_eq!(run.code, Some(2));
    assert!(
        run.stderr
            .contains("cadus-worker: unknown argument `serve` — run `cadus-worker --help`"),
        "stderr:\n{}",
        run.stderr
    );
}
