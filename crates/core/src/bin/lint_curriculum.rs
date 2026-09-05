//! Validate the curriculum tree — the CI and content-authoring check (C5).
//!
//! This is the port of 1.0 `scripts/lint_curriculum.py`. It runs every rule of
//! `docs/reference/curriculum-1.0-spec.md` section 5 through
//! [`cadus_core::curriculum::lint_curriculum`], prints each violation, and exits
//! non-zero when the tree holds any finding — advisory findings included.
//!
//! Usage:
//!
//! ```text
//! lint_curriculum [CURRICULUM_DIR]
//! ```
//!
//! `CURRICULUM_DIR` defaults to `$CADUS_CURRICULUM`, and to `curriculum` when
//! that variable is unset.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use cadus_core::curriculum::lint_curriculum;

/// The environment variable that names the curriculum tree.
const CURRICULUM_ENV: &str = "CADUS_CURRICULUM";

/// The tree to lint when neither an argument nor the variable names one.
const DEFAULT_CURRICULUM: &str = "curriculum";

fn main() -> ExitCode {
    let argument = env::args().nth(1);
    let path = match argument {
        Some(text) => PathBuf::from(text),
        None => {
            PathBuf::from(env::var(CURRICULUM_ENV).unwrap_or_else(|_| DEFAULT_CURRICULUM.into()))
        }
    };
    // 1.0 prints the path the way it received it, so the output keeps the
    // argument text and not a canonical path.
    let shown = path.display().to_string();

    let findings = lint_curriculum(&path);
    if findings.is_empty() {
        println!("OK: {shown} is a valid curriculum (0 findings).");
        return ExitCode::SUCCESS;
    }

    eprintln!("FAIL: {} curriculum finding(s) in {shown}:", findings.len());
    // A finding names a topic by its slug, which is never empty.
    for finding in &findings {
        let where_ = finding
            .topic
            .as_deref()
            .map_or_else(String::new, |topic| format!(" ({topic})"));
        eprintln!("  [{}]{where_} {}", finding.code, finding.message);
    }
    ExitCode::FAILURE
}
