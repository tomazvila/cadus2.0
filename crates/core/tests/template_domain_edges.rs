//! The refusal sites of the domains, the draws, the renderer, the document,
//! and the diagnosis gate, reached one by one (D6, C4).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use cadus_core::curriculum::{AnswerKind, Exemplar};
use cadus_core::template::{
    Cmp, Compiled, Constraint, DiagnosisDoc, Distractor, Domain, DomainError, DrawError, DrawPlan,
    GateSpec, InstantiateError, IntRange, Params, RenderError, Scalar, StrayBrace, Term, Value,
    below, candidates, declared_space, draw_bindings, draw_satisfying, enumerate, from_body,
    gate_diagnosis, gate_diagnosis_body, keep_known_tags, literal_to_rational, placeholders,
    render, rng_from_seed, space_size, stray_brace, walk_satisfying,
};
use num_bigint::BigInt;
use num_rational::BigRational;
use serde_json::json;

/// An int domain.
fn int(low: i64, high: i64) -> Domain {
    Domain::Int { low, high }
}

/// A parameter set.
fn params(entries: &[(&str, Domain)]) -> Params {
    entries
        .iter()
        .map(|(name, domain)| ((*name).to_string(), domain.clone()))
        .collect()
}

/// The constraint that reads the text choice `op` as a number.
fn text_as_number() -> Constraint {
    Constraint {
        op: Cmp::Eq,
        left: Term::Param("op".to_string()),
        right: Term::Lit(BigRational::from_integer(BigInt::from(1))),
    }
}

/// The choice of two operator texts.
fn operators() -> Domain {
    Domain::Choice {
        values: vec![Scalar::Text("+".to_string()), Scalar::Text("-".to_string())],
    }
}

/// The error a text choice under a numeric constraint raises.
fn not_numeric() -> DomainError {
    DomainError::Constraint(cadus_core::template::ConstraintError::NotNumeric {
        name: "op".to_string(),
        text: "+".to_string(),
    })
}

/// The whole number `value`.
fn num(value: i64) -> Value {
    Value::Num(BigRational::from_integer(BigInt::from(value)))
}

#[test]
fn a_scalar_has_a_text_and_a_rational() {
    assert_eq!(Scalar::Int(3).text(), "3");
    assert_eq!(
        Scalar::Int(3).rational(),
        Some(BigRational::from_integer(BigInt::from(3)))
    );
    assert_eq!(
        Scalar::Text("1.5".to_string()).rational(),
        Some(BigRational::new(BigInt::from(3), BigInt::from(2)))
    );
    assert_eq!(Scalar::Text("x".to_string()).rational(), None);
}

#[test]
fn a_literal_with_a_zero_or_a_symbolic_side_is_refused() {
    assert_eq!(literal_to_rational("1/0"), None);
    assert_eq!(literal_to_rational("x/2"), None);
    assert_eq!(literal_to_rational("1/y"), None);
    assert_eq!(
        literal_to_rational("3/4"),
        Some(BigRational::new(BigInt::from(3), BigInt::from(4)))
    );
}

#[test]
fn a_number_and_a_text_never_compare_equal_and_a_number_sorts_first() {
    let plus = Value::Text("+".to_string());
    assert_ne!(num(1), Value::Text("1".to_string()));
    assert!(num(1) < plus);
    assert!(plus > num(1));
    assert!(plus < Value::Text("-".to_string()));
    assert_eq!(plus, Value::Text("+".to_string()));
}

#[test]
fn the_empty_choice_and_the_zero_scale() {
    let empty = Domain::Choice { values: vec![] };
    assert_eq!(empty.size("a"), Err(DomainError::EmptyChoice));
    assert_eq!(empty.values("a"), Err(DomainError::EmptyChoice));
    let whole = Domain::Decimal {
        low: -5,
        high: 5,
        scale: 0,
    };
    let values = whole.values("d").expect("the domain lists");
    assert_eq!(values.len(), 11);
    assert_eq!(values[0].canonical_string(), "-5");
    assert_eq!(values[10].canonical_string(), "5");
}

#[test]
fn every_domain_error_through_size_and_values() {
    let too_many_choices = Domain::Choice {
        values: (0..10_001).map(Scalar::Int).collect(),
    };
    let cases: [(Domain, DomainError); 9] = [
        (int(9, 2), DomainError::EmptyRange { low: 9, high: 2 }),
        (
            too_many_choices,
            DomainError::TooLarge {
                name: "a".to_string(),
                count: 10_001,
            },
        ),
        (
            Domain::Rational {
                num: IntRange { low: 1, high: 3 },
                den: IntRange { low: -1, high: 1 },
            },
            DomainError::ZeroDenominator { low: -1, high: 1 },
        ),
        (
            Domain::Rational {
                num: IntRange { low: 5, high: 1 },
                den: IntRange { low: 1, high: 2 },
            },
            DomainError::EmptyRange { low: 5, high: 1 },
        ),
        (
            Domain::Rational {
                num: IntRange { low: 1, high: 2 },
                den: IntRange { low: 5, high: 1 },
            },
            DomainError::EmptyRange { low: 5, high: 1 },
        ),
        (
            Domain::Rational {
                num: IntRange { low: 1, high: 200 },
                den: IntRange { low: 1, high: 100 },
            },
            DomainError::TooLarge {
                name: "a".to_string(),
                count: 20_000,
            },
        ),
        (
            Domain::Decimal {
                low: 1,
                high: 9,
                scale: 10,
            },
            DomainError::DecimalScale { scale: 10 },
        ),
        (
            Domain::Decimal {
                low: 9,
                high: 2,
                scale: 1,
            },
            DomainError::EmptyRange { low: 9, high: 2 },
        ),
        (
            Domain::Decimal {
                low: 1,
                high: 20_000,
                scale: 1,
            },
            DomainError::TooLarge {
                name: "a".to_string(),
                count: 20_000,
            },
        ),
    ];
    for (domain, error) in cases {
        assert_eq!(domain.size("a"), Err(error.clone()), "{domain:?}");
        assert_eq!(domain.values("a"), Err(error), "{domain:?}");
    }
}

