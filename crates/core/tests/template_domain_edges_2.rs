//! Part 2 of the `template_domain_edges` tests. The header of `template_domain_edges_1.rs` names the sources.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::domain_edges::*;

/// A document over `a` in 1..12 with the given statement and constraints.
fn document(statement: &str, constraints: &str, params: &str) -> cadus_core::template::TemplateDoc {
    let body = format!(
        r#"{{"v": 1, "topic_id": "t", "answer_kind": "numeric", "statement": "{statement}",
            "params": {params}, "constraints": {constraints}, "answer_expr": "a", "hints": ["h"]}}"#
    );
    from_body(&body).expect("the body reads")
}

#[test]
fn a_compiled_document_reports_its_domain_its_statement_and_its_draw() {
    let bad = document(
        "Compute {a}.",
        "[]",
        r#"{"a": {"kind": "int", "low": 9, "high": 2}}"#,
    );
    assert_eq!(
        Compiled::new(&bad).expect_err("the domain is empty"),
        InstantiateError::Draw(DrawError::Domain(DomainError::EmptyRange {
            low: 9,
            high: 2
        }))
    );
    let stray = document(
        "Compute {a} }.",
        "[]",
        r#"{"a": {"kind": "int", "low": 1, "high": 12}}"#,
    );
    let compiled = Compiled::new(&stray).expect("the domains read");
    let mut bindings = cadus_core::template::Bindings::new();
    bindings.insert("a".to_string(), num(1));
    assert_eq!(
        compiled
            .instantiate(bindings)
            .expect_err("the brace is stray"),
        InstantiateError::Render(RenderError::StrayBrace {
            index: 12,
            snippet: "}.".to_string(),
        })
    );
    let never = document(
        "Compute {a}.",
        r#"[{"op": "gt", "left": "a", "right": {"lit": 20}}]"#,
        r#"{"a": {"kind": "int", "low": 1, "high": 12}}"#,
    );
    let compiled = Compiled::new(&never).expect("the domains read");
    let mut rng = rng_from_seed(0);
    assert_eq!(
        compiled.draw(&mut rng).expect_err("no tuple satisfies"),
        InstantiateError::Draw(DrawError::NoSatisfyingTuple { attempts: 1_000 })
    );
    let texts = document(
        "Compute {a} {op}.",
        r#"[{"op": "eq", "left": "op", "right": {"lit": 1}}]"#,
        r#"{"a": {"kind": "int", "low": 1, "high": 12}, "op": {"kind": "choice", "values": ["+", "-"]}}"#,
    );
    let compiled = Compiled::new(&texts).expect("the domains read");
    assert_eq!(
        compiled
            .candidates(&mut rng)
            .expect_err("the constraint reads a text"),
        InstantiateError::Draw(DrawError::Constraint(
            cadus_core::template::ConstraintError::NotNumeric {
                name: "op".to_string(),
                text: "+".to_string(),
            }
        ))
    );
}

#[test]
fn the_renderer_refuses_a_stray_brace_and_an_unbound_hole() {
    let mut bindings = cadus_core::template::Bindings::new();
    bindings.insert("a".to_string(), num(1));
    let stray = |index: usize, snippet: &str| RenderError::StrayBrace {
        index,
        snippet: snippet.to_string(),
    };
    assert_eq!(render("a } b", &bindings), Err(stray(2, "} b")));
    assert_eq!(render("{", &bindings), Err(stray(0, "{")));
    assert_eq!(render("{a} {", &bindings), Err(stray(4, "{")));
    assert_eq!(
        render("{b}", &bindings),
        Err(RenderError::Undeclared {
            name: "b".to_string()
        })
    );
    assert_eq!(placeholders("a }"), Err(stray(2, "}")));
    assert_eq!(
        stray_brace("{a} }"),
        Some(StrayBrace {
            index: 4,
            snippet: "}".to_string(),
        })
    );
    let doc = document(
        "Compute {a} }.",
        "[]",
        r#"{"a": {"kind": "int", "low": 1, "high": 12}}"#,
    );
    assert_eq!(doc.statement_names(), Err(stray(12, "}.")));
}

