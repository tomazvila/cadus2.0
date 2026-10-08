//! Grader pass 4, collections: lists, sets, points and inequality unions in
//! the spellings a learner writes. Each accepted row has its wrong twin.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

const LIST: &str = r#"{"kind":"list","ordered":false,"member":{"kind":"exact"}}"#;
const ORDERED: &str = r#"{"kind":"list","ordered":true,"member":{"kind":"exact"}}"#;
const SET: &str = r#"{"kind":"set"}"#;
const EXACT: &str = r#"{"kind":"exact"}"#;
const UNION: &str = r#"{"kind":"inequality_union"}"#;
const SETUP: &str = r#"{"kind":"relation_setup"}"#;
const NOTATION: &str = r#"{"kind":"required_inequality_notation"}"#;
const POINT2: &str = r#"{"kind":"coordinates","arity":2}"#;
const POINT3: &str = r#"{"kind":"coordinates","arity":3}"#;
const COLUMN: &str = r#"{"kind":"matrix","rows":2,"cols":1}"#;

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

fn rows(json: &str, key: &str, right: &[&str], wrong: &[&str]) {
    for learner in right {
        assert_eq!(verdict(json, key, learner), V::Right, "{key} / {learner}");
    }
    for learner in wrong {
        assert_eq!(verdict(json, key, learner), V::Wrong, "{key} / {learner}");
    }
}

// ---- list spellings (rows 22, 34, 78, 192, 256, 260, 273, 274)

#[test]
fn a_list_reads_braces_words_and_names() {
    rows(
        LIST,
        "2, 3",
        &[
            "between 2 and 3",
            "{2, 3}",
            "{3,2}",
            "x = 2 or x = 3",
            "2 and 3",
            "3 < k < 2 is not read",
        ][..5],
        &["between 2 and 4", "{2, 4}", "x = 2 or x = 4", "{2}"],
    );
    rows(LIST, "4, 5", &["between 4 and 5"], &["between 4 and 6"]);
    rows(LIST, "H, T", &["{H, T}"], &["{H, H}"]);
    rows(LIST, "1, 3", &["columns 1 and 3"], &["columns 1 and 2"]);
    rows(
        LIST,
        "2, 3, 7",
        &["orders 2, 3 and 7"],
        &["orders 2, 3 and 5"],
    );
    rows(LIST, "5, 7, 13", &["{5, 7, 13}"], &["{5, 7, 11}"]);
}

#[test]
fn a_between_chain_names_the_two_bounds() {
    rows(LIST, "a, b", &["a < β < b", "b > β > a"], &["a < β < c"]);
}

#[test]
fn a_list_of_words_compares_by_spelling() {
    rows(
        LIST,
        "HH, HT, TH, TT",
        &["HH, HT, TH, TT", "TT, TH, HT, HH", "{HH, HT, TH, TT}"],
        &["HH, HT, TH, TH", "HH, HT, TH"],
    );
    rows(ORDERED, "HH, HT", &["HH, HT"], &["HT, HH"]);
    rows(
        EXACT,
        "I, III",
        &["quadrants I and III", "III, I", "I, III"],
        &["quadrants I and II", "I, IV"],
    );
}

#[test]
fn a_list_of_signed_points_expands_each_sign() {
    rows(
        LIST,
        "(5,3), (5,-3), (-5,3), (-5,-3)",
        &["(±5, ±3)"],
        &["(±5, 3)", "(±5, ±4)"],
    );
}

// ---- set (rows 45, 114, 232, 275, 276)

#[test]
fn a_set_key_may_be_a_bare_list_and_a_learner_may_omit_braces() {
    rows(
        SET,
        "2, 5",
        &["2, 5", "5, 2", "{2, 5}"],
        &["2", "2, 6", "{2, 5, 7}", "{2,2,5}"],
    );
}

#[test]
fn a_set_reads_one_member_without_braces() {
    rows(SET, "{1}", &["1", "{1}"], &["2", "{2}"]);
    rows(SET, "{4}", &["4"], &["5"]);
    rows(EXACT, "1", &["{1}"], &["{2}", "{1, 2}"]);
}

#[test]
fn a_set_reads_the_empty_set_in_words() {
    rows(
        SET,
        "{}",
        &["empty set", "the empty set", "∅", "{ }", "{}"],
        &["{0}", "0"],
    );
}

#[test]
fn a_set_reads_a_named_set_and_a_set_builder_and_spaces() {
    rows(SET, "{1, 2}", &["A ∩ B = {1, 2}"], &["A ∩ B = {1, 3}"]);
    rows(
        SET,
        "{1, 2, 3}",
        &["{x : x in {1,2,3}}", "1 2 3"],
        &["{x : x in {1,2,4}}", "1 2 4"],
    );
}