#[test]
fn the_space_helpers_report_a_bad_domain_and_a_constraint_they_cannot_decide() {
    let bad = params(&[("a", int(9, 2))]);
    let empty = DomainError::EmptyRange { low: 9, high: 2 };
    assert_eq!(declared_space(&bad), Err(empty.clone()));
    assert_eq!(enumerate(&bad, 4_096), Err(empty.clone()));
    assert_eq!(walk_satisfying(&bad, &[]), Err(empty.clone()));
    assert_eq!(space_size(&bad, &[]), Err(empty.clone()));
    let nine = params(&[("a", int(1, 3)), ("b", int(1, 3))]);
    assert_eq!(
        enumerate(&nine, 4),
        Err(DomainError::TooLarge {
            name: "b".to_string(),
            count: 9,
        })
    );
    // A domain past the exhaustive limit, then a bad one.
    let late = params(&[("a", int(1, 10_000)), ("b", int(9, 2))]);
    assert_eq!(walk_satisfying(&late, &[]), Err(empty));
    // The exhaustive walk and the sampled walk both report an undecidable constraint.
    let small = params(&[("a", int(1, 2)), ("op", operators())]);
    assert_eq!(
        walk_satisfying(&small, &[text_as_number()]),
        Err(not_numeric())
    );
    let large = params(&[("a", int(1, 5_000)), ("op", operators())]);
    assert_eq!(
        walk_satisfying(&large, &[text_as_number()]),
        Err(not_numeric())
    );
}

#[test]
fn the_draw_helpers_and_the_bounded_draw() {
    let mut rng = rng_from_seed(7);
    let plan = params(&[("a", int(1, 3))]);
    let drawn = draw_bindings(&plan, &mut rng).expect("draws");
    assert!(drawn.contains_key("a"));
    assert_eq!(
        draw_bindings(&params(&[("a", int(9, 2))]), &mut rng),
        Err(DomainError::EmptyRange { low: 9, high: 2 })
    );
    let never = Constraint {
        op: Cmp::Gt,
        left: Term::Param("a".to_string()),
        right: Term::Lit(BigRational::from_integer(BigInt::from(5))),
    };
    assert_eq!(
        draw_satisfying(&plan, &[never], &mut rng),
        Err(DrawError::NoSatisfyingTuple { attempts: 1_000 })
    );
    let texts = params(&[("op", operators())]);
    assert_eq!(
        draw_satisfying(&texts, &[text_as_number()], &mut rng),
        Err(DrawError::Constraint(
            cadus_core::template::ConstraintError::NotNumeric {
                name: "op".to_string(),
                text: "+".to_string(),
            }
        ))
    );
    assert_eq!(below(&mut rng, 0), 0);
    assert_eq!(below(&mut rng, 1), 0);
    // A bound just above half the range rejects about one draw in two, so the
    // rejection loop of the multiply-and-shift method runs.
    let bound = (1_u64 << 63) + 1;
    for _ in 0..64 {
        assert!(below(&mut rng, bound) < bound);
    }
}

#[test]
fn the_candidate_stream_reports_a_bad_domain_and_an_undecidable_constraint() {
    let mut rng = rng_from_seed(1);
    let bad = params(&[("a", int(9, 2))]);
    let empty = DomainError::EmptyRange { low: 9, high: 2 };
    assert_eq!(
        candidates(&bad, &[], &mut rng),
        Err(DrawError::Domain(empty.clone()))
    );
    let late = params(&[("a", int(1, 10_000)), ("b", int(9, 2))]);
    assert_eq!(
        candidates(&late, &[], &mut rng),
        Err(DrawError::Domain(empty))
    );
    let refused = DrawError::Constraint(cadus_core::template::ConstraintError::NotNumeric {
        name: "op".to_string(),
        text: "+".to_string(),
    });
    let small = params(&[("a", int(1, 2)), ("op", operators())]);
    assert_eq!(
        candidates(&small, &[text_as_number()], &mut rng),
        Err(refused.clone())
    );
    let large = params(&[("a", int(1, 5_000)), ("op", operators())]);
    assert_eq!(
        candidates(&large, &[text_as_number()], &mut rng),
        Err(refused)
    );
    let plan = DrawPlan::new(&small).expect("the plan builds");
    assert_eq!(plan.declared_space(), 4);
}

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
