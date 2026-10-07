//! Lane B5b: one test for each invariant on the fixture tree
//! `tests/fixtures/tree`. Each rule has a KP that breaches it, and the KP
//! `precalculus/fx/kp1` (and the named OK KP) that does not.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "b5b_support.rs"]
mod support;

use std::sync::OnceLock;

use support::{fixture, pairs, run, scratch};

/// Each finding of the fixture tree as `(kp, invariant)`. The self-test mode
/// reads each exemplar as new, so I12, I13 and I14 run.
fn findings() -> &'static [(String, String)] {
    static FOUND: OnceLock<Vec<(String, String)>> = OnceLock::new();
    FOUND.get_or_init(|| {
        let none = scratch("b5b_invariants_none.json", r#"{"defects": []}"#);
        let result = run(&["selftest", "--tree", &fixture("tree"), "--expected", &none]);
        assert_eq!(result.exit, 1, "{}", result.doc);
        pairs(&result.doc["extra"])
    })
}

/// The invariants of the findings of one KP.
fn of(kp: &str) -> Vec<&'static str> {
    findings()
        .iter()
        .filter(|(place, _)| place == kp)
        .map(|(_, invariant)| invariant.as_str())
        .collect()
}

fn assert_breach(kp: &str, invariant: &str, ok_kp: &str) {
    assert_eq!(of(kp), [invariant], "{kp}");
    assert_eq!(of(ok_kp), [] as [&str; 0], "{ok_kp}");
}

const CLEAN: &str = "precalculus/fx/kp1";

#[test]
fn i2_hard_floor_and_goal() {
    // kp2 has 3 verdict exemplars: below the typed floor of I9 as well.
    assert_eq!(of("precalculus/fx/kp2"), ["I2", "I9"]);
    assert_breach("precalculus/fx/kp3", "I2", CLEAN);
}

#[test]
fn i2_floor_is_four_for_geometry() {
    assert_eq!(of("geometry/gx/kp2"), ["I2", "I9"]);
    assert_eq!(of("geometry/gx/kp1"), [] as [&str; 0]);
}

#[test]
fn i3_mutant_that_grades_correct() {
    assert_breach("precalculus/fx/kp4", "I3", CLEAN);
}

#[test]
fn i4_none_is_legal_only_as_the_last_item_of_a_proof_kp() {
    assert_breach("precalculus/fx/kp5", "I4", CLEAN);
    assert_breach("proofs/px/kp2", "I4", "proofs/px/kp1");
}

#[test]
fn i5_equal_problem() {
    assert_breach("precalculus/fx/kp6", "I5", CLEAN);
}

#[test]
fn i6_digit_skeleton() {
    assert_breach("precalculus/fx/kp7", "I6", CLEAN);
}

#[test]
fn i7_different_keys() {
    assert_breach("precalculus/fx/kp8", "I7", CLEAN);
}

#[test]
fn i8_label_option_count() {
    assert_breach("precalculus/fx/kp9", "I8", CLEAN);
}

#[test]
fn i9_typed_verdict_floor() {
    assert_breach("precalculus/fx/kp10", "I9", "precalculus/not-longest/kp1");
}

#[test]
fn i10_file_rule_names_the_first_kp_of_the_file() {
    assert_breach(
        "precalculus/longest/kp1",
        "I10",
        "precalculus/not-longest/kp1",
    );
    assert_eq!(of("precalculus/longest/kp2"), [] as [&str; 0]);
}

#[test]
fn i11_correct_option_two_times() {
    assert_breach("precalculus/fx/kp11", "I11", "precalculus/not-longest/kp2");
}

#[test]
fn i12_answer_format() {
    assert_breach("precalculus/fx/kp12", "I12", CLEAN);
}

#[test]
fn i13_sketch() {
    assert_breach("precalculus/fx/kp13", "I13", CLEAN);
}

#[test]
fn i14_explicit_contract() {
    assert_breach("precalculus/fx/kp14", "I14", CLEAN);
}

#[test]
fn d27_multipart_of_label_parts() {
    assert_breach("precalculus/fx/kp15", "D27", CLEAN);
}

#[test]
fn r3_formula_key_under_the_exact_comparison() {
    assert_breach("precalculus/fx/kp16", "R3", CLEAN);
}

#[test]
fn no_other_finding_is_in_the_fixture_tree() {
    assert_eq!(findings().len(), 20, "{:?}", findings());
}