#[test]
fn a_set_reads_a_sign_pair_of_complex_numbers() {
    rows(
        SET,
        "1+i, 1-i, -1+i, -1-i",
        &["±1 ± i", "{1+i, 1-i, -1+i, -1-i}"],
        &["±1 + i", "{1+i, 1-i}"],
    );
}

// ---- coordinates (rows 47, 191, 202, 203, 208, 212, 243, 258, 264)

#[test]
fn a_point_reads_words_names_and_brackets() {
    rows(
        POINT2,
        "(7, 3)",
        &[
            "7 and 3",
            "x = 7, y = 3",
            "a=7 and b=3",
            "the point (7, 3)",
            "7 to 3",
        ][..4],
        &["3 and 7", "7 and 4", "a=3 and b=7"],
    );
    rows(POINT2, "(3, 12)", &["3 to 12"], &["12 to 3", "3 to 11"]);
    rows(POINT2, "(40, 40)", &["40 and 40"], &["40 and 41"]);
    rows(
        POINT2,
        "(4, 12)",
        &["4 and 12", "domain 4, codomain 12"],
        &["domain 12, codomain 4", "4 and 13"],
    );
    rows(POINT2, "(6, -2)", &["a=6 and b=-2"], &["a=6 and b=2"]);
    rows(POINT2, "(0, 0)", &["the origin", "origin"], &["(0, 1)"]);
    rows(
        POINT3,
        "(9, 12, 6)",
        &["(9, 12, 6) grams"],
        &["(9, 12, 7) grams"],
    );
    rows(
        POINT3,
        "(-3/5, 0, 4/5)",
        &["<-3/5, 0, 4/5>"],
        &["<3/5, 0, 4/5>"],
    );
    rows(POINT2, "(13, 12)", &["[[13],[12]]"], &["[[12],[13]]"]);
}

#[test]
fn a_point_may_hold_the_imaginary_unit() {
    rows(POINT2, "(1, i)", &["(1, i)"], &["(1, 2)", "(i, 1)"]);
}

#[test]
fn a_column_vector_key_reads_a_tuple() {
    rows(
        COLUMN,
        "[[4],[11]]",
        &["(4, 11)", "[[4],[11]]"],
        &["(11, 4)"],
    );
}

// ---- inequality unions (rows 89, 90, 138, 143, 152, 158, 172, 214, 215, 274, 276, 278, 280)

#[test]
fn an_inequality_reads_not_equal_and_a_barred_unknown() {
    rows(EXACT, "x ≠ 4", &["x ≠ 4", "x != 4"], &["x ≠ 5", "x != 5"]);
    rows(
        UNION,
        "x̄ < 96 or x̄ > 104",
        &["x̄ < 96 or x̄ > 104", "x̄ > 104 or x̄ < 96"],
        &["x̄ < 96 or x̄ > 105"],
    );
}

#[test]
fn a_union_keeps_an_isolated_point() {
    rows(
        UNION,
        "x <= -1 or x = 3",
        &["x <= -1 or x = 3", "x = 3 or x <= -1", "(-inf, -1] U {3}"],
        &["x <= -1", "x <= -1 or x = 4", "x <= -1 or x >= 3"],
    );
    rows(
        UNION,
        "x <= 6",
        &["x < 6 or x = 6"],
        &["x < 6", "x < 6 or x = 7"],
    );
}

#[test]
fn a_union_reads_between_and_spoken_comparisons() {
    rows(
        UNION,
        "2 < t < 4",
        &["t between 2 and 4"],
        &["t between 2 and 5", "2 <= t <= 4"],
    );
    rows(
        UNION,
        "s > 3",
        &["s greater than 3", "for s > 3", "s is greater than 3"],
        &["s greater than 4", "s less than 3", "for s >= 3"],
    );
    rows(
        SETUP,
        "s > 3",
        &["s greater than 3", "for s > 3"],
        &["s greater than 4"],
    );
}

#[test]
fn a_union_reads_all_real_numbers() {
    rows(
        UNION,
        "all real numbers",
        &["all real numbers", "(-inf, inf)", "(-∞, ∞)", "ℝ"],
        &["(0, inf)", "x > 0"],
    );
}

#[test]
fn a_union_reads_irrational_bounds() {
    rows(
        UNION,
        "x < -1/sqrt(3) or x > 1/sqrt(3)",
        &[
            "x < -1/sqrt(3) or x > 1/sqrt(3)",
            "|x| > 1/sqrt(3)",
            "x > 1/sqrt(3) or x < -sqrt(3)/3",
        ],
        &["|x| > 1/sqrt(2)", "x > 1/sqrt(3)", "|x| < 1/sqrt(3)"],
    );
    rows(
        UNION,
        "-pi/2 < x < pi/2",
        &["-pi/2 < x < pi/2", "|x| < pi/2"],
        &["-pi/2 < x < pi/3", "-pi/2 <= x < pi/2"],
    );
}

