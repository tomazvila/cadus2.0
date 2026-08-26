//! U4 acceptance: the canonical dump, the curriculum hash, and 1.0 parity
//! (D1, R5, C5).
//!
//! Every expected value below is a literal from
//! `docs/reference/curriculum-1.0-spec.md` sections 3 and 8, or from a recorded
//! run of `scripts/oracle/dump_curriculum_1_0.py` against `curriculum/`:
//!
//! ```text
//! $ /home/deploy/dev/cadus/.venv/bin/python \
//!       scripts/oracle/dump_curriculum_1_0.py curriculum | wc -c
//! 4052882
//! sha256=f121f9baf73f29e89679a3304c5405352f0ff47f163a0140bd0e92102a0a8b9e
//! ```
//!
//! The 4,052,882 bytes count the trailing newline the oracle writes;
//! [`canonical_dump`] returns the 4,052,881 bytes before it.
//!
//! No expected value is derived by the code under test. The last test runs the
//! 1.0 oracle live and compares the two dumps byte for byte; it needs the
//! environment variable `CADUS_ORACLE_PYTHON` and skips without it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use cadus_core::curriculum::{
    Curriculum, DUMP_SCHEMA, canonical_dump, curriculum_hash, load_curriculum, sha256_hex,
};

/// The semantic curriculum hash of the checked-in tree (spec section 3).
const TREE_HASH: &str = "f121f9baf73f29e89679a3304c5405352f0ff47f163a0140bd0e92102a0a8b9e";

/// The length of the dump in bytes, without the trailing newline.
const DUMP_LEN: usize = 4_052_881;

/// The `counts` object of the dump, as the oracle writes it.
const COUNTS: &str = "\"counts\":{\"anki_seeds\":2144,\"courses\":13,\"encompassing_edges\":3200,\"exemplars\":6800,\"knowledge_points\":3138,\"prereq_edges\":3281,\"topics\":1090,\"units\":88}";

/// The repository root.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The curriculum tree of the repository (C5).
fn curriculum_root() -> PathBuf {
    repo_root().join("curriculum")
}

/// The arena of the checked-in tree.
fn tree() -> Curriculum {
    let (curriculum, findings) = load_curriculum(&curriculum_root()).expect("the tree loads");
    assert!(findings.is_empty(), "the clean tree has 0 parse findings");
    curriculum
}

// --------------------------------------------------------------------------- //
// The hash and the shape of the dump
// --------------------------------------------------------------------------- //

#[test]
fn curriculum_hash_of_the_checked_in_tree_is_the_oracle_hash() {
    assert_eq!(curriculum_hash(&tree()), TREE_HASH);
}

#[test]
fn canonical_dump_is_the_literal_length() {
    assert_eq!(canonical_dump(&tree()).len(), DUMP_LEN);
}

#[test]
fn canonical_dump_holds_the_literal_counts() {
    let dump = canonical_dump(&tree());
    assert!(
        dump.contains(COUNTS),
        "the dump must hold the literal counts of spec section 3"
    );
}

#[test]
fn canonical_dump_starts_with_the_counts_and_names_the_schema() {
    let dump = canonical_dump(&tree());
    // `counts` is the first key, because the oracle sorts every key.
    let head = format!("{{{COUNTS},\"courses\":[{{\"id\":\"foundations\",");
    assert!(
        dump.starts_with(&head),
        "the dump must start with the sorted counts and the catalog in file order"
    );
    assert!(dump.contains("\"schema\":\"cadus-curriculum-dump/1\""));
    assert_eq!(DUMP_SCHEMA, "cadus-curriculum-dump/1");
}

#[test]
fn canonical_dump_holds_the_literal_edge_rows_and_the_topological_ends() {
    let dump = canonical_dump(&tree());
    // The first sorted prerequisite row and the first sorted encompassing row.
    assert!(dump.contains(
        "\"prereq_edges\":[[\"abelian-groups\",\"binary-operations\",0.4,true],\
[\"abelian-groups\",\"function-composition-inverse\",0.3,false],"
    ));
    assert!(dump.contains(
        "\"encompassing_edges\":[[\"abelian-groups\",\"binary-operations\",0.4],\
[\"abelian-groups\",\"function-composition-inverse\",0.3],"
    ));
    // The topological order of spec section 3: the first five and the last three.
    assert!(dump.contains(
        "\"topo_order\":[\"single-digit-addition\",\"subtraction-facts\",\
\"multiplication-tables\",\"division-facts\",\"perfect-squares\","
    ));
    assert!(
        dump.ends_with(
            "\"cartesian-closed-categories\",\"presheaves-intro\",\"toposes-glimpse\"]}"
        )
    );
    // The tree is a DAG, so the cycle field is null.
    assert!(dump.contains("\"cycle\":null,"));
}

#[test]
fn sha256_hex_matches_the_published_vectors() {
    // FIPS 180-2 test vectors. They pin the digest, not the dump.
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

// --------------------------------------------------------------------------- //
// The binary
// --------------------------------------------------------------------------- //

#[test]
fn the_binary_writes_the_dump_and_the_hash() {
    let output = Command::new(env!("CARGO_BIN_EXE_dump_curriculum"))
        .arg(curriculum_root())
        .output()
        .expect("dump_curriculum runs");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout.len(), DUMP_LEN + 1, "the dump plus a newline");
    assert_eq!(output.stdout.last(), Some(&b'\n'));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("sha256={TREE_HASH}\n")
    );
    assert_eq!(sha256_hex(&output.stdout[..DUMP_LEN]), TREE_HASH);
}

#[test]
fn the_binary_exits_2_on_a_tree_with_no_courses_file() {
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/no-courses-file");
    let output = Command::new(env!("CARGO_BIN_EXE_dump_curriculum"))
        .arg(missing)
        .output()
        .expect("dump_curriculum runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no courses.yaml under"));
}

#[test]
fn the_binary_exits_2_without_a_directory() {
    let output = Command::new(env!("CARGO_BIN_EXE_dump_curriculum"))
        .output()
        .expect("dump_curriculum runs");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "usage: dump_curriculum <dir>\n"
    );
}

// --------------------------------------------------------------------------- //
// The live 1.0 oracle
// --------------------------------------------------------------------------- //

/// Run the 1.0 oracle and compare the two dumps byte for byte.
///
/// Set `CADUS_ORACLE_PYTHON` to the interpreter of the 1.0 virtual environment,
/// `/home/deploy/dev/cadus/.venv/bin/python`. Without it the test does nothing,
/// because a machine with no 1.0 checkout still has to pass the gate.
#[test]
fn the_rust_dump_equals_the_live_1_0_dump() {
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        eprintln!("skipped: set CADUS_ORACLE_PYTHON to run the 1.0 oracle");
        return;
    };

    let script = repo_root().join("scripts/oracle/dump_curriculum_1_0.py");
    let output = Command::new(&python)
        .arg(&script)
        .arg(curriculum_root())
        .output()
        .unwrap_or_else(|e| panic!("run {python} {}: {e}", script.display()));
    assert!(
        output.status.success(),
        "the 1.0 oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The oracle writes the dump and one newline; the hash goes to stderr.
    let expected = output.stdout;
    let mut actual = canonical_dump(&tree()).into_bytes();
    actual.push(b'\n');

    if actual != expected {
        let at = first_difference(&actual, &expected);
        panic!(
            "the dumps differ at byte {at}\n  rust:   {}\n  python: {}",
            window(&actual, at),
            window(&expected, at)
        );
    }
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("sha256={TREE_HASH}\n")
    );
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
