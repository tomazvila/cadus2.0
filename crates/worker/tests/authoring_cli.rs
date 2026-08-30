//! M6 R8 acceptance: the authoring CLI entry point (A2, C6, T3).
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
//! The endpoint is a fake OpenAI-compatible server in this file. No test reaches
//! a real provider, and the dry-run test proves the fake saw NO request at all.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cadus_core::curriculum::{AnswerKind, Curriculum, load_curriculum};
use cadus_store::test_support::TestDb;
use cadus_worker::authoring::cli::{
    self, AuthorArgs, Command, HELP, PlanRow, call_bounds, documents, parse, render_plan, select,
};
use cadus_worker::authoring::prompt::Kind;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// --------------------------------------------------------------------------- //
// The literals of this file
// --------------------------------------------------------------------------- //

/// The serving key of the fixture knowledge point the tests author for.
const KP_KEY: &str = "perfect-squares/kp1";

/// The whole plan of `--kp perfect-squares/kp1 --kind template` on an empty
/// table, byte for byte.
const EMPTY_TEMPLATE_PLAN: &str = "\
authoring plan
kp_id kind taken target author
perfect-squares/kp1 template 0 3 1
plan: pairs 1, documents 1, model calls 1 to 5
dry run: no model call and no write
";

/// The environment variable that holds the superuser DSN of the test cluster.
const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

// --------------------------------------------------------------------------- //
// The fake OpenAI-compatible server
// --------------------------------------------------------------------------- //

/// A local endpoint that answers a fixed list of replies, in order. A call past
/// the end of the list gets `500` with an empty body.
struct FakeModel {
    base_url: String,
    calls: Arc<Mutex<Vec<Value>>>,
}

impl FakeModel {
    async fn start(replies: Vec<(u16, String)>) -> FakeModel {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&calls);

        tokio::spawn(async move {
            let mut index = 0_usize;
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut raw: Vec<u8> = Vec::new();
                let mut buffer = [0_u8; 4096];
                let request = loop {
                    let read = socket.read(&mut buffer).await.unwrap_or(0);
                    if read == 0 {
                        break String::from_utf8_lossy(&raw).to_string();
                    }
                    raw.extend_from_slice(&buffer[..read]);
                    let text = String::from_utf8_lossy(&raw).to_string();
                    if let Some(split) = text.find("\r\n\r\n") {
                        let length: usize = text[..split]
                            .to_lowercase()
                            .split("\r\n")
                            .find_map(|line| line.strip_prefix("content-length:"))
                            .and_then(|value| value.trim().parse().ok())
                            .unwrap_or(0);
                        if text.len() >= split + 4 + length {
                            break text;
                        }
                    }
                };
                let split = request.find("\r\n\r\n").unwrap_or(request.len());
                let body: Value = serde_json::from_str(request.get(split + 4..).unwrap_or(""))
                    .unwrap_or(Value::Null);
                record.lock().unwrap().push(body);

                let (status, payload) = replies
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| (500, String::new()));
                index += 1;
                let reply = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
                let _ = socket.flush().await;
            }
        });

        FakeModel {
            base_url: format!("http://127.0.0.1:{port}/v1"),
            calls,
        }
    }

    /// The count of requests the endpoint received.
    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    /// The request bodies the endpoint received, in order.
    fn calls(&self) -> Vec<Value> {
        self.calls.lock().unwrap().clone()
    }
}

/// A reply that carries a complete `emit_template` call with these arguments.
fn tool_reply(arguments: &Value) -> (u16, String) {
    let payload = json!({
        "id": "gen-1",
        "choices": [{"finish_reason": "tool_calls", "message": {"tool_calls": [{"function": {
            "name": "emit_template", "arguments": arguments.to_string()
        }}]}}],
        "usage": {"prompt_tokens": 900, "completion_tokens": 300}
    });
    (200, payload.to_string())
}

/// The tool arguments of a template the gate accepts, for the fixture knowledge
/// point `perfect-squares/kp1`.
fn good_arguments() -> Value {
    json!({
        "statement": "Compute ${a}^{{2}}$.",
        "params": {"a": {"kind": "int", "low": 1, "high": 12}},
        "constraints": [],
        "answer_expr": "a**2",
        "solution_sketch": "${a} \\times {a}$ gives the answer.",
        "hints": ["What does squaring a number mean?"],
        "distractors": [],
        "samples": [
            {"params": {"a": 1}, "expected": "1"},
            {"params": {"a": 12}, "expected": "144"}
        ]
    })
}

