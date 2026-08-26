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

// --------------------------------------------------------------------------- //
// The CPython tie rule (finding #1)
// --------------------------------------------------------------------------- //

/// Forty values whose shortest 16-digit run is an exact tie, with the text
/// CPython 3.13 `repr` writes for each.
///
/// Rust `{:e}` breaks such a tie away from zero and writes the odd last digit;
/// CPython breaks it to the even last digit. Every literal below is therefore
/// the digit run Rust does NOT produce, so the table fails on a formatter that
/// takes the Rust digits unchanged.
///
/// The values come from a sweep of 12,276 tie values across 14 decimal
/// exponents, each one read back through the 1.0 interpreter:
///
/// ```text
/// $ /home/deploy/dev/cadus/.venv/bin/python
/// >>> repr(0.7210159301757812)
/// '0.7210159301757812'
/// >>> repr(float('0.7210159301757813'))     # the Rust text, same float
/// '0.7210159301757812'
/// ```
// The two literals at the end of this test are the EXACT decimal values of the
// review record, not a shorter text with the same float. `excessive_precision`
// asks for the shortest text, which is the point at issue, so the lint is off
// for this test only.
#[allow(clippy::excessive_precision)]
#[test]
fn python_repr_f64_matches_the_python_repr_tie_table() {
    let table: [(f64, &str); 40] = [
        (2.9802322387695312e-08, "2.9802322387695312e-08"),
        (5.960464477539062e-07, "5.960464477539062e-07"),
        (1.0728836059570312e-06, "1.0728836059570312e-06"),
        (1.5497207641601562e-06, "1.5497207641601562e-06"),
        (2.0265579223632812e-06, "2.0265579223632812e-06"),
        (2.5033950805664062e-06, "2.5033950805664062e-06"),
        (1.0728836059570312e-05, "1.0728836059570312e-05"),
        (1.1682510375976562e-05, "1.1682510375976562e-05"),
        (1.2636184692382812e-05, "1.2636184692382812e-05"),
        (1.3589859008789062e-05, "1.3589859008789062e-05"),
        (0.00010156631469726562, "0.00010156631469726562"),
        (0.00010347366333007812, "0.00010347366333007812"),
        (0.00010538101196289062, "0.00010538101196289062"),
        (0.0009660720825195312, "0.0009660720825195312"),
        (0.0010499954223632812, "0.0010499954223632812"),
        (0.0011339187622070312, "0.0011339187622070312"),
        (0.0020227432250976562, "0.0020227432250976562"),
        (0.009943008422851562, "0.009943008422851562"),
        (0.07004165649414062, "0.07004165649414062"),
        (0.08716201782226562, "0.08716201782226562"),
        (0.09175491333007812, "0.09175491333007812"),
        (0.7210159301757812, "0.7210159301757812"),
        (0.8485183715820312, "0.8485183715820312"),
        (0.9550857543945312, "0.9550857543945312"),
        (23578090445.601562, "23578090445.601562"),
        (159562330159.14062, "159562330159.14062"),
        (739737452653.5312, "739737452653.5312"),
        (965509764881.4062, "965509764881.4062"),
        (1038184773832.2812, "1038184773832.2812"),
        (1498178914742.1562, "1498178914742.1562"),
        (1738271748034.5312, "1738271748034.5312"),
        (87754016791058.12, "87754016791058.12"),
        (90843643801111.12, "90843643801111.12"),
        (90862545561720.62, "90862545561720.62"),
        (108237785351538.12, "108237785351538.12"),
        (267974754781290.62, "267974754781290.62"),
        (981173252320198.2, "981173252320198.2"),
        (1260577940332290.2, "1260577940332290.2"),
        (1848794474891072.2, "1848794474891072.2"),
        (1862875407186800.2, "1862875407186800.2"),
    ];
    for (value, expected) in table {
        assert_eq!(python_repr_f64(value), expected, "repr of {value:?}");
    }

    // The two values of the fixture of finding #1, from the review record.
    assert_eq!(python_repr_f64(0.50000762939453125), "0.5000076293945312");
    assert_eq!(
        python_repr_f64(0.000093936920166015625),
        "9.393692016601562e-05"
    );
}

/// The three values where the tie rule must NOT fire, with the CPython text.
///
/// A value on a binade boundary has a rounding interval twice as wide above as
/// below. `2^-24` is exactly `5.9604644775390625e-08`, an exact halfway point
/// of the two 16-digit candidates, yet the lower candidate parses to the float
/// below it, so CPython keeps the odd last digit. `5e-324` and `2.5e-323` are
/// subnormal, where the interval is wider than the decimal step, so the lower
/// candidate round-trips while the halfway point is not exact.
///
/// ```text
/// $ /home/deploy/dev/cadus/.venv/bin/python
/// >>> repr(2.0 ** -24)
/// '5.960464477539063e-08'
/// >>> float('5.960464477539062e-08') == 2.0 ** -24
/// False
/// >>> repr(5e-324), repr(2.5e-323)
/// ('5e-324', '2.5e-323')
/// ```
#[test]
fn python_repr_f64_keeps_the_odd_digit_where_the_tie_is_not_a_tie() {
    assert_eq!(
        python_repr_f64(5.960464477539063e-08),
        "5.960464477539063e-08"
    );
    assert_eq!(python_repr_f64(5e-324), "5e-324");
    assert_eq!(python_repr_f64(2.5e-323), "2.5e-323");
}

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
    let root = fixture("arena-negative-zero");
    let (curriculum, findings) = load_curriculum(&root).expect("the fixture loads");
    assert!(findings.is_empty(), "the fixture has 0 parse findings");
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
    let root = fixture("arena-negative-zero");
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
    let root = fixture("arena-repeated-edge");
    let (curriculum, findings) = load_curriculum(&root).expect("the fixture loads");
    assert!(findings.is_empty(), "the fixture has 0 parse findings");
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
