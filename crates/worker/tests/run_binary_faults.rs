//! The `cadus-worker` process under a fault at one step of an `author` pass or
//! of the tick loop: the curriculum, the database URL, the connect, the schema,
//! the model configuration, and a store read under the batch.
//!
//! Every test starts the real binary and reads its exit code and its output.
//! Every expected value is a literal: a literal exit code, a literal sentence.
//! `run_binary.rs` holds the tests of the loop that runs, and
//! `authoring_cli_process.rs` the tests of the pass that runs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;

use common::{
    Run, SQUARES as KP_KEY, TEST_DSN_VAR, role_dsn, run_binary_with, spawn, superuser_dsn,
    wait_output, with_granted_role, worker_command,
};

/// A closed model endpoint: no pass below makes a call.
const NO_MODEL: &str = "http://127.0.0.1:1/v1";

/// A base URL whose scheme the model client refuses.
const FTP_MODEL: &str = "ftp://127.0.0.1:1/v1";

/// The sentence the model client writes for [`FTP_MODEL`].
const FTP_SENTENCE: &str = "the base URL scheme \"ftp\" is neither http nor https";

/// A DSN that no test reaches: the pass stops before the connect.
const UNREACHED_DSN: &str = "postgresql://x@127.0.0.1:1/x";

/// The DSN of the maintenance database of the test cluster. It holds no schema.
fn schemaless_dsn() -> String {
    std::env::var(TEST_DSN_VAR).unwrap_or_else(|_| panic!("{TEST_DSN_VAR} is not set"))
}

/// A DSN of the test cluster that names a database that does not exist.
fn absent_dsn() -> String {
    superuser_dsn("no_such_database")
}

/// Run `author --kp perfect-squares/kp1 --kind template` plus `rest` against
/// this DSN, this endpoint and these variables, and demand exit code 2.
async fn author_fails(dsn: &str, base_url: &str, rest: &[&str], env: &[(&str, &str)]) -> Run {
    let mut args = vec!["author", "--kp", KP_KEY, "--kind", "template"];
    args.extend_from_slice(rest);
    let run = run_binary_with(dsn, base_url, &args, env).await;
    assert_eq!(run.code, Some(2), "stderr:\n{}", run.stderr);
    run
}

/// Start the tick loop against this DSN with these variables, and demand exit
/// code 2 before the first tick.
async fn serve_fails(dsn: &str, env: &[(&str, &str)]) -> Run {
    let mut command = worker_command(dsn);
    command.env("WORKER_TICK_SECS", "1");
    for (name, value) in env {
        command.env(name, value);
    }
    let run = wait_output(spawn(&mut command), 10).await;
    assert_eq!(run.code, Some(2), "log:\n{}", run.log());
    assert!(
        !run.log().contains("heartbeat tick="),
        "log:\n{}",
        run.log()
    );
    run
}

/// The pass stops at a curriculum that does not load, before any database.
#[tokio::test]
async fn an_author_pass_stops_at_a_curriculum_that_does_not_load() {
    let run = author_fails(
        UNREACHED_DSN,
        NO_MODEL,
        &["--dry-run"],
        &[("CADUS_CURRICULUM", "/no/such/tree")],
    )
    .await;
    assert!(
        run.stderr.contains(
            "the curriculum at /no/such/tree did not load: no courses.yaml under /no/such/tree"
        ),
        "stderr:\n{}",
        run.stderr
    );
}

/// The pass stops at a database URL that is empty, and at a connect that the
/// cluster refuses.
#[tokio::test]
async fn an_author_pass_stops_at_the_database_url_or_at_the_connect() {
    let run = author_fails("", NO_MODEL, &["--dry-run"], &[]).await;
    assert!(
        run.stderr.contains("DATABASE_URL is empty"),
        "stderr:\n{}",
        run.stderr
    );

    let run = author_fails(&absent_dsn(), NO_MODEL, &["--dry-run"], &[]).await;
    assert!(
        run.stderr
            .contains("database \"no_such_database\" does not exist"),
        "stderr:\n{}",
        run.stderr
    );
}