/// The gate spec of a numeric knowledge point with the given exemplar answers.
fn spec_of(answers: &[&str]) -> Vec<Exemplar> {
    answers
        .iter()
        .map(|answer| Exemplar {
            problem: "p".to_string(),
            answer: (*answer).to_string(),
            solution_sketch: None,
        })
        .collect()
}

#[test]
fn the_diagnosis_filter_keeps_a_body_without_a_list_and_a_tag_it_cannot_read() {
    let vocabulary = vec!["known".to_string()];
    let mut bare = json!({"v": 1});
    assert_eq!(
        keep_known_tags(&mut bare, &vocabulary),
        Vec::<String>::new()
    );
    assert_eq!(bare, json!({"v": 1}));
    let mut mixed = json!({"distractors": [
        {"answer": "1"},
        {"answer": "2", "error_tag": " "},
        {"answer": "3", "error_tag": "known"},
        {"answer": "4", "error_tag": "other"}
    ]});
    assert_eq!(
        keep_known_tags(&mut mixed, &vocabulary),
        vec!["other".to_string()]
    );
    assert_eq!(
        mixed,
        json!({"distractors": [
            {"answer": "1"},
            {"answer": "2", "error_tag": " "},
            {"answer": "3", "error_tag": "known"}
        ]})
    );
    let exemplars = spec_of(&["5"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &exemplars,
    };
    let refusal = gate_diagnosis_body("not json", &spec, &vocabulary).expect_err("not JSON");
    assert_eq!(refusal.code, "diagnosis-body");
    assert!(
        refusal
            .message
            .starts_with("the distractor list does not read as a diagnosis document: "),
        "{}",
        refusal.message
    );
}

#[test]
fn the_diagnosis_gate_skips_a_prose_exemplar_and_accepts_two_distinct_distractors() {
    let exemplars = spec_of(&["yes", "5"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &exemplars,
    };
    let distractor = |answer: &str| Distractor {
        answer: answer.to_string(),
        error_tag: "known".to_string(),
        note: Some("A note.".to_string()),
    };
    let doc = DiagnosisDoc {
        v: 1,
        topic_id: "t".to_string(),
        answer_kind: AnswerKind::Numeric,
        distractors: vec![distractor("6"), distractor("7")],
    };
    assert_eq!(gate_diagnosis(&doc, &spec), Ok(()));
}

#[test]
fn the_declared_space_is_the_product_of_the_distinct_counts() {
    let mixed = params(&[
        ("a", int(1, 3)),
        ("b", int(1, 2)),
        (
            "r",
            Domain::Rational {
                num: IntRange { low: 1, high: 2 },
                den: IntRange { low: 1, high: 3 },
            },
        ),
    ]);
    // 1, 1/2, 1/3, 2, and 2/3: five distinct fractions of the six pairs.
    assert_eq!(declared_space(&mixed), Ok(30));
    let mut rng = rng_from_seed(3);
    assert_eq!(
        draw_satisfying(&params(&[("a", int(9, 2))]), &[], &mut rng),
        Err(DrawError::Domain(DomainError::EmptyRange {
            low: 9,
            high: 2
        }))
    );
}

#[test]
fn the_enumeration_accepts_a_space_of_exactly_the_limit() {
    let twelve = params(&[("a", int(1, 12))]);
    let tuples = enumerate(&twelve, 12).expect("twelve tuples are inside a limit of twelve");
    assert_eq!(tuples.len(), 12);
    assert_eq!(
        enumerate(&twelve, 11),
        Err(DomainError::TooLarge {
            name: "a".to_string(),
            count: 12
        })
    );
}

#[test]
fn a_decimal_domain_at_the_scale_bound_writes_nine_places() {
    let domain = Domain::Decimal {
        low: 1,
        high: 2,
        scale: 9,
    };
    let values = domain.values("d").expect("scale nine is inside the bound");
    let texts: Vec<String> = values
        .iter()
        .map(cadus_core::template::Value::canonical_string)
        .collect();
    assert_eq!(texts, ["0.000000001", "0.000000002"]);
}
