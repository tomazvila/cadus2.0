//! U4 acceptance, part 2: the `dump_curriculum` binary, the CPython `repr`
//! sweep, and the live 1.0 oracle over the tree and the fixture trees (D1, R5).
//!
//! No expected value is derived by the code under test. The oracle tests run the
//! 1.0 interpreter live and compare the two dumps byte for byte; they need the
//! environment variable `CADUS_ORACLE_PYTHON` and skip without it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;
use std::process::Command;

use cadus_core::curriculum::{canonical_dump, curriculum_hash, python_repr_f64, sha256_hex};
use common::dump::{
    compare_dumps, curriculum_root, fixture_arena, fixture_tree, oracle_dump, oracle_python,
};

/// The semantic curriculum hash of the checked-in tree (spec section 3).
// Regenerated for the expanded live tree; independently derived alongside parity.rs.
const TREE_HASH: &str = "7372b3e95c9862ab5a96befa04f635ac3d4a8a2dfac5b0ea26449ebf29e93ec3";

/// The length of the dump in bytes, without the trailing newline.
const DUMP_LEN: usize = 4_610_360;

/// Compare [`python_repr_f64`] with CPython `repr` over a sweep of 494,972
/// finite floats, and require 0 mismatches.
///
/// The sweep holds two families:
///
/// - 200,000 pseudo-random values in `(0, 1]`, each one a 53-bit mantissa from
///   a SplitMix64 stream scaled by a power of two the same stream picks.
/// - The ladders `m / 2^s` for every `s` in 1..=60, with 5,000 pseudo-random
///   `m` per step. An exact tie needs a value whose exact decimal ends in a 5
///   one digit past the shortest run, which is what these ladders produce, so
///   the family carries 1,846 of them.
///
/// The test writes the bit pattern and the Rust text of every value to a file
/// under `CARGO_TARGET_TMPDIR` and hands the file to the 1.0 interpreter, which
/// reads each float back with `struct.unpack` and compares `repr` with the Rust
/// text. A file keeps the two programs off one pipe, so neither blocks.
///
/// Set `CADUS_ORACLE_PYTHON` to run it. Without the variable the test does
/// nothing, the same rule as the other oracle tests of this file.
#[test]
fn python_repr_f64_matches_cpython_over_a_sweep() {
    let Some(python) = oracle_python() else {
        return;
    };

    let mut lines = String::new();
    let mut count = 0usize;
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let record = |value: f64, lines: &mut String, count: &mut usize| {
        if value > 0.0 && value <= 1.0 {
            *count += 1;
            lines.push_str(&format!(
                "{:016x} {}\n",
                value.to_bits(),
                python_repr_f64(value)
            ));
        }
    };
    for _ in 0..200_000 {
        let raw = next();
        let mantissa = (raw >> 11) as f64;
        let step = (raw % 61) as i32;
        record(mantissa * 2f64.powi(-53 - step), &mut lines, &mut count);
    }
    for step in 1..=60i32 {
        for _ in 0..5_000 {
            let raw = next();
            let bound = 1u64 << step.min(53);
            record(
                (raw % bound) as f64 * 2f64.powi(-step),
                &mut lines,
                &mut count,
            );
        }
    }
    assert_eq!(count, 494_972, "the sweep is deterministic");

    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("python_repr_sweep.txt");
    std::fs::write(&path, &lines).expect("write the sweep file");

    // The 1.0 interpreter reads every float back and compares its own `repr`.
    let program = "\
import struct, sys
bad = 0
total = 0
for line in open(sys.argv[1]):
    bits, text = line.split()
    value = struct.unpack('<d', struct.pack('<Q', int(bits, 16)))[0]
    total += 1
    if repr(value) != text:
        bad += 1
        if bad <= 5:
            print('MISMATCH', bits, repr(value), text)
print('checked', total, 'mismatches', bad)
";
    let output = Command::new(&python)
        .arg("-c")
        .arg(program)
        .arg(&path)
        .output()
        .unwrap_or_else(|e| panic!("run {python}: {e}"));
    let report = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "the sweep program failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        report.trim_end(),
        "checked 494972 mismatches 0",
        "python_repr_f64 must match CPython repr on every value of the sweep"
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
    let root = fixture_tree("arena-float-repr");
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

/// The `arena-negative-zero` fixture puts `0.0` and `-0.0` on one
/// (child, parent) pair, so this comparison fails whenever `compare_float`
/// orders the two signs of zero (finding #2).
///
/// Topic `b` declares `-0.0` with `key: true` first and `0.0` with
/// `key: false` second. Python holds `-0.0 == 0.0`, so the sort of a row falls
/// through to the `key` flag and `false` comes first. Topic `c` declares
/// `0.0` and then `-0.0` with one `key` flag, so no element is left to compare
/// and both stable sorts keep the order of the file.
///
/// The two literals below come from the 1.0 oracle:
///
/// ```text
/// $ /home/deploy/dev/cadus/.venv/bin/python scripts/oracle/dump_curriculum_1_0.py \
///       crates/core/tests/fixtures/arena-negative-zero
/// sha256=1d364876fae80721cbcc790bf3f527e0b145a23d650c5fe640cce65f3000126d
/// ..."prereq_edges":[["b","a",0.0,false],["b","a",-0.0,true],
///                    ["c","a",0.0,true],["c","a",-0.0,true]]...
/// ```
#[test]
fn the_dump_orders_the_two_signs_of_zero_the_way_python_does() {
    let curriculum = fixture_arena("arena-negative-zero");
    let dump = canonical_dump(&curriculum);
    assert!(
        dump.contains(
            "\"prereq_edges\":[[\"b\",\"a\",0.0,false],[\"b\",\"a\",-0.0,true],\
[\"c\",\"a\",0.0,true],[\"c\",\"a\",-0.0,true]]"
        ),
        "the two signs of zero sort equal and the row falls through to the key flag"
    );
    assert_eq!(
        curriculum_hash(&curriculum),
        "1d364876fae80721cbcc790bf3f527e0b145a23d650c5fe640cce65f3000126d"
    );
}

/// The same fixture against the live 1.0 oracle, byte for byte (finding #2).
#[test]
fn the_rust_dump_equals_the_live_1_0_dump_on_the_negative_zero_fixture() {
    let Some(python) = oracle_python() else {
        return;
    };
    let root = fixture_tree("arena-negative-zero");
    let (expected, stderr) = oracle_dump(&python, &root);
    compare_dumps(&root, &expected);
    assert_eq!(
        stderr,
        "sha256=1d364876fae80721cbcc790bf3f527e0b145a23d650c5fe640cce65f3000126d\n"
    );
}

/// The `arena-repeated-edge` fixture against the live 1.0 oracle (finding #9).
///
/// Topic `b` writes the repeated edge b -> a as 0.7 then 0.2, and topic `c`
/// writes c -> a as 0.2 then 0.7. 1.0 keeps the maximum of each pair, so both
/// rows of `encompassing_edges` carry 0.7:
///
/// ```text
/// $ /home/deploy/dev/cadus/.venv/bin/python scripts/oracle/dump_curriculum_1_0.py \
///       crates/core/tests/fixtures/arena-repeated-edge
/// sha256=d3dfd7bdd11ec965a1d6b2ee815d8a750a69f2180236231fd4d2762b1c3756bd
/// ..."encompassing_edges":[["b","a",0.7],["c","a",0.7]]...
/// ```
#[test]
fn the_rust_dump_equals_the_live_1_0_dump_on_the_repeated_edge_fixture() {
    let root = fixture_tree("arena-repeated-edge");
    let curriculum = fixture_arena("arena-repeated-edge");
    let dump = canonical_dump(&curriculum);
    assert!(
        dump.contains("\"encompassing_edges\":[[\"b\",\"a\",0.7],[\"c\",\"a\",0.7]]"),
        "both encompassing rows keep the larger weight of the repeated edge"
    );
    assert_eq!(
        curriculum_hash(&curriculum),
        "d3dfd7bdd11ec965a1d6b2ee815d8a750a69f2180236231fd4d2762b1c3756bd"
    );

    let Some(python) = oracle_python() else {
        return;
    };
    let (expected, _stderr) = oracle_dump(&python, &root);
    compare_dumps(&root, &expected);
}
