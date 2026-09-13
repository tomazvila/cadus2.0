//! Pinned regression checks for the canonical dump, curriculum hash, and
//! float formatting. Historical expectations are recorded in
//! `docs/reference/curriculum-1.0-spec.md` and the answer-contract reports.
//! Reviewed curriculum corrections and their exact source bindings live in
//! `docs/content-foundations/curriculum-reviews/`.

mod common;

use cadus_core::curriculum::{
    DUMP_SCHEMA, canonical_dump, curriculum_hash, python_repr_f64, sha256_hex,
};
use common::dump::tree;

/// The semantic curriculum hash of the checked-in tree (spec section 3).
const TREE_HASH: &str = "4afb128b42fa599bc4ec40d612f914cc9667f75459ff99f8f4e3ee3c5ef7b5a6";

/// The length of the dump in bytes, without the trailing newline.
const DUMP_LEN: usize = 4_631_098;

/// The `counts` object of the dump, as the oracle writes it.
const COUNTS: &str = "\"counts\":{\"anki_seeds\":2144,\"courses\":13,\"encompassing_edges\":3200,\"exemplars\":8352,\"knowledge_points\":3138,\"prereq_edges\":3281,\"topics\":1090,\"units\":88}";

// --------------------------------------------------------------------------- //
// The hash and the shape of the dump
// --------------------------------------------------------------------------- //

#[test]
fn curriculum_hash_covers_the_reviewed_answer_contracts() {
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