/// The stale listing and the plan both read `content_store`, so a database
/// with no schema stops the pass at that read.
#[tokio::test]
async fn an_author_pass_stops_at_a_read_of_a_table_that_is_absent() {
    for rest in [&["--stale"][..], &["--dry-run"][..]] {
        let run = author_fails(&schemaless_dsn(), NO_MODEL, rest, &[]).await;
        assert!(
            run.stderr
                .contains("relation \"content_store\" does not exist"),
            "{rest:?}; stderr:\n{}",
            run.stderr
        );
        assert!(run.stdout.is_empty(), "{rest:?}; stdout:\n{}", run.stdout);
    }
}

/// A run that is not a dry run needs a model endpoint: an empty key, a token
/// bound that is not a number, and a scheme the client refuses each end the
/// pass after the plan.
#[tokio::test]
async fn an_author_pass_stops_at_a_model_configuration_it_cannot_read() {
    TestDb::with(|db| async move {
        let dsn = superuser_dsn(&db.name);
        let cases: [(&str, &str, &str, &str); 3] = [
            (
                NO_MODEL,
                "OPENAI_API_KEY",
                "",
                "OPENAI_API_KEY is empty — an authoring run needs a model endpoint; use \
                 `--dry-run` to print the plan without one",
            ),
            (
                NO_MODEL,
                "AUTHORING_OUTPUT_TOKENS",
                "many",
                "AUTHORING_OUTPUT_TOKENS must be a whole number, not \"many\"",
            ),
            (FTP_MODEL, "OPENAI_MODEL", "qwen3.6", FTP_SENTENCE),
        ];
        for (base_url, name, value, sentence) in cases {
            let run = author_fails(&dsn, base_url, &[], &[(name, value)]).await;
            assert!(
                run.stderr.contains(sentence),
                "{name}={value}; stderr:\n{}",
                run.stderr
            );
            assert!(
                run.stdout.contains("perfect-squares/kp1 template 0 3 1"),
                "{name}={value}; the plan prints before the stop; stdout:\n{}",
                run.stdout
            );
        }
    })
    .await;
}

/// The batch reads the prompt digest of every stored row, so a role that reads
/// every other column plans the pass and then stops at that read.
#[tokio::test]
async fn an_author_pass_stops_at_a_store_read_the_role_cannot_run() {
    TestDb::with(|db| async move {
        let name = db.name.clone();
        with_granted_role(
            &db,
            &[
                "GRANT USAGE ON SCHEMA public TO {role}",
                "GRANT SELECT (kp_id, kind, status) ON content_store TO {role}",
            ],
            move |_db, role, _pool| async move {
                let run = author_fails(&role_dsn(&name, &role), NO_MODEL, &[], &[]).await;
                assert!(
                    run.stderr
                        .contains("permission denied for table content_store"),
                    "stderr:\n{}",
                    run.stderr
                );
                assert!(
                    run.stdout.contains("perfect-squares/kp1 template 0 3 1"),
                    "the plan prints before the stop; stdout:\n{}",
                    run.stdout
                );
            },
        )
        .await;
    })
    .await;
}

/// The loop stops at a connect the cluster refuses, with exit code 2.
#[tokio::test]
async fn the_loop_stops_at_a_connect_the_cluster_refuses() {
    let run = serve_fails(&absent_dsn(), &[]).await;
    assert!(
        run.stderr
            .contains("database \"no_such_database\" does not exist"),
        "stderr:\n{}",
        run.stderr
    );
}

/// The diagnosis job stops at a base URL whose scheme the client refuses, after
/// the connect and before the first tick.
#[tokio::test]
async fn the_loop_stops_at_a_model_endpoint_the_client_refuses() {
    TestDb::with(|db| async move {
        let run = serve_fails(
            &superuser_dsn(&db.name),
            &[
                ("OPENAI_API_KEY", "test-key"),
                ("OPENAI_BASE_URL", FTP_MODEL),
            ],
        )
        .await;
        assert!(run.stderr.contains(FTP_SENTENCE), "stderr:\n{}", run.stderr);
        assert!(
            run.stderr.contains("cadus-worker: database role"),
            "the role report ran before the stop; stderr:\n{}",
            run.stderr
        );
    })
    .await;
}
