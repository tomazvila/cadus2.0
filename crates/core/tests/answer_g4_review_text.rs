//! Grader pass 4, review of text readers: label sentences, name prefixes,
//! named parts, the capital `E`, and thousands groups. Each accepted spelling
//! has its wrong twin.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

const EXACT: &str = r#"{"kind":"exact"}"#;
const SET: &str = r#"{"kind":"set"}"#;
const POINT2: &str = r#"{"kind":"coordinates","arity":2}"#;
const YES_NO: &str = r#"{"kind":"label","options":[["yes"],["no"]]}"#;
const SOLIDS: &str = r#"{"kind":"label","options":[["circular cylinder"],["cone"]]}"#;
const PARITY: &str = r#"{"kind":"label","options":[["even"],["odd"]]}"#;
const SERIES: &str = r#"{"kind":"label","options":[["converges"],["diverges"]]}"#;
const GROUPS: &str = r#"{"kind":"label","options":[["Z_2"],["Z_4"]]}"#;
const KLEIN: &str = r#"{"kind":"label","options":[["Z_2×Z_2","Klein group"],["Z_4"]]}"#;
const NEITHER: &str = r#"{"kind":"label","options":[["even"],["odd"],["neither"]]}"#;

#[derive(Debug, PartialEq, Eq)]
enum V {
    Right,
    Wrong,
    Ungraded,
}

fn verdict(json: &str, key: &str, learner: &str) -> V {
    let contract: AnswerContract = serde_json::from_str(json).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(v) if v.correct => V::Right,
        Outcome::Decided(_) => V::Wrong,
        Outcome::Undecidable(_) => V::Ungraded,
    }
}

fn expect(json: &str, key: &str, learner: &str, want: &V) {
    assert_eq!(&verdict(json, key, learner), want, "{key} / {learner}");
}

fn rows(json: &str, key: &str, right: &[&str], wrong: &[&str], ungraded: &[&str]) {
    for learner in right {
        expect(json, key, learner, &V::Right);
    }
    for learner in wrong {
        expect(json, key, learner, &V::Wrong);
    }
    for learner in ungraded {
        expect(json, key, learner, &V::Ungraded);
    }
}

// ---- label sentences that open with an alias

#[test]
fn a_denied_yes_is_not_a_yes() {
    rows(
        YES_NO,
        "yes",
        &["yes, not no", "yes, because it is closed", "yes it is"],
        &["yes, it is not", "yes it does not"],
        &["yes, it is not abelian"],
    );
}

#[test]
fn a_contrast_names_the_other_option() {
    rows(
        PARITY,
        "even",
        &["even and not odd", "even, not odd", "it is even, not odd"],
        &["even, not even", "even and odd"],
        &[],
    );
    rows(
        SERIES,
        "converges",
        &["converges, not diverges"],
        &["converges, diverges"],
        &[],
    );
    rows(
        SOLIDS,
        "cone",
        &[
            "it is a cone, not a cylinder",
            "cone, not a circular cylinder",
        ],
        &["cone, cylinder", "cone cylinder", "cone, not a cone"],
        &[],
    );
    rows(
        KLEIN,
        "Z_2×Z_2",
        &["Klein group, not Z_4"],
        &["Z_4, not Klein group"],
        &[],
    );
}

#[test]
fn a_product_of_groups_is_another_group() {
    rows(
        GROUPS,
        "Z_2",
        &["Z_2", "Z_2, because it has order 2"],
        &["Z_2 x Z_2", "Z_2 + Z_2", "Z_2 times Z_2", "Z_2 × Z_2"],
        &[],
    );
}

#[test]
fn an_option_that_is_a_negation_keeps_its_nor() {
    rows(NEITHER, "neither", &["neither even nor odd"], &[], &[]);
}

#[test]
fn the_alias_table_stays_as_authored() {
    expect(YES_NO, "yes", "true", &V::Wrong);
    expect(YES_NO, "no", "nope", &V::Wrong);
    expect(YES_NO, "yes", "y", &V::Wrong);
}

// ---- name prefixes

#[test]
fn only_a_single_name_is_stripped() {
    rows(
        EXACT,
        "3",
        &[
            "x = 3",
            "f(x) = 3",
            "g'(x) = 3",
            "P(X=1) = 3",
            "dim = 3",
            "a_n = 3",
            "f^-1(x) = 3",
        ],
        &[],
        &[
            "x + 1 = 3",
            "x^2 = 3",
            "sqrt(x) = 3",
            "e^x = 3",
            "x/2 = 3",
            "x - y = 3",
            "x-y = 3",
            "x(x+1) = 3",
            "f(x)+1 = 3",
            "2x = 3",
        ],
    );
    rows(EXACT, "5", &[], &[], &["x^2 + y^2 = 5", "r^2 = 5"]);
    rows(
        EXACT,
        "2x+1",
        &["y = 2x+1", "f(x) = 2x+1"],
        &[],
        &["x = 2x+1"],
    );
}

// ---- named parts

#[test]
fn named_coordinates_read_by_name() {
    rows(
        POINT2,
        "(2,3)",
        &[
            "x = 2, y = 3",
            "y = 3, x = 2",
            "y=3, x=2",
            "(x = 2, y = 3)",
            "(2, 3)",
        ],
        &["y = 2, x = 3", "x = 3, y = 2"],
        &["x = 2, x = 3"],
    );
}

#[test]
fn named_parts_of_a_bare_key_keep_the_order_of_the_key() {
    rows(
        EXACT,
        "2, 5",
        &["x=2, y=5", "y=5, x=2", "5, 2", "a=2, b=5"],
        &[
            "x=5, y=2",
            "y=2, x=5",
            "a=5, b=2",
            "min 5, max 2",
            "max 2, min 5",
        ],
        &[],
    );
    rows(
        EXACT,
        "3, pi/2",
        &["amplitude 3, period π/2"],
        &["amplitude π/2, period 3"],
        &[],
    );
}

// ---- the capital E

#[test]
fn a_capital_e_is_a_variable_name() {
    rows(EXACT, "e^x", &["e^x"], &[], &["E^x"]);
    rows(EXACT, "2e", &["2e"], &[], &["2E"]);
    rows(EXACT, "1500", &["1.5E3", "1.5e3"], &[], &[]);
    expect(EXACT, "e", "E", &V::Wrong);
}

// ---- thousands groups

#[test]
fn a_grouped_number_with_a_decimal_part_is_one_value() {
    rows(
        EXACT,
        "1205",
        &["1,205", "1,205.0", "1,205.00"],
        &["1,206.0"],
        &[],
    );
    rows(EXACT, "1205.5", &["1,205.50"], &["1,205.60"], &[]);
}

#[test]
fn a_grouped_number_inside_a_collection_is_one_member() {
    rows(
        SET,
        "{1205, 3}",
        &["{1,205, 3}", "1,205 and 3", "{1205, 3}"],
        &["{1,206, 3}", "1,205 and 4"],
        &[],
    );
    rows(SET, "{1205}", &["1,205", "{1,205}"], &["1,206"], &[]);
    rows(
        POINT2,
        "(1205, 3)",
        &["(1,205, 3)", "(1205, 3)"],
        &["(1,206, 3)"],
        &[],
    );
    rows(POINT2, "(1,205)", &["1,205", "(1,205)"], &["1,206"], &[]);
}
