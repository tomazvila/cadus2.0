//! The `property` contract: the learner's object is tested against a named
//! predicate, and the stored answer is one valid example.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use cadus_core::curriculum::parse_curriculum;

fn contract(json: &str) -> AnswerContract {
    serde_json::from_str(json).unwrap_or_else(|error| panic!("{json}: {error}"))
}

fn verdict(contract: &AnswerContract, example: &str, learner: &str) -> Option<bool> {
    match check_contract(example, learner, contract.clone()) {
        Outcome::Decided(verdict) => Some(verdict.correct),
        Outcome::Undecidable(_) => None,
    }
}

/// Every learner answer in `right` grades correct, every one in `wrong` grades
/// wrong, and every one in `garbage` gives no verdict.
fn table(json: &str, example: &str, right: &[&str], wrong: &[&str], garbage: &[&str]) {
    let contract = contract(json);
    assert!(
        contract.validate_expected(example).is_ok(),
        "{json}: the example {example:?} must have the property"
    );
    for learner in right {
        assert_eq!(
            verdict(&contract, example, learner),
            Some(true),
            "{json}: {learner:?}"
        );
    }
    for learner in wrong {
        assert_eq!(
            verdict(&contract, example, learner),
            Some(false),
            "{json}: {learner:?}"
        );
    }
    for learner in garbage {
        assert_eq!(
            verdict(&contract, example, learner),
            None,
            "{json}: {learner:?}"
        );
    }
}

const GARBAGE: [&str; 3] = ["nine?!", "3 +", "@@"];

#[test]
fn divisor_count_accepts_any_number_with_exactly_n_divisors() {
    table(
        r#"{"kind":"property","check":"divisor_count","args":{"n":3}}"#,
        "9",
        &[
            "9",
            "4",
            "25",
            "49",
            "121",
            "18/2",
            "9.0",
            "n = 49",
            "  169 ",
            "sqrt(9)+1",
        ],
        &[
            "8", "16", "6", "1", "0", "-9", "10", "9/2", "2", "sqrt(2)", "x",
        ],
        &GARBAGE,
    );
    table(
        r#"{"kind":"property","check":"divisor_count","args":{"n":6,"max":50}}"#,
        "12",
        &["12", "18", "20", "28", "32", "44", "45", "50"],
        &["52", "36", "16", "11"],
        &[],
    );
}