// --------------------------------------------------------------------------- //
// The fixtures
// --------------------------------------------------------------------------- //

/// The curriculum tree the process tests point `CADUS_CURRICULUM` at.
fn fixture_curriculum() -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pool")
        .to_string_lossy()
        .into_owned()
}

/// That tree, loaded.
fn arena() -> Curriculum {
    load_curriculum(std::path::Path::new(&fixture_curriculum()))
        .expect("the fixture curriculum loads")
        .0
}

/// Build the superuser DSN of one throwaway database.
fn superuser_dsn(db_name: &str) -> String {
    let base = std::env::var(TEST_DSN_VAR).unwrap_or_else(|_| panic!("{TEST_DSN_VAR} is not set"));
    let (prefix, tail) = base
        .rsplit_once('/')
        .unwrap_or_else(|| panic!("{TEST_DSN_VAR} has no database path: {base}"));
    match tail.split_once('?') {
        Some((_, query)) => format!("{prefix}/{db_name}?{query}"),
        None => format!("{prefix}/{db_name}"),
    }
}

/// A child process that never outlives the test that made it.
struct KillOnDrop(Option<tokio::process::Child>);

impl KillOnDrop {
    fn new(child: tokio::process::Child) -> Self {
        Self(Some(child))
    }

    fn into_inner(mut self) -> tokio::process::Child {
        self.0.take().expect("the guard still holds the child")
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let Some(mut child) = self.0.take() else {
            return;
        };
        let _ = child.start_kill();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }
}

/// What one run of the binary produced.
struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Run `cadus-worker` with these arguments, this database and this endpoint.
///
/// `OPENAI_API_KEY` and `OPENAI_BASE_URL` always name the fake endpoint, so a
/// pass that calls a model reaches the fake and a pass that must call none is
/// caught by the fake's own counter.
async fn run_binary(dsn: &str, base_url: &str, args: &[&str]) -> Run {
    run_binary_with(dsn, base_url, args, &[]).await
}

/// [`run_binary`], plus these environment variables.
///
/// The pairs go in last, so a test names the exact value of a knob the run
/// reads. A knob the pairs do not name keeps the value of the shell that started
/// the test.
async fn run_binary_with(dsn: &str, base_url: &str, args: &[&str], env: &[(&str, &str)]) -> Run {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_cadus-worker"));
    command
        .args(args)
        .env("DATABASE_URL", dsn)
        .env("CADUS_CURRICULUM", fixture_curriculum())
        .env("OPENAI_API_KEY", "test-key")
        .env("OPENAI_BASE_URL", base_url)
        .env("OPENAI_MODEL", "qwen3.6")
        .env("RUST_LOG", "info")
        // `tracing_subscriber` colors its fields on a pipe too, so a log line
        // reaches this test with escape bytes inside `output_tokens=4000`.
        // `NO_COLOR` turns the color off and leaves the text readable.
        .env("NO_COLOR", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (name, value) in env {
        command.env(name, value);
    }
    let child = KillOnDrop::new(command.spawn().expect("the worker binary must start"));

    let output = tokio::time::timeout(
        Duration::from_secs(30),
        child.into_inner().wait_with_output(),
    )
    .await
    .expect("the authoring pass must end within 30 s")
    .expect("reading the worker output must succeed");

    Run {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// The `content_store` rows of one serving key, as `(kind, status)`.
async fn rows_of(pool: &PgPool, kp_id: &str) -> Vec<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT kind, status FROM content_store WHERE kp_id = $1 ORDER BY kind",
    )
    .bind(kp_id)
    .fetch_all(pool)
    .await
    .unwrap()
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
        })
    );
}

