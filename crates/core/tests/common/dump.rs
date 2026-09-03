//! Helpers the curriculum-dump parity tests share: the fixture trees, the
//! checked-in tree, and the byte-for-byte comparison against the live 1.0
//! oracle.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;

use cadus_core::curriculum::{Curriculum, canonical_dump, load_curriculum};

use super::events::repo_root;

/// The curriculum tree of the repository (C5).
#[must_use]
pub fn curriculum_root() -> PathBuf {
    repo_root().join("curriculum")
}

/// One fixture tree under `crates/core/tests/fixtures/`.
#[must_use]
pub fn fixture_tree(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The arena of the checked-in tree.
#[must_use]
pub fn tree() -> Curriculum {
    let (curriculum, findings) = load_curriculum(&curriculum_root()).expect("the tree loads");
    assert!(findings.is_empty(), "the clean tree has 0 parse findings");
    curriculum
}

/// The arena of one fixture tree, which has no parse finding.
#[must_use]
pub fn fixture_arena(name: &str) -> Curriculum {
    let (curriculum, findings) = load_curriculum(&fixture_tree(name)).expect("the fixture loads");
    assert!(findings.is_empty(), "the fixture has 0 parse findings");
    curriculum
}

/// The interpreter of the 1.0 virtual environment, or `None` with a note.
pub fn oracle_python() -> Option<String> {
    match std::env::var("CADUS_ORACLE_PYTHON") {
        Ok(python) => Some(python),
        Err(_) => {
            eprintln!("skipped: set CADUS_ORACLE_PYTHON to run the 1.0 oracle");
            None
        }
    }
}

/// Run the 1.0 oracle on one tree. It gives the dump plus a newline, and the
/// `sha256=` line the oracle writes to standard error.
pub fn oracle_dump(python: &str, root: &Path) -> (Vec<u8>, String) {
    let script = repo_root().join("scripts/oracle/dump_curriculum_1_0.py");
    let output = Command::new(python)
        .arg(&script)
        .arg(root)
        .output()
        .unwrap_or_else(|e| panic!("run {python} {}: {e}", script.display()));
    assert!(
        output.status.success(),
        "the 1.0 oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output.stdout, stderr)
}

/// Compare the Rust dump of one tree against the oracle bytes, byte for byte.
pub fn compare_dumps(root: &Path, expected: &[u8]) {
    let (curriculum, _findings) = load_curriculum(root).expect("the tree loads");
    let mut actual = canonical_dump(&curriculum).into_bytes();
    actual.push(b'\n');
    if actual != expected {
        let at = first_difference(&actual, expected);
        panic!(
            "the dumps differ at byte {at}\n  rust:   {}\n  python: {}",
            window(&actual, at),
            window(expected, at)
        );
    }
}

/// The index of the first differing byte of two byte strings.
fn first_difference(a: &[u8], b: &[u8]) -> usize {
    a.iter()
        .zip(b.iter())
        .position(|(x, y)| x != y)
        .unwrap_or_else(|| a.len().min(b.len()))
}

/// A 200-byte window of a byte string, starting at `at`.
fn window(bytes: &[u8], at: usize) -> String {
    let end = (at + 200).min(bytes.len());
    let slice = bytes.get(at..end).unwrap_or(&[]);
    format!("{:?}", String::from_utf8_lossy(slice))
}
