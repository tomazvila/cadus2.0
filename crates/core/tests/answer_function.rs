//! The `function` contract: document shape, `FunctionSpec`, and sample points.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

use cadus_core::answer::contract::function::{
    CONSTANT_NAMES, DEFAULT_DOMAIN, FunctionSpec, MIN_FINITE_POINTS, SAMPLE_FRACTIONS, TOLERANCE,
};
use cadus_core::answer::{
    AnswerContract, AnswerPart, Outcome, TriageVerdict, check_contract, triage_verdict,
};
use cadus_core::curriculum::model::Exemplar;

const BAD_VARS: &str = "a function contract requires one to four distinct variable names";
const BAD_DOMAIN: &str =
    "a function domain requires two exact rationals with low below high for a listed variable";

/// The three frozen shapes: YAML text, JSON text.
const SHAPES: [(&str, &str); 3] = [
    (
        "{kind: function, vars: [x]}",
        r#"{"kind":"function","vars":["x"]}"#,
    ),
    (
        "{kind: function, vars: [x], up_to_constant: true}",
        r#"{"kind":"function","vars":["x"],"up_to_constant":true}"#,
    ),
    (
        r#"{kind: function, vars: [x, y], domain: {x: ["1/4", "11/4"], y: ["1", "2"]}}"#,
        r#"{"kind":"function","vars":["x","y"],"domain":{"x":["1/4","11/4"],"y":["1","2"]}}"#,
    ),
];

fn names(vars: &[&str]) -> Vec<String> {
    vars.iter().map(ToString::to_string).collect()
}

fn domain(entries: &[(&str, &str, &str)]) -> BTreeMap<String, (String, String)> {
    entries
        .iter()
        .map(|(name, low, high)| (name.to_string(), (low.to_string(), high.to_string())))
        .collect()
}

fn spec(vars: &[&str], up_to_constant: bool) -> FunctionSpec {
    FunctionSpec::new(&names(vars), up_to_constant, &BTreeMap::new()).unwrap()
}

fn refusal(vars: &[&str], entries: &[(&str, &str, &str)]) -> &'static str {
    FunctionSpec::new(&names(vars), false, &domain(entries))
        .unwrap_err()
        .reason
}

#[test]
fn the_three_shapes_round_trip_in_yaml_and_json() {
    for (yaml, json) in SHAPES {
        let from_yaml: AnswerContract = serde_norway::from_str(yaml).unwrap();
        let from_json: AnswerContract = serde_json::from_str(json).unwrap();
        assert_eq!(from_yaml, from_json, "{yaml}");
        assert_eq!(serde_json::to_string(&from_yaml).unwrap(), json);
        let yaml_again = serde_norway::to_string(&from_json).unwrap();
        let reread: AnswerContract = serde_norway::from_str(&yaml_again).unwrap();
        assert_eq!(reread, from_json, "{yaml_again}");
    }
}

#[test]
fn the_shapes_read_into_the_variant_fields() {
    let plain: AnswerContract = serde_json::from_str(SHAPES[0].1).unwrap();
    assert_eq!(
        plain,
        AnswerContract::Function {
            vars: names(&["x"]),
            up_to_constant: false,
            domain: BTreeMap::new(),
        }
    );
    let full: AnswerContract = serde_json::from_str(SHAPES[2].1).unwrap();
    assert_eq!(
        full,
        AnswerContract::Function {
            vars: names(&["x", "y"]),
            up_to_constant: false,
            domain: domain(&[("x", "1/4", "11/4"), ("y", "1", "2")]),
        }
    );
}

