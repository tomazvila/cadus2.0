//! `dump_curriculum <dir>` — the Rust side of the M1 parity oracle (R5, C5).
//!
//! The program writes the canonical dump of spec section 8 and a newline to
//! standard output, and `sha256=<hex>` to standard error. It exits 0 on success
//! and 2 on any error, so a shell can diff it against the 1.0 oracle:
//!
//! ```text
//! cargo run -p cadus-core --bin dump_curriculum -- curriculum > rust.json
//! /home/deploy/dev/cadus/.venv/bin/python \
//!     scripts/oracle/dump_curriculum_1_0.py curriculum > python.json
//! diff rust.json python.json
//! ```

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use cadus_core::curriculum::{Finding, canonical_dump, curriculum_hash, load_curriculum};

/// The exit code of any error. The parity contract of spec section 8 fixes it
/// at 2, so a shell tells an error apart from a difference in the output.
fn failure() -> ExitCode {
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(root) = args.next() else {
        return fail("usage: dump_curriculum <dir>");
    };
    if args.next().is_some() {
        return fail("usage: dump_curriculum <dir>");
    }

    let (curriculum, findings) = match load_curriculum(Path::new(&root)) {
        Ok(loaded) => loaded,
        Err(error) => return fail(&format!("dump_curriculum: {error}")),
    };

    // 1.0 `Graph.load` raises on a fatal parse-stage finding and tolerates every
    // graph-stage code (parity trap 13). The dump follows it: a tree that 1.0
    // refuses to load has no dump.
    let fatal: Vec<&Finding> = findings.iter().filter(|f| f.fatal).collect();
    if !fatal.is_empty() {
        let mut lines = format!("dump_curriculum: {} fatal finding(s):", fatal.len());
        for finding in fatal {
            lines.push_str(&format!("\n  [{}] {}", finding.code, finding.message));
        }
        return fail(&lines);
    }

    let dump = canonical_dump(&curriculum);
    let hash = curriculum_hash(&curriculum);

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    if let Err(error) = out.write_all(dump.as_bytes()) {
        return fail(&format!("dump_curriculum: write: {error}"));
    }
    if let Err(error) = out.write_all(b"\n") {
        return fail(&format!("dump_curriculum: write: {error}"));
    }
    if let Err(error) = out.flush() {
        return fail(&format!("dump_curriculum: flush: {error}"));
    }

    let stderr = std::io::stderr();
    let mut log = stderr.lock();
    if writeln!(log, "sha256={hash}").is_err() {
        return failure();
    }
    ExitCode::SUCCESS
}

/// Report one message on standard error and give the failure code.
fn fail(message: &str) -> ExitCode {
    let stderr = std::io::stderr();
    let mut log = stderr.lock();
    let _ = writeln!(log, "{message}");
    failure()
}
