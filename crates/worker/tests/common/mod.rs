//! The fixtures every worker test binary shares: the fake model endpoint, the
//! worker process, the seed rows, and the fault roles.
//!
//! Every test binary includes this module with `mod common;` and reads the part
//! it needs, so a helper that one binary does not call is not dead code here.

#![allow(
    dead_code,
    unused_imports,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

pub mod content;
pub mod fake;
pub mod fault;
pub mod pool;
pub mod process;
pub mod queue;
pub mod teach;

use std::sync::{Arc, Mutex};

pub use content::{
    ContentRow, Seed, assert_one_row, content_rows, content_rows_of_kind, ledger_shape,
    seed_content, set_content_status,
};
pub use fake::{
    FakeModel, assert_batch, author, author_expect, author_within, author_within_expect, batch,
    golden_spec, good_arguments, handle, missing_low_edge, named_reply, proof_spec, reply, slots,
    squares_spec, the_one_decline, tool_reply,
};
pub use fault::{closed_handle, with_grants};
pub use pool::{
    ADDING, Refill, SQUARES, SQUARES_BODY, SQUARES_DIGEST, USER_ID, arena, fixture_curriculum,
    pool_rows, refill_job, refill_pass, seed_approved_template, seed_drained_pair, seed_fixed_user,
    seed_squares_pair, seed_user_with_id, unclaimed,
};
pub use process::{
    KillOnDrop, Run, role_dsn, run_binary, run_binary_with, send_signal, spawn, superuser_dsn,
    wait_output, worker_command,
};
pub use queue::{
    Ledger, diagnose, diagnose_to, diagnosis_reply, enqueue, enqueue_as, ledger, ledger_clock,
    one_attempt, payload, row_of, session_with_spent,
};
pub use teach::{
    NAMES_AN_INSTANCE_ANSWER, OTHER_TEACH_DIGEST, STORED_LADDER_BODY, STORED_LADDER_DIGEST,
    STORED_TEACH_BODY, STORED_TEACH_DIGEST, ladder_arguments, ladder_that_names_an_instance_answer,
    teach_arguments,
};

/// The serving key of the perfect-squares knowledge point, `"<topic_id>/<kp_id>"`.
pub const SQUARES_KEY: &str = "perfect-squares/squares";

/// The digest of the body the loop stores for [`good_arguments`] under
/// [`SQUARES_KEY`] and kind `template`.
///
/// The key of `content_store` is the knowledge point, the kind AND the body, so
/// the material is `SQUARES_KEY`, one NUL byte, `template`, one NUL byte, and
/// the body. Computed outside this tree with
///
/// ```sh
/// printf 'perfect-squares/squares\0template\0%s' '<STORED_BODY>' | sha256sum
/// # fbed1683b615cbc029d86252b22400070c4ac4c30680d8bc71c0cfff23608f17
/// ```
pub const STORED_DIGEST: &str = "sha256:fbed1683b615cbc0";

/// The first line of the retry block (1.0 `prompts.py:767-769`).
pub const RETRY_HEADER: &str = "YOUR PREVIOUS ATTEMPT WAS REFUSED. The server's exact reason was:";

/// The gate's edge-coverage sentence for the low end of `a`
/// (`crates/core/src/template/gate.rs`, row "edge coverage").
pub const LOW_EDGE: &str = "no worked sample uses the low end of a (1) — the edges are where an \
expression stops being right";

/// A prompt digest that is NOT the digest of any current kind.
///
/// It stands for the prompt an operator has since edited (spec section 2.2,
/// "Prompt digest"). It is 16 hex characters, as every prompt digest is.
pub const OLD_PROMPT: &str = "0000000000000000";

/// The environment variable that holds the superuser DSN of the test cluster.
pub const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

/// Turn the log of the worker on for this process, at every level.
///
/// The test harness keeps the output and shows it on a failure only. A log
/// that is on runs every field of every log line, so the tests read the code
/// the operator reads.
pub fn trace() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("cadus_worker=trace"))
        .with_test_writer()
        .try_init();
}

/// A writer that keeps every log byte in memory.
///
/// `tracing_subscriber::fmt` needs a `MakeWriter`. This one hands out a clone of
/// itself, and every clone appends to the same buffer, so a test reads the whole
/// log after the loop stops.
#[derive(Clone, Default)]
pub struct Capture(Arc<Mutex<Vec<u8>>>);

impl Capture {
    /// The captured log as one string.
    pub fn text(&self) -> String {
        let bytes = self.0.lock().expect("the capture lock is not poisoned");
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

impl std::io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("the capture lock is not poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = Capture;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