#[test]
fn prime_and_composite_classify_integers_exactly() {
    table(
        r#"{"kind":"property","check":"prime","args":{"min":20,"max":40}}"#,
        "23",
        &["23", "29", "31", "37", "46/2"],
        &["19", "41", "21", "25", "33", "39", "27", "23.5"],
        &GARBAGE,
    );
    table(
        r#"{"kind":"property","check":"composite"}"#,
        "91",
        &["91", "4", "1000000", "561", "2047"],
        &["1", "0", "2", "97", "-4", "1/2", "7919"],
        &GARBAGE,
    );
    // A number past the proved Miller-Rabin range gives no verdict.
    assert_eq!(
        verdict(
            &contract(r#"{"kind":"property","check":"prime"}"#),
            "7",
            "100000000000000000000000000000000000000"
        ),
        None
    );
}

#[test]
fn multiple_of_and_divisor_of_read_integers_in_any_form() {
    table(
        r#"{"kind":"property","check":"multiple_of","args":{"k":6,"min":1}}"#,
        "42",
        &["6", "42", "600", "84/2", "42.0", "6 x 10^2"],
        &["0", "-6", "40", "15", "6.5"],
        &GARBAGE,
    );
    table(
        r#"{"kind":"property","check":"divisor_of","args":{"k":36}}"#,
        "12",
        &["1", "2", "3", "4", "6", "9", "12", "18", "36"],
        &["5", "8", "24", "72", "0", "-6", "3/2"],
        &GARBAGE,
    );
}

#[test]
fn between_is_strict_and_reads_every_exact_form() {
    table(
        r#"{"kind":"property","check":"between","args":{"low":"2/5","high":"1/2"}}"#,
        "9/20",
        &[
            "9/20",
            "0.45",
            "45%",
            "0.41",
            "0.499",
            "4/9",
            "\\frac{9}{20}",
            "x = 0.42",
        ],
        &["1/2", "2/5", "0.5", "0.4", "0.51", "1/3", "x", "(1, 2)"],
        &GARBAGE,
    );
}

#[test]
fn integer_in_range_checks_the_inclusive_bounds() {
    table(
        r#"{"kind":"property","check":"integer_in_range","args":{"min":-3,"max":"3"}}"#,
        "0",
        &["-3", "0", "3", "6/2"],
        &["4", "-4", "1/2", "2.5"],
        &GARBAGE,
    );
}

#[test]
fn a_prime_counterexample_needs_n_in_range_and_a_value_that_is_not_prime() {
    table(
        r#"{"kind":"property","check":"prime_counterexample","args":{"expr":"n^2 + n + 41","min":0}}"#,
        "40",
        // 40: 1681 = 41^2. 41: 41 * 43. 44: 2021 = 43 * 47.
        &["40", "41", "44", "n = 40", "80/2"],
        // 0..39 all give primes; -1 is below the bound; 40.5 is not an integer.
        &["0", "1", "10", "39", "-1", "40.5"],
        &GARBAGE,
    );
    table(
        r#"{"kind":"property","check":"prime_counterexample","args":{"expr":"2*k + 1","var":"k","min":1}}"#,
        "4",
        &["4", "7", "10"],
        &["1", "2", "3", "5", "0"],
        &[],
    );
}

#[test]
fn the_stored_example_must_have_the_property() {
    for (json, example) in [
        (
            r#"{"kind":"property","check":"divisor_count","args":{"n":3}}"#,
            "6",
        ),
        (
            r#"{"kind":"property","check":"between","args":{"low":"2/5","high":"1/2"}}"#,
            "1/2",
        ),
        (r#"{"kind":"property","check":"prime"}"#, "1"),
        (
            r#"{"kind":"property","check":"prime_counterexample","args":{"expr":"n^2+n+41","min":0}}"#,
            "39",
        ),
        (
            r#"{"kind":"property","check":"multiple_of","args":{"k":7}}"#,
            "nonsense!",
        ),
    ] {
        let contract = contract(json);
        assert!(
            contract.validate_expected(example).is_err(),
            "{json}: {example}"
        );
        assert!(
            matches!(
                check_contract(example, example, contract),
                Outcome::Undecidable(_)
            ),
            "{json}: {example}"
        );
    }
}

#[test]
fn bad_arguments_fail_the_typed_parse_with_a_named_reason() {
    for (json, reason) in [
        (
            r#"{"kind":"property","check":"divisor_count"}"#,
            "needs `n`",
        ),
        (
            r#"{"kind":"property","check":"divisor_count","args":{"n":0}}"#,
            "from 1 through 64",
        ),
        (
            r#"{"kind":"property","check":"multiple_of","args":{"k":0}}"#,
            "nonzero `k`",
        ),
        (
            r#"{"kind":"property","check":"divisor_of","args":{"k":"ten"}}"#,
            "not an integer",
        ),
        (
            r#"{"kind":"property","check":"between","args":{"low":"1/2","high":"2/5"}}"#,
            "less than",
        ),
        (
            r#"{"kind":"property","check":"between","args":{"low":"x","high":"1"}}"#,
            "exact number",
        ),
        (
            r#"{"kind":"property","check":"integer_in_range"}"#,
            "`min`, `max`",
        ),
        (
            r#"{"kind":"property","check":"integer_in_range","args":{"min":5,"max":1}}"#,
            "must not exceed",
        ),
        (
            r#"{"kind":"property","check":"prime","args":{"k":3}}"#,
            "does not take",
        ),
        (
            r#"{"kind":"property","check":"prime_counterexample","args":{"expr":"sin(n)"}}"#,
            "polynomial",
        ),
        (
            r#"{"kind":"property","check":"prime_counterexample","args":{"expr":"m^2+1"}}"#,
            "polynomial",
        ),
        (
            r#"{"kind":"property","check":"prime_counterexample","args":{"expr":"n^2","var":"NN"}}"#,
            "lowercase",
        ),
        (
            r#"{"kind":"property","check":"perfect"}"#,
            "unknown variant",
        ),
    ] {
        let error = serde_json::from_str::<AnswerContract>(json)
            .err()
            .unwrap_or_else(|| panic!("{json} must be refused"))
            .to_string();
        assert!(error.contains(reason), "{json}: {error}");
    }
}

#[test]
fn the_contract_round_trips_through_json() {
    let json = r#"{"kind":"property","check":"between","args":{"high":"1/2","low":"2/5"}}"#;
    let parsed = contract(json);
    assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
    let bare = r#"{"kind":"property","check":"prime"}"#;
    assert_eq!(serde_json::to_string(&contract(bare)).unwrap(), bare);
    assert_eq!(
        parsed.property_description().as_deref(),
        Some("a rational number strictly between 2/5 and 1/2, in any exact form")
    );
    assert_eq!(AnswerContract::Exact.property_description(), None);
}

#[test]
fn the_near_misses_hold_a_wrong_answer_for_every_predicate() {
    for (json, example) in [
        (
            r#"{"kind":"property","check":"divisor_count","args":{"n":3}}"#,
            "9",
        ),
        (
            r#"{"kind":"property","check":"between","args":{"low":"2/5","high":"1/2"}}"#,
            "9/20",
        ),
        (
            r#"{"kind":"property","check":"prime_counterexample","args":{"expr":"n^2+n+41","min":0}}"#,
            "40",
        ),
        (
            r#"{"kind":"property","check":"integer_in_range","args":{"min":1,"max":100}}"#,
            "50",
        ),
        (
            r#"{"kind":"property","check":"divisor_of","args":{"k":36}}"#,
            "12",
        ),
    ] {
        let contract = contract(json);
        let misses = contract.property_near_misses(example).unwrap();
        assert!(
            misses
                .iter()
                .any(|learner| verdict(&contract, example, learner) == Some(false)),
            "{json}: {misses:?}"
        );
    }
}

#[test]
fn a_property_contract_is_not_a_list_member() {
    let json = r#"{"kind":"list","ordered":false,"member":{"kind":"property","check":"prime"}}"#;
    assert!(serde_json::from_str::<AnswerContract>(json).is_err());
}

#[test]
fn the_loader_refuses_bad_property_args_with_the_reason() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("property-loader");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("c")).unwrap();
    fs::write(
        root.join("courses.yaml"),
        "courses:\n- id: c\n  name: C\n  order: 1\n",
    )
    .unwrap();
    let unit = |contract: &str| {
        format!(
            "unit: u\ncourse: c\nmodule: M\ntopics:\n- id: a\n  name: A\n  difficulty: 0.2\n  \
             answer_kind: expression\n  expected_time_secs: 30\n  knowledge_points:\n  - id: kp1\n    \
             name: K\n    exemplars:\n    - problem: Give a number with exactly three factors.\n      \
             answer: '9'\n      answer_contract: {contract}\n      solution_sketch: 9 has 1, 3, 9.\n"
        )
    };
    fs::write(
        root.join("c/00.yaml"),
        unit(r#"{"kind": "property", "check": "divisor_count", "args": {"n": 0}}"#),
    )
    .unwrap();
    let parsed = parse_curriculum(&root);
    let messages: Vec<String> = parsed.findings.iter().map(|f| f.message.clone()).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("divisor_count needs `n` from 1 through 64")),
        "{messages:?}"
    );
    fs::write(
        root.join("c/00.yaml"),
        unit(r#"{"kind": "property", "check": "divisor_count", "args": {"n": 3}}"#),
    )
    .unwrap();
    let parsed = parse_curriculum(&root);
    assert!(parsed.findings.is_empty(), "{:?}", parsed.findings);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_integer_filters_combine_with_every_integer_check() {
    table(
        r#"{"kind":"property","check":"multiple_of","args":{"k":3,"not_multiple_of":9,"min":100,"max":999}}"#,
        "102",
        &["102", "105", "111", "993"],
        &["108", "117", "99", "1002", "101"],
        &GARBAGE,
    );
    table(
        r#"{"kind":"property","check":"composite","args":{"coprime_to":30,"max":100}}"#,
        "49",
        &["49", "77", "91"],
        &["51", "25", "97", "121", "1"],
        &GARBAGE,
    );
    table(
        r#"{"kind":"property","check":"coprime_to","args":{"k":12,"min":10,"max":20}}"#,
        "11",
        &["11", "13", "17", "19"],
        &["10", "12", "14", "15", "16", "18", "20", "1", "23"],
        &GARBAGE,
    );
    assert_eq!(
        contract(r#"{"kind":"property","check":"multiple_of","args":{"k":3,"not_multiple_of":9,"min":100,"max":999}}"#)
            .property_description()
            .as_deref(),
        Some("a multiple of 3 (from 100 through 999; not a multiple of 9)")
    );
    for json in [
        r#"{"kind":"property","check":"prime","args":{"not_multiple_of":1}}"#,
        r#"{"kind":"property","check":"prime","args":{"coprime_to":0}}"#,
        r#"{"kind":"property","check":"coprime_to"}"#,
        r#"{"kind":"property","check":"between","args":{"low":"0","high":"1","min":0}}"#,
    ] {
        assert!(
            serde_json::from_str::<AnswerContract>(json).is_err(),
            "{json}"
        );
    }
}

#[test]
fn the_multiple_of_filter_narrows_a_divisor_check() {
    table(
        r#"{"kind":"property","check":"divisor_of","args":{"k":48,"multiple_of":4}}"#,
        "12",
        &["4", "8", "12", "16", "24", "48"],
        &["2", "6", "3", "32", "96"],
        &GARBAGE,
    );
    let doubled = r#"{"kind":"property","check":"multiple_of","args":{"k":4,"multiple_of":6}}"#;
    assert!(serde_json::from_str::<AnswerContract>(doubled).is_err());
}
