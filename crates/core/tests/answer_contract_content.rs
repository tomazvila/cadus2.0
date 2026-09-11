//! Contracts reach curriculum validation and template instances (D-F1, C5).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use cadus_core::curriculum::{AnswerKind, lint_curriculum, load_raw_curriculum};
use cadus_core::template::{Compiled, GateSpec, from_body, gate, rng_from_seed};
use common::gate::{body_with, doc_of, exemplars};
use common::paths::curriculum_root;
use common::scratch::ScratchTree;

#[test]
fn a_multi_step_template_uses_its_reviewed_contract() {
    let body = body_with(&[
        ("answer_kind", r#""multi-step""#),
        ("answer_contract", r#"{"kind":"exact"}"#),
    ]);
    let doc = doc_of(&body);
    let items = exemplars(&["49", "81"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::MultiStep,
        exemplars: &items,
    };
    gate(&doc, &spec).expect("the complete template gate accepts this representation");
    let instance = Compiled::new(&doc)
        .unwrap()
        .draw(&mut rng_from_seed(2))
        .unwrap();
    assert_eq!(instance.answer_contract, Some(AnswerContract::Exact));
    assert!(
        matches!(check_contract(&instance.answer, &instance.answer, instance.answer_contract.unwrap()), Outcome::Decided(v) if v.correct)
    );
    let mut missing = doc.clone();
    missing.answer_contract = None;
    assert!(gate(&missing, &spec).is_err());
    missing.answer_contract = Some(AnswerContract::None);
    assert!(gate(&missing, &spec).is_err());
}

#[test]
fn a_label_template_computes_a_text_choice_under_its_contract() {
    let body = serde_json::json!({
        "v": 1, "topic_id": "classification", "answer_kind": "expression",
        "answer_contract": {"kind":"label","options":[["yes"],["no"]]},
        "statement": "For case {p}, is the property {c}?",
        "params": {
            "c":{"kind":"choice","values":["yes","no"]},
            "p":{"kind":"int","low":1,"high":6}
        },
        "constraints": [], "answer_expr": "c",
        "solution_sketch": "Read the stated property.",
        "hints": ["Check the definition."], "distractors": [],
        "samples": [
            {"params":{"c":"yes","p":1},"expected":"yes"},
            {"params":{"c":"no","p":6},"expected":"no"}
        ]
    });
    let doc = from_body(&body.to_string()).unwrap();
    let items = exemplars(&["yes", "no"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Expression,
        exemplars: &items,
    };
    gate(&doc, &spec).unwrap();
    let instance = Compiled::new(&doc)
        .unwrap()
        .draw(&mut rng_from_seed(3))
        .unwrap();
    assert!(matches!(instance.answer.as_str(), "yes" | "no"));
    assert!(matches!(
        check_contract(
            &instance.answer,
            &instance.answer,
            instance.answer_contract.unwrap()
        ),
        Outcome::Decided(verdict) if verdict.correct
    ));
}

#[test]
fn a_multipart_template_computes_named_numeric_and_label_parts() {
    let body = serde_json::json!({
        "v": 1, "topic_id": "vertex", "answer_kind": "multi-step",
        "answer_contract": {"kind":"multipart","parts":[
            {"name":"direction","contract":{"kind":"label","options":[["minimum"],["maximum"]]}},
            {"name":"extreme_value","contract":{"kind":"exact"}}
        ]},
        "statement": "For case {a}, identify the {c} and its value.",
        "params": {
            "a":{"kind":"int","low":1,"high":6},
            "c":{"kind":"choice","values":["minimum","maximum"]}
        },
        "constraints": [], "answer_expr": "multipart(c, a)",
        "solution_sketch": "Find the vertex and classify it.",
        "hints": ["Inspect the leading coefficient."], "distractors": [],
        "samples": [
            {"params":{"a":1,"c":"minimum"},"expected":"direction = minimum; extreme_value = 1"},
            {"params":{"a":6,"c":"maximum"},"expected":"direction = maximum; extreme_value = 6"}
        ]
    });
    let doc = from_body(&body.to_string()).unwrap();
    let items = exemplars(&["direction = minimum; extreme_value = 1"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::MultiStep,
        exemplars: &items,
    };
    gate(&doc, &spec).unwrap();
    let instance = Compiled::new(&doc)
        .unwrap()
        .draw(&mut rng_from_seed(4))
        .unwrap();
    assert!(instance.answer.starts_with("direction = "));
    assert!(instance.answer.contains("; extreme_value = "));
    assert!(matches!(
        check_contract(
            &instance.answer,
            &instance.answer,
            instance.answer_contract.unwrap()
        ),
        Outcome::Decided(verdict) if verdict.correct
    ));
}

#[test]
fn a_reduced_ratio_template_writes_the_exact_ratio_notation() {
    let body = serde_json::json!({
        "v": 1, "topic_id": "ratios", "answer_kind": "numeric",
        "answer_contract": {"kind":"reduced_ratio"},
        "statement": "Reduce {a}:40.",
        "params": {"a":{"kind":"choice","values":[2,4,6,8,10,12,14,16,18,20,24,30]}},
        "constraints": [], "answer_expr": "a/40",
        "solution_sketch": "Divide both parts by their greatest common factor.",
        "hints": ["Reduce both parts."], "distractors": [],
        "samples": [
            {"params":{"a":2},"expected":"1:20"},
            {"params":{"a":30},"expected":"3:4"}
        ]
    });
    let doc = from_body(&body.to_string()).unwrap();
    let compiled = Compiled::new(&doc).unwrap();
    let answers: Vec<_> = doc
        .samples
        .iter()
        .map(|sample| compiled.instantiate(sample.bindings()).unwrap().answer)
        .collect();
    assert_eq!(answers, ["1:20", "3:4"]);
}

#[test]
fn a_sign_case_template_covers_all_three_discriminant_outcomes() {
    let body = serde_json::json!({
        "v": 1, "topic_id": "discriminant", "answer_kind": "numeric",
        "statement": "A quadratic has discriminant {p}. How many real roots does it have?",
        "params": {"p":{"kind":"int","low":-4,"high":7}},
        "constraints": [], "answer_expr": "signcase(p, [0, 1, 2])",
        "solution_sketch": "Use the sign of the discriminant.",
        "hints": ["Compare the discriminant with zero."], "distractors": [],
        "samples": [
            {"params":{"p":-4},"expected":0},
            {"params":{"p":0},"expected":1},
            {"params":{"p":7},"expected":2}
        ]
    });
    let doc = from_body(&body.to_string()).unwrap();
    let items = exemplars(&["0", "1", "2"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &items,
    };
    gate(&doc, &spec).unwrap();
}

#[test]
fn a_unit_template_evaluates_its_numeric_expression_before_the_suffix() {
    let body = serde_json::json!({
        "v": 1, "topic_id": "trig-application", "answer_kind": "multi-step",
        "answer_contract": {"kind":"unit","quantity":"length","unit":"m"},
        "statement": "A measured side is five times {p} metres. Find its length.",
        "params": {"p":{"kind":"int","low":1,"high":12}},
        "constraints": [], "answer_expr": "5*p",
        "solution_sketch": "Multiply the scale by the measured side.",
        "hints": ["Keep the unit in the answer."], "distractors": [],
        "samples": [
            {"params":{"p":1},"expected":"5 m"},
            {"params":{"p":12},"expected":"60 m"}
        ]
    });
    let doc = from_body(&body.to_string()).unwrap();
    let items = exemplars(&["5 m", "60 m"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::MultiStep,
        exemplars: &items,
    };
    gate(&doc, &spec).unwrap();
}

#[test]
fn the_fifty_one_inventory_topics_have_explicit_usable_exact_items() {
    let (raw, findings) = load_raw_curriculum(&curriculum_root()).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let mut topics = 0;
    let mut items = 0;
    for entry in raw.topics() {
        let annotated: Vec<_> = entry
            .topic
            .knowledge_points
            .iter()
            .flat_map(|kp| &kp.exemplars)
            .filter(|item| item.answer_contract == Some(AnswerContract::Exact))
            .collect();
        if annotated.is_empty() {
            continue;
        }
        if entry.topic.answer_kind != AnswerKind::MultiStep {
            continue;
        }
        topics += 1;
        for item in annotated {
            assert_eq!(item.answer_contract, Some(AnswerContract::Exact));
            item.canonical_answer().unwrap();
            assert!(
                matches!(check_contract(&item.answer, &item.answer, AnswerContract::Exact), Outcome::Decided(v) if v.correct)
            );
            items += 1;
        }
    }
    assert_eq!(topics, 51);
    assert_eq!(items, 457);
    assert!(lint_curriculum(&curriculum_root()).is_empty());
}

#[test]
fn the_loader_and_lint_refuse_invalid_contracts_and_expected_values() {
    let tree = ScratchTree::new("answer-contract");
    tree.courses(&["demo"]);
    for (contract, expected, code) in [
        ("{kind: approx, decimals: 19}", "1/3", "schema"),
        ("{kind: exact, tolerance: 1}", "1", "schema"),
        ("{kind: exact}", "garbage", "answer_contract"),
        ("{kind: approx, decimals: 2}", "x", "answer_contract"),
    ] {
        tree.unit("demo/01-unit.yaml", "demo", &[("item", &format!(
            "    knowledge_points:\n      - id: kp1\n        name: Point\n        exemplars:\n          - problem: Question\n            answer: '{expected}'\n            answer_contract: {contract}\n"
        ))]);
        let findings = lint_curriculum(tree.root());
        assert!(
            findings.iter().any(|finding| finding.code == code),
            "{findings:?}"
        );
    }
}

#[test]
fn inequality_union_templates_write_only_bounded_validated_relations() {
    for (expression, expected) in [
        ("excludepoint(x,c)", "x < 3 or x > 3"),
        ("lowerbound(x,c)", "x >= 3"),
        ("upperbound(x,c)", "x <= 3"),
    ] {
        let body = serde_json::json!({
            "v":1,"topic_id":"domains","answer_kind":"multi-step",
            "answer_contract":{"kind":"inequality_union"},
            "statement":"State the domain in {x} with boundary {c}.",
            "params":{
                "x":{"kind":"choice","values":["x"]},
                "c":{"kind":"choice","values":[3]}
            },
            "constraints":[],"answer_expr":expression,
            "solution_sketch":"Apply the stated boundary.","hints":["Find the boundary."],
            "distractors":[],"samples":[{"params":{"x":"x","c":3},"expected":expected}]
        });
        let doc = from_body(&body.to_string()).unwrap();
        let item = Compiled::new(&doc)
            .unwrap()
            .instantiate(doc.samples[0].bindings())
            .unwrap();
        assert_eq!(item.answer, expected);
    }

    let unsafe_body = serde_json::json!({
        "v":1,"topic_id":"domains","answer_kind":"multi-step",
        "answer_contract":{"kind":"inequality_union"},
        "statement":"State the domain in {x} with boundary {c}.",
        "params":{
            "x":{"kind":"choice","values":["x or y"]},
            "c":{"kind":"choice","values":[3]}
        },
        "constraints":[],"answer_expr":"lowerbound(x,c)",
        "solution_sketch":"Apply the stated boundary.","hints":["Find the boundary."],
        "distractors":[],"samples":[{"params":{"x":"x or y","c":3},"expected":"x >= 3"}]
    });
    let doc = from_body(&unsafe_body.to_string()).unwrap();
    assert!(
        Compiled::new(&doc)
            .unwrap()
            .instantiate(doc.samples[0].bindings())
            .is_err()
    );
}

#[test]
fn bounded_number_theory_writers_compute_closed_labels() {
    for (expression, value, expected, options) in [
        (
            "divisibilitylabel(a,6)",
            18,
            "yes",
            vec![vec!["yes"], vec!["no"]],
        ),
        (
            "divisibilitylabel(a,6)",
            19,
            "no",
            vec![vec!["yes"], vec!["no"]],
        ),
        (
            "primeclass(a)",
            0,
            "neither",
            vec![vec!["prime"], vec!["composite"], vec!["neither"]],
        ),
        (
            "primeclass(a)",
            2,
            "prime",
            vec![vec!["prime"], vec!["composite"], vec!["neither"]],
        ),
        (
            "primeclass(a)",
            21,
            "composite",
            vec![vec!["prime"], vec!["composite"], vec!["neither"]],
        ),
    ] {
        let body = serde_json::json!({
            "v":1,"topic_id":"number-theory","answer_kind":"expression",
            "answer_contract":{"kind":"label","options":options},
            "statement":"Classify {a}.",
            "params":{"a":{"kind":"choice","values":[value]}},
            "constraints":[],"answer_expr":expression,
            "solution_sketch":"Apply the definition.","hints":["Check the definition."],
            "distractors":[],"samples":[{"params":{"a":value},"expected":expected}]
        });
        let doc = from_body(&body.to_string()).unwrap();
        let item = Compiled::new(&doc)
            .unwrap()
            .instantiate(doc.samples[0].bindings())
            .unwrap();
        assert_eq!(item.answer, expected);
    }
}

#[test]
fn quotient_remainder_writer_passes_the_numeric_gate_under_its_contract() {
    let body = serde_json::json!({
        "v":1,"topic_id":"division","answer_kind":"numeric",
        "answer_contract":{"kind":"quotient_remainder","divisor":5},
        "statement":"Compute ${a} \\div 5$. Give quotient and remainder.",
        "params":{"a":{"kind":"int","low":21,"high":32}},
        "constraints":[],
        "answer_expr":"quotientremainder(floor(a/5),a-5*floor(a/5))",
        "solution_sketch":"Find the greatest multiple of $5$ below ${a}$.",
        "hints":["Use divisor times quotient plus remainder."],"distractors":[],
        "samples":[
            {"params":{"a":21},"expected":"4 R1"},
            {"params":{"a":32},"expected":"6 R2"}
        ]
    });
    let doc = from_body(&body.to_string()).unwrap();
    let items = exemplars(&["9 R2", "6 R2"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &items,
    };
    gate(&doc, &spec).unwrap();
}

#[test]
fn structured_writers_refuse_invalid_domains_and_contract_outputs() {
    for (contract, expression) in [
        (
            serde_json::json!({"kind":"quotient_remainder","divisor":5}),
            "quotientremainder(4,5)",
        ),
        (
            serde_json::json!({"kind":"label","options":[["yes"],["no"]]}),
            "divisibilitylabel(a,0)",
        ),
        (
            serde_json::json!({"kind":"label","options":[["prime"],["composite"],["neither"]]}),
            "primeclass(1000001)",
        ),
        (
            serde_json::json!({"kind":"label","options":[["yes"],["no"]]}),
            "primeclass(a)",
        ),
    ] {
        let body = serde_json::json!({
            "v":1,"topic_id":"negative-control","answer_kind":"numeric",
            "answer_contract":contract,"statement":"Check {a}.",
            "params":{"a":{"kind":"choice","values":[2]}},"constraints":[],
            "answer_expr":expression,"solution_sketch":"Apply the definition.",
            "hints":["Check the allowed domain."],"distractors":[],
            "samples":[{"params":{"a":2},"expected":"yes"}]
        });
        let doc = from_body(&body.to_string()).unwrap();
        assert!(
            Compiled::new(&doc)
                .unwrap()
                .instantiate(doc.samples[0].bindings())
                .is_err()
        );
    }
}
