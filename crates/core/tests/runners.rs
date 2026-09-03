//! The two binaries of the crate: `lint_curriculum` (C5, spec section 5) and
//! `dump_curriculum` (R5, spec section 8).
//!
//! The lint runner prints the literal text of 1.0 `scripts/lint_curriculum.py`.
//! The dump runner writes the canonical dump and its hash, and exits 2 on any
//! error, so a shell tells an error apart from a difference in the output.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};

use cadus_core::curriculum::{canonical_dump, curriculum_hash};
use common::paths::{arena, curriculum_root, fixture, lint_fixture};

/// The exit code, the standard output and the standard error of one run.
fn run(mut command: Command) -> (i32, String, String) {
    let output = command.output().expect("the runner starts");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Run the `lint_curriculum` binary over one path.
fn run_runner(path: &Path) -> (i32, String, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lint_curriculum"));
    command.arg(path);
    run(command)
}

/// Run the `dump_curriculum` binary with these arguments.
fn run_dump(args: &[&OsStr]) -> (i32, String, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dump_curriculum"));
    command.args(args);
    run(command)
}

// --------------------------------------------------------------------------- //
// The lint runner
// --------------------------------------------------------------------------- //

#[test]
fn the_runner_prints_ok_and_exits_zero_on_a_clean_tree() {
    // The literal text of 1.0 `scripts/lint_curriculum.py` (spec section 5).
    let path = lint_fixture("clean");
    let (code, stdout, stderr) = run_runner(&path);
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        format!(
            "OK: {} is a valid curriculum (0 findings).\n",
            path.display()
        )
    );
    assert_eq!(stderr, "");
}

#[test]
fn the_runner_prints_every_finding_and_exits_one() {
    // Verified against 1.0 `python scripts/lint_curriculum.py <fixture>` on the
    // same tree: the two runs print the same three lines.
    let path = lint_fixture("missing_ref");
    let (code, stdout, stderr) = run_runner(&path);
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert_eq!(
        stderr,
        format!(
            "FAIL: 3 curriculum finding(s) in {}:\n\
             \x20 [missing_ref] (b) prerequisite 'ghost' of 'b' does not exist\n\
             \x20 [missing_ref] (b) encompassings_extra 'phantom' of 'b' does not exist\n\
             \x20 [missing_ref] (b) key_prerequisite 'nowhere' in b.kp1 does not exist\n",
            path.display()
        )
    );
}

#[test]
fn the_runner_omits_the_topic_when_a_finding_names_none() {
    // A `module_inconsistent` "spans multiple courses" finding carries no topic,
    // so 1.0 prints `  [code] message` with no parenthesis.
    let path = lint_fixture("module_inconsistent");
    let (code, _, stderr) = run_runner(&path);
    assert_eq!(code, 1);
    assert_eq!(
        stderr,
        format!(
            "FAIL: 1 curriculum finding(s) in {}:\n\
             \x20 [module_inconsistent] module 'Shared' spans multiple courses: ['c1', 'c2']\n",
            path.display()
        )
    );
}

/// With no argument the runner reads `$CADUS_CURRICULUM`, and with neither it
/// reads `curriculum` under the working directory.
#[test]
fn the_runner_reads_the_environment_and_then_the_default_path() {
    let path = lint_fixture("clean");
    let mut command = Command::new(env!("CARGO_BIN_EXE_lint_curriculum"));
    command.env("CADUS_CURRICULUM", &path);
    let (code, stdout, _) = run(command);
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        format!(
            "OK: {} is a valid curriculum (0 findings).\n",
            path.display()
        )
    );

    let mut command = Command::new(env!("CARGO_BIN_EXE_lint_curriculum"));
    command
        .env_remove("CADUS_CURRICULUM")
        .current_dir(curriculum_root().join(".."));
    let (code, stdout, _) = run(command);
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "OK: curriculum is a valid curriculum (0 findings).\n"
    );
}

// --------------------------------------------------------------------------- //
// The dump runner
// --------------------------------------------------------------------------- //

/// The dump is the canonical line and a newline on standard output, and the
/// hash on standard error.
#[test]
fn the_dump_runner_writes_the_dump_and_the_hash() {
    let path = fixture("cycle-3");
    let (code, stdout, stderr) = run_dump(&[path.as_os_str()]);
    assert_eq!(code, 0);
    let curriculum = arena("cycle-3");
    assert_eq!(stdout, format!("{}\n", canonical_dump(&curriculum)));
    assert_eq!(stderr, format!("sha256={}\n", curriculum_hash(&curriculum)));
}

/// A wrong argument count and a tree that does not load exit 2.
#[test]
fn the_dump_runner_exits_two_on_a_usage_error_and_on_a_load_error() {
    assert_eq!(
        run_dump(&[]),
        (
            2,
            String::new(),
            "usage: dump_curriculum <dir>\n".to_owned()
        )
    );
    let path = fixture("cycle-3");
    assert_eq!(
        run_dump(&[path.as_os_str(), OsStr::new("extra")]),
        (
            2,
            String::new(),
            "usage: dump_curriculum <dir>\n".to_owned()
        )
    );
    let fatal = fixture("arena-parse-fatal");
    let (code, stdout, stderr) = run_dump(&[fatal.as_os_str()]);
    assert_eq!(code, 2);
    assert_eq!(stdout, "");
    assert_eq!(
        stderr,
        "dump_curriculum: [weight_out_of_range] topics.1.prerequisites.0.weight: Input should be \
         less than or equal to 1\n"
    );
}

/// A standard output that takes no byte is a write error, and a standard
/// error that takes no byte still exits 2 after the dump.
#[test]
fn the_dump_runner_exits_two_when_a_stream_refuses_the_write() {
    let path = fixture("cycle-3");
    let full = || std::fs::File::create("/dev/full").expect("/dev/full opens");
    let mut command = Command::new(env!("CARGO_BIN_EXE_dump_curriculum"));
    command.arg(&path).stdout(Stdio::from(full()));
    let (code, _, stderr) = run(command);
    assert_eq!(code, 2);
    assert_eq!(
        stderr,
        "dump_curriculum: write: No space left on device (os error 28)\n"
    );

    let mut command = Command::new(env!("CARGO_BIN_EXE_dump_curriculum"));
    command.arg(&path).stderr(Stdio::from(full()));
    let (code, stdout, _) = run(command);
    assert_eq!(code, 2);
    assert_eq!(stdout, format!("{}\n", canonical_dump(&arena("cycle-3"))));
}