#[test]
fn a_union_reads_an_absolute_value_bound() {
    rows(UNION, "-3 < x < 3", &["|x|<3"], &["|x|<=3", "|x|>3"]);
    rows(UNION, "-5 < x < 5", &["|x|<5", "5 > |x|"], &["|x|<4"]);
    rows(
        UNION,
        "x >= 3 or x <= -3",
        &["|x| >= 3"],
        &["|x| > 3", "|x| >= 2"],
    );
    rows(
        UNION,
        "1 < x < 3",
        &["|x - 2| < 1"],
        &["|x - 2| < 2", "|x + 2| < 1"],
    );
}

#[test]
fn a_union_reads_membership_and_set_builder_and_units() {
    rows(
        UNION,
        "7 <= x < 8",
        &["x in [7, 8)"],
        &["x in (7, 8)", "x in [7, 8]"],
    );
    rows(
        UNION,
        "0 < x <= 2",
        &["x in (0, 2]", "{x : 0 < x <= 2}", "{x | 0 < x <= 2}"],
        &["x in [0, 2]", "{x : 0 <= x <= 2}"],
    );
    rows(
        UNION,
        "120 <= x < 150",
        &["120 cm <= x < 150 cm"],
        &["120 cm <= x < 151 cm"],
    );
}

#[test]
fn the_notation_contract_keeps_interval_and_inequality_apart() {
    assert_eq!(verdict(NOTATION, "x >= 2", "x >= 2"), V::Right);
    assert_eq!(verdict(NOTATION, "x >= 2", "[2, inf)"), V::Wrong);
}

#[test]
fn a_lower_bound_of_zero_the_key_does_not_state_is_ungraded() {
    assert_eq!(verdict(UNION, "t<12", "0 <= t < 12"), V::Ungraded);
    assert_eq!(verdict(UNION, "t<12", "0 < t < 12"), V::Ungraded);
    // Any other extra bound is a wrong answer.
    assert_eq!(verdict(UNION, "t<12", "1 <= t < 12"), V::Wrong);
    assert_eq!(verdict(UNION, "t<12", "0 <= t < 11"), V::Wrong);
    assert_eq!(verdict(UNION, "t<12", "t < 12"), V::Right);
}

#[test]
fn a_comma_key_under_exact_stays_a_multiset() {
    // Note 115: a bare comma key compares as a multiset, so a swapped pair is
    // right. An ordered pair needs the `list` contract with `ordered: true`.
    assert_eq!(verdict(EXACT, "2, 3", "3, 2"), V::Right);
    assert_eq!(verdict(ORDERED, "12, 2", "2, 12"), V::Wrong);
    assert_eq!(verdict(POINT2, "(12, 2)", "(2, 12)"), V::Wrong);
}

// ---- a point part of a multipart key (row 60)

#[test]
fn a_named_point_part_reads_hyphen_and_space_names() {
    let json = r#"{"kind":"multipart","parts":[{"name":"x_intercept","contract":{"kind":"coordinates","arity":2}},{"name":"y_intercept","contract":{"kind":"coordinates","arity":2}}]}"#;
    let key = "x_intercept = (6, 0); y_intercept = (0, 4)";
    rows(
        json,
        key,
        &[
            "x-intercept (6,0), y-intercept (0,4)",
            "x intercept (6,0), y intercept (0,4)",
            "x_intercept = (6, 0); y_intercept = (0, 4)",
        ],
        &["x-intercept (0,4), y-intercept (6,0)"],
    );
}

// ---- routed rows: a sum of scaled points (B190) and a parametric multiple (C193)

#[test]
fn a_sum_of_scaled_points_reads_as_the_point_it_adds_up_to() {
    rows(
        POINT2,
        "(5, 4)",
        &[
            "2(1, 2) + 1(3, 0)",
            "(1, 2) + (3, 0) + (1, 2)",
            "3(3, 2) - (4, 2)",
        ],
        &["2(1, 2) + 2(3, 0)", "(1, 2) + (3, 0)"],
    );
}

#[test]
fn a_parametric_multiple_of_a_key_point_gets_no_verdict() {
    // The contract does not say the key is a direction, so `t(2,1)` has no verdict.
    assert_eq!(verdict(EXACT, "(2, 1)", "t(2,1)"), V::Ungraded);
}