#[test]
fn an_unknown_key_or_a_bad_field_is_a_load_error() {
    for doc in [
        "{kind: function, vars: [x], points: 8}",
        "{kind: function}",
        "{kind: function, vars: []}",
        "{kind: function, vars: [x, x]}",
        "{kind: function, vars: [x], domain: {y: ['1', '2']}}",
        "{kind: function, vars: [x], domain: {x: ['2', '1']}}",
        "{kind: function, vars: [x], domain: {x: ['1']}}",
    ] {
        assert!(
            serde_norway::from_str::<AnswerContract>(doc).is_err(),
            "{doc}"
        );
    }
    assert!(
        serde_json::from_str::<AnswerContract>(r#"{"kind":"function","vars":["x"],"points":8}"#)
            .is_err()
    );
}

#[test]
fn an_exemplar_with_a_function_contract_loads() {
    let exemplar: Exemplar = serde_norway::from_str(
        "problem: Differentiate $x^3 + x$.\n\
         answer_contract: {kind: function, vars: [x]}\n\
         answer: 3x^2 + 1\n",
    )
    .unwrap();
    let contract = exemplar.answer_contract.unwrap();
    assert!(contract.validate().is_ok());
    assert!(
        matches!(contract, AnswerContract::Function { ref vars, .. } if vars == &names(&["x"]))
    );
}

#[test]
fn function_is_a_legal_multipart_part_and_ordered_list_member() {
    let multipart: AnswerContract = serde_json::from_str(
        r#"{"kind":"multipart","parts":[{"name":"fx","contract":{"kind":"function","vars":["x","y"]}},{"name":"fy","contract":{"kind":"function","vars":["x","y"]}}]}"#,
    )
    .unwrap();
    assert!(
        matches!(multipart, AnswerContract::Multipart { ref parts } if parts.iter().all(|AnswerPart { contract, .. }| matches!(contract, AnswerContract::Function { .. })))
    );
    let member = r#"{"kind":"function","vars":["x"]}"#;
    let ordered = format!(r#"{{"kind":"list","ordered":true,"member":{member}}}"#);
    assert!(serde_json::from_str::<AnswerContract>(&ordered).is_ok());
    // An unordered list compares canonical forms, and a formula has many forms.
    let unordered = format!(r#"{{"kind":"list","ordered":false,"member":{member}}}"#);
    assert!(serde_json::from_str::<AnswerContract>(&unordered).is_err());
    // A part with a bad variable list refuses the whole contract.
    assert!(
        serde_json::from_str::<AnswerContract>(
            r#"{"kind":"multipart","parts":[{"name":"f","contract":{"kind":"function","vars":["e"]}}]}"#
        )
        .is_err()
    );
}

#[test]
fn the_variable_list_refusals() {
    for vars in [
        &[][..],
        &["x", "y", "z", "t", "u"],
        &["x", "x"],
        &["x", "y", "x"],
        &["e"],
        &["pi"],
        &["xy"],
        &["2"],
        &["x+1"],
        &["sin"],
        &[""],
        &[" x"],
    ] {
        assert_eq!(refusal(vars, &[]), BAD_VARS, "{vars:?}");
    }
    for vars in [
        &["x"][..],
        &["x", "y"],
        &["x", "y", "t"],
        &["theta"],
        &["C"],
    ] {
        assert!(
            FunctionSpec::new(&names(vars), false, &BTreeMap::new()).is_ok(),
            "{vars:?}"
        );
    }
}

#[test]
fn the_domain_refusals() {
    for entries in [
        &[("y", "1", "2")][..],
        &[("x", "1", "1")],
        &[("x", "2", "1")],
        &[("x", "sqrt(2)", "2")],
        &[("x", "1", "pi")],
        &[("x", "a", "2")],
        &[("x", "", "2")],
        &[("x", "1", "(2, 3)")],
        &[("x", "1", "1 + 1/10^400")],
    ] {
        assert_eq!(refusal(&["x"], entries), BAD_DOMAIN, "{entries:?}");
    }
    let accepted =
        FunctionSpec::new(&names(&["x", "y"]), false, &domain(&[("y", "-1.5", "6/4")])).unwrap();
    assert_eq!(accepted.domain, vec![(0.25, 2.75), (-1.5, 1.5)]);
}

#[test]
fn the_constants_of_the_frozen_interface() {
    assert_eq!(SAMPLE_FRACTIONS.len(), 8);
    assert!(
        SAMPLE_FRACTIONS
            .iter()
            .zip(0_u32..)
            .all(|(fraction, index)| *fraction == (2 * index + 1, 16))
    );
    assert_eq!(MIN_FINITE_POINTS, 6);
    assert!((TOLERANCE - 1e-9).abs() < f64::EPSILON);
    assert_eq!(CONSTANT_NAMES, ["C", "c", "K", "k"]);
    assert_eq!(DEFAULT_DOMAIN, ("1/4", "11/4"));
}

#[test]
#[allow(clippy::float_cmp)]
fn the_default_sample_points_of_one_variable() {
    let points = spec(&["x"], false).sample_points();
    let values: Vec<f64> = points.iter().map(|env| env["x"]).collect();
    // Each value is a dyadic rational, so the comparison is exact.
    assert_eq!(
        values,
        [
            0.40625, 0.71875, 1.03125, 1.34375, 1.65625, 1.96875, 2.28125, 2.59375
        ]
    );
    assert!(points.iter().all(|env| env.len() == 1));
}

#[test]
#[allow(clippy::float_cmp)]
fn no_two_variables_are_equal_at_one_point() {
    for vars in [&["x", "y"][..], &["x", "y", "t"]] {
        let points = spec(vars, false).sample_points();
        assert_eq!(points.len(), 8);
        for env in &points {
            assert_eq!(env.len(), vars.len());
            let mut values: Vec<f64> = env.values().copied().collect();
            values.sort_by(f64::total_cmp);
            values.dedup();
            assert_eq!(values.len(), vars.len(), "{env:?}");
        }
        // Each variable gets each of the 8 fractions one time.
        for name in vars {
            let mut seen: Vec<f64> = points.iter().map(|env| env[*name]).collect();
            seen.sort_by(f64::total_cmp);
            seen.dedup();
            assert_eq!(seen.len(), 8, "{name}");
        }
    }
    // The formula of the pack: variable 1 at point 2 has the fraction (2 * 3 + 1) % 8 = 7.
    let points = spec(&["x", "y"], false).sample_points();
    assert_eq!(points[2]["y"], 0.25 + 2.5 * (15.0 / 16.0));
}

#[test]
#[allow(clippy::float_cmp)]
fn a_domain_moves_the_points_of_its_variable_only() {
    let spec = FunctionSpec::new(&names(&["x", "y"]), false, &domain(&[("x", "6", "9")])).unwrap();
    let points = spec.sample_points();
    assert_eq!(points[0]["x"], 6.0 + 3.0 / 16.0);
    assert_eq!(points[7]["x"], 6.0 + 45.0 / 16.0);
    assert_eq!(points[0]["y"], 0.25 + 2.5 * (3.0 / 16.0));
}

#[test]
#[allow(clippy::float_cmp)]
fn the_constant_names_are_zero_with_up_to_constant_only() {
    let plain = spec(&["x"], false).sample_points();
    assert!(
        plain
            .iter()
            .all(|env| CONSTANT_NAMES.iter().all(|name| !env.contains_key(*name)))
    );
    let points = spec(&["x"], true).sample_points();
    for env in &points {
        assert_eq!(env.len(), 5);
        assert!(CONSTANT_NAMES.iter().all(|name| env[*name] == 0.0));
    }
    // A constant name that is a listed variable keeps its sample value.
    let points = spec(&["x", "C"], true).sample_points();
    for env in &points {
        assert_eq!(env.len(), 5);
        assert!(env["C"] > 0.25);
        assert!(env["c"] == 0.0 && env["K"] == 0.0 && env["k"] == 0.0);
    }
}

#[test]
fn triage_grades_a_function_contract() {
    let contract: AnswerContract = serde_json::from_str(SHAPES[1].1).unwrap();
    assert_eq!(triage_verdict(&contract), TriageVerdict::Grades);
}

#[test]
fn the_key_refusals_that_do_not_need_the_evaluator() {
    let contract: AnswerContract = serde_json::from_str(SHAPES[0].1).unwrap();
    for (key, reason) in [
        ("(1, 2)", "the authored function must be one expression"),
        ("x > 2", "the authored function must be one expression"),
        ("{x, 2}", "the authored function must be one expression"),
        ("y = (x, 2)", "the authored function must be one expression"),
    ] {
        assert_eq!(contract.validate_expected(key).unwrap_err().reason, reason);
    }
    // The reason of the parser goes through with no change.
    let parser_reason = cadus_core::answer::canonical_form("x/sqrt(x^2+9")
        .unwrap_err()
        .reason;
    assert_eq!(
        contract
            .validate_expected("x/sqrt(x^2+9")
            .unwrap_err()
            .reason,
        parser_reason
    );
    // A key that is longer than the input cap is refused before the parser.
    let long_key = "x".repeat(cadus_core::answer::MAX_ANSWER_CHARS + 1);
    assert_eq!(
        contract.validate_expected(&long_key).unwrap_err().reason,
        "the answer is longer than the input cap"
    );
    // A key that is refused gives no verdict.
    assert!(matches!(
        check_contract("(1, 2)", "(1, 2)", contract),
        Outcome::Undecidable(_)
    ));
}

/// The `function` variant moved the documents with no field to one helper.
#[test]
fn each_document_with_no_field_reads_into_its_variant() {
    for (kind, contract) in [
        ("exact", AnswerContract::Exact),
        ("set", AnswerContract::Set),
        ("inequality_union", AnswerContract::InequalityUnion),
        ("reduced_ratio", AnswerContract::ReducedRatio),
        ("ascending_chain", AnswerContract::AscendingChain),
        ("polynomial_relation", AnswerContract::PolynomialRelation),
        ("relation_setup", AnswerContract::RelationSetup),
        ("required_assignment", AnswerContract::RequiredAssignment),
        (
            "required_inequality_notation",
            AnswerContract::RequiredInequalityNotation,
        ),
        ("required_single_power", AnswerContract::RequiredSinglePower),
        (
            "required_normalized_scientific_notation",
            AnswerContract::RequiredNormalizedScientificNotation,
        ),
        (
            "required_simplest_radical",
            AnswerContract::RequiredSimplestRadical,
        ),
        ("none", AnswerContract::None),
    ] {
        let json = format!(r#"{{"kind":"{kind}"}}"#);
        assert_eq!(
            serde_json::from_str::<AnswerContract>(&json).unwrap(),
            contract
        );
        assert_eq!(serde_json::to_string(&contract).unwrap(), json);
        let with_field = format!(r#"{{"kind":"{kind}","vars":["x"]}}"#);
        assert!(serde_json::from_str::<AnswerContract>(&with_field).is_err());
    }
    assert!(
        AnswerContract::ReducedRatio
            .validate_expected("3:4")
            .is_ok()
    );
    assert!(
        AnswerContract::AscendingChain
            .validate_expected("1 < 2 < 3")
            .is_ok()
    );
}