/// A bad line is refused with the sentence the operator reads on stderr.
#[test]
fn a_bad_line_is_refused_with_its_own_sentence() {
    assert_eq!(
        parse(&["serve"]).unwrap_err().to_string(),
        "unknown argument `serve` — run `cadus-worker --help`"
    );
    assert_eq!(
        parse(&["author", "--all"]).unwrap_err().to_string(),
        "unknown option `--all` — run `cadus-worker --help`"
    );
    assert_eq!(
        parse(&["author", "--kind", "worked_example"])
            .unwrap_err()
            .to_string(),
        "unknown kind `worked_example` — the kinds are template, teach, hint_ladder, diagnosis"
    );
    assert_eq!(
        parse(&["author", "--kp", "--dry-run"])
            .unwrap_err()
            .to_string(),
        "the option `--kp` needs a value"
    );
    assert_eq!(
        parse(&["author", "--kind"]).unwrap_err().to_string(),
        "the option `--kind` needs a value"
    );
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
    let rows = vec![
        PlanRow {
            kp_id: KP_KEY.to_owned(),
            kind: Kind::Template,
            taken: 1,
            target: 3,
        },
        PlanRow {
            kp_id: KP_KEY.to_owned(),
            kind: Kind::Teach,
            taken: 1,
            target: 1,
        },
    ];

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
    let rows = vec![PlanRow {
        kp_id: KP_KEY.to_owned(),
        kind: Kind::Template,
        taken: 3,
        target: 3,
    }];

    assert_eq!(documents(&rows), 0);
    assert_eq!(call_bounds(&rows), (0, 0));
    assert!(render_plan(&rows, true).ends_with("dry run: no model call and no write\n"));
}

// --------------------------------------------------------------------------- //
// The acceptance checks (the process, against the test database)
// --------------------------------------------------------------------------- //

/// ACCEPTANCE 1: a dry run prints the plan and makes ZERO model calls.
///
/// The endpoint of this test answers one complete template. The pass never asks
/// for it: the fake's counter is 0, the exit code is 0, the plan on stdout is
/// the literal text above, and the table holds no row.
#[tokio::test]
async fn dry_run_prints_the_plan_and_calls_no_model() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        let dsn = superuser_dsn(&db.name);

        let run = run_binary(
            &dsn,
            &fake.base_url,
            &["author", "--kp", KP_KEY, "--kind", "template", "--dry-run"],
        )
        .await;

        assert_eq!(run.code, Some(0), "stderr:\n{}", run.stderr);
        assert_eq!(run.stdout, EMPTY_TEMPLATE_PLAN);
        assert_eq!(fake.call_count(), 0, "a dry run must call no model");
        assert!(rows_of(&db.admin, KP_KEY).await.is_empty());
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
        let dsn = superuser_dsn(&db.name);

        let run = run_binary(
            &dsn,
            &fake.base_url,
            &["author", "--kp", KP_KEY, "--kind", "template"],
        )
        .await;

        assert_eq!(run.code, Some(0), "stderr:\n{}", run.stderr);
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
            rows_of(&db.admin, KP_KEY).await,
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

        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        let run = run_binary_with(
            &dsn,
            &fake.base_url,
            &["author", "--kp", KP_KEY, "--kind", "template"],
            &diagnosis_knobs,
        )
        .await;

        assert_eq!(run.code, Some(0), "stderr:\n{}", run.stderr);
        let sent = fake.calls();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["max_tokens"], json!(4000));
        assert!(
            run.stderr
                .contains("output_tokens=4000 reasoning_max_tokens=2000"),
            "the configuration line must name both authoring ceilings; stderr:\n{}",
            run.stderr
        );

        let wider = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        let mut knobs = diagnosis_knobs.to_vec();
        knobs.push(("AUTHORING_OUTPUT_TOKENS", "1234"));
        knobs.push(("AUTHORING_REASONING_MAX_TOKENS", "567"));
        let run = run_binary_with(
            &dsn,
            &wider.base_url,
            &["author", "--kp", KP_KEY, "--kind", "template"],
            &knobs,
        )
        .await;

        assert_eq!(run.code, Some(0), "stderr:\n{}", run.stderr);
        let sent = wider.calls();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["max_tokens"], json!(1234));
        assert!(
            run.stderr
                .contains("output_tokens=1234 reasoning_max_tokens=567"),
            "the operator knobs must reach the configuration line; stderr:\n{}",
            run.stderr
        );
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
        let dsn = superuser_dsn(&db.name);
        sqlx::query(
            "INSERT INTO content_store (digest, kp_id, kind, body, status)
             VALUES ('sha256:aa11', $1, 'teach', '{}'::jsonb, 'approved')",
        )
        .bind(KP_KEY)
        .execute(&db.admin)
        .await
        .unwrap();

        let run = run_binary(
            &dsn,
            &fake.base_url,
            &["author", "--kp", KP_KEY, "--kind", "teach"],
        )
        .await;

        assert_eq!(run.code, Some(0), "stderr:\n{}", run.stderr);
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
        assert_eq!(rows_of(&db.admin, KP_KEY).await.len(), 1);
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

/// The module of the CLI is reachable under its documented path.
#[test]
fn the_subcommand_name_is_author() {
    assert_eq!(cli::AUTHOR, "author");
}
