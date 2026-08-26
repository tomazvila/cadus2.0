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
    Curriculum, DUMP_SCHEMA, canonical_dump, curriculum_hash, load_curriculum, python_repr_f64,
    sha256_hex,
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

/// One fixture tree under `crates/core/tests/fixtures/`.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
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

// --------------------------------------------------------------------------- //
// The Python float text (parity trap 16, finding #3)
// --------------------------------------------------------------------------- //

/// Every pair below is `(value, repr(value))` from CPython 3.13, the 1.0
/// interpreter. Ten of the thirty values straddle a threshold: `0.0001` and
/// `9.999e-05` sit on the two sides of the small-exponent threshold, and
/// `9999999999999998.0` and `1e+16` sit on the two sides of the large one.
///
/// `serde_json` writes `0.00001`, `1.234e-7`, `1e16` and `9999999999999998`
/// for four of them, so this table fails on the pre-fix formatter.
#[test]
fn python_repr_f64_matches_the_python_repr_table() {
    let table: [(f64, &str); 30] = [
        (0.0, "0.0"),
        (-0.0, "-0.0"),
        (1.0, "1.0"),
        (0.5, "0.5"),
        (0.85, "0.85"),
        (0.1, "0.1"),
        (0.3, "0.3"),
        (1e-4, "0.0001"),
        (9.999e-5, "9.999e-05"),
        (1e-05, "1e-05"),
        (1.5e-05, "1.5e-05"),
        (1.234e-07, "1.234e-07"),
        (6.300_000_000_000_001e-5, "6.300000000000001e-05"),
        (0.000_123_4, "0.0001234"),
        (0.000_123_456_789_012_345_67, "0.00012345678901234567"),
        (1e15, "1000000000000000.0"),
        (9_999_999_999_999_998.0, "9999999999999998.0"),
        (1e16, "1e+16"),
        (1.000_000_000_000_000_2e16, "1.0000000000000002e+16"),
        (123_456_789_012_345_680.0, "1.2345678901234568e+17"),
        (1e17, "1e+17"),
        (2.5e-323, "2.5e-323"),
        (5e-324, "5e-324"),
        (1.797_693_134_862_315_7e308, "1.7976931348623157e+308"),
        (2.225_073_858_507_201_4e-308, "2.2250738585072014e-308"),
        (0.096_768_000_000_000_02, "0.09676800000000002"),
        (0.000_450_000_000_000_000_04, "0.00045000000000000004"),
        (0.000_476_279_999_999_999_93, "0.00047627999999999993"),
        (-0.85, "-0.85"),
        (-1.234e-07, "-1.234e-07"),
    ];
    for (value, expected) in table {
        assert_eq!(python_repr_f64(value), expected, "repr of {value:?}");
    }

    // The three values Python `repr` writes without a JSON form. `float()` maps
    // them to `null` before the dump, so they never reach the text.
    assert_eq!(python_repr_f64(f64::NAN), "nan");
    assert_eq!(python_repr_f64(f64::INFINITY), "inf");
    assert_eq!(python_repr_f64(f64::NEG_INFINITY), "-inf");
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
    let Some(python) = oracle_python() else {
        return;
    };
    let (expected, stderr) = oracle_dump(&python, &curriculum_root());
    compare_dumps(&curriculum_root(), &expected);
    assert_eq!(stderr, format!("sha256={TREE_HASH}\n"));
}

/// The `arena-float-repr` fixture puts a float on both sides of every Python
/// `repr` threshold, so this comparison fails whenever the dump writes a float
/// the way `serde_json` does (finding #3).
///
/// The fixture authors `difficulty` 0.85, 0.00001, 0.0000001234, 0.0001 and
/// 1.0, and the weights 0.000015, 0.0001, 6.300000000000001e-05, 0.00009999 and
/// 0.0. The oracle writes them `0.85`, `1e-05`, `1.234e-07`, `0.0001`, `1.0`,
/// `1.5e-05`, `0.0001`, `6.300000000000001e-05`, `9.999e-05` and `0.0`.
#[test]
fn the_rust_dump_equals_the_live_1_0_dump_on_the_float_fixture() {
    let Some(python) = oracle_python() else {
        return;
    };
    let root = fixture("arena-float-repr");
    let (expected, _stderr) = oracle_dump(&python, &root);
    compare_dumps(&root, &expected);

    // The float texts the fixture exists for, quoted from the oracle output.
    let text = String::from_utf8_lossy(&expected).into_owned();
    for literal in [
        "\"difficulty\":0.85",
        "\"difficulty\":1e-05",
        "\"difficulty\":1.234e-07",
        "\"difficulty\":0.0001",
        "\"difficulty\":1.0",
        "[\"b\",\"a\",1.5e-05]",
        "[\"c\",\"a\",6.300000000000001e-05]",
        "[\"d\",\"c\",9.999e-05]",
        "[\"e\",\"d\",0.0,false]",
    ] {
        assert!(text.contains(literal), "the 1.0 dump must hold {literal}");
    }
}

/// The interpreter of the 1.0 virtual environment, or `None` with a note.
fn oracle_python() -> Option<String> {
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
fn oracle_dump(python: &str, root: &Path) -> (Vec<u8>, String) {
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
fn compare_dumps(root: &Path, expected: &[u8]) {
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
