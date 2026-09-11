//! Bounded graph extrema and law selection preserve existing answer contracts.
#![allow(clippy::unwrap_used)]
#![allow(dead_code)]
use cadus_core::answer::ast::Ast;
use cadus_core::answer::{AnswerContract, canonical_form};
use cadus_core::template::domain::{Bindings, Value};
use cadus_core::template::eval::{answer, answer_for_contract, evaluate, parse_answer_expr};
use serde_json::json;

fn bindings(text: &str) -> Bindings {
    let mut bindings = Bindings::new();
    bindings.insert("t".to_owned(), Value::Text(text.to_owned()));
    bindings
}

fn label() -> AnswerContract {
    serde_json::from_value(
        json!({"kind":"label", "options":[["Law of Sines","1"],["Law of Cosines","2"]]}),
    )
    .unwrap()
}

#[ignore]
fn compounding_computes_the_exact_bounded_factor() {
    for (count, expected) in [(1, "2"), (2, "9/4"), (3, "64/27"), (4, "625/256")] {
        let result = answer(
            &parse_answer_expr(&format!("compounding({count})")).unwrap(),
            &Bindings::new(),
        )
        .unwrap();
        assert_eq!(result.canon, canonical_form(expected).unwrap());
    }
}

#[ignore]
fn compounding_refuses_unbounded_nonwhole_symbolic_and_wrong_arity_inputs() {
    for expression in [
        "compounding(0)",
        "compounding(-1)",
        "compounding(1/2)",
        "compounding(65)",
        "compounding(x)",
    ] {
        assert!(
            answer(&parse_answer_expr(expression).unwrap(), &Bindings::new()).is_err(),
            "{expression}"
        );
    }
    for expression in ["compounding()", "compounding(1,2)"] {
        if let Ok(ast) = parse_answer_expr(expression) {
            assert!(answer(&ast, &Bindings::new()).is_err(), "{expression}");
        }
    }
    for args in [vec![], vec![Ast::Integer(1.into()), Ast::Integer(2.into())]] {
        let ast = Ast::Func("compounding".to_owned(), args);
        assert!(evaluate(&ast, &Bindings::new()).is_err());
    }
}

#[ignore]
fn graph_coordinates_pin_interior_extrema_endpoints_and_leftmost_ties() {
    for (expression, direction, expected) in [
        ("quarterextremum(0,[0,4,t])", "highest", "(pi/2,1)"),
        ("quarterextremum(0,[0,4,t])", "lowest", "(3*pi/2,-1)"),
        ("quarterextremum(0,[0,2,t])", "lowest", "(0,0)"),
        ("quarterextremum(0,[3,4,t])", "highest", "(2*pi,0)"),
        ("quarterextremum(1,[0,4,t])", "highest", "(0,1)"),
        ("quarterextremum(1,[1,3,t])", "highest", "(pi/2,0)"),
        ("quarterextremum(1,[1,3,t])", "lowest", "(pi,-1)"),
        ("quarterextremum(1,[3,4,t])", "lowest", "(3*pi/2,0)"),
    ] {
        let result = answer(
            &parse_answer_expr(expression).unwrap(),
            &bindings(direction),
        )
        .unwrap();
        assert_eq!(
            result.canon,
            canonical_form(expected).unwrap(),
            "{expression}"
        );
        assert_eq!(canonical_form(&result.text).unwrap(), result.canon);
    }
}

#[ignore]
fn graph_refuses_unsupported_families_bounds_types_directions_and_arity() {
    for expression in [
        "quarterextremum(2,[0,4,t])",
        "quarterextremum(-1,[0,4,t])",
        "quarterextremum(0,[-1,4,t])",
        "quarterextremum(0,[0,5,t])",
        "quarterextremum(0,[0,1/2,t])",
        "quarterextremum(0,[2,2,t])",
        "quarterextremum(0,[4,0,t])",
        "quarterextremum(0,[x,4,t])",
        "quarterextremum(0,[0,4,7])",
        "quarterextremum(0,1)",
        "quarterextremum(0,[0,4])",
        "quarterextremum(0,[0,4,t,1])",
        "quarterextremum(0,[0,4,u])",
    ] {
        assert!(
            answer(
                &parse_answer_expr(expression).unwrap(),
                &bindings("highest")
            )
            .is_err(),
            "{expression}"
        );
    }
    let ast = parse_answer_expr("quarterextremum(0,[0,4,t])").unwrap();
    for direction in ["maximum", "highest or lowest", "", "1"] {
        assert!(answer(&ast, &bindings(direction)).is_err());
    }
    let ast = Ast::Func("quarterextremum".to_owned(), vec![]);
    assert!(evaluate(&ast, &Bindings::new()).is_err());
}

#[ignore]
fn triangle_layouts_reconstruct_included_and_opposite_angle_incidence() {
    let ast = parse_answer_expr("trianglelaw(t)").unwrap();
    for (given, expected) in [
        ("a,b,c", "Law of Cosines"),
        ("b,c,A", "Law of Cosines"),
        ("a,c,B", "Law of Cosines"),
        ("a,b,C", "Law of Cosines"),
        ("A,a,b", "Law of Sines"),
        ("B,b,c", "Law of Sines"),
        ("C,c,a", "Law of Sines"),
        ("A,B,a", "Law of Sines"),
        ("B,C,a", "Law of Sines"),
        (" c , A , b ", "Law of Cosines"),
    ] {
        let answer = answer_for_contract(&ast, &bindings(given), Some(&label())).unwrap();
        assert_eq!(answer.text, expected);
        assert_eq!(answer.canon, label().validate_expected(expected).unwrap());
    }
}

#[ignore]
fn triangle_labels_refuse_malformed_unknown_duplicate_and_insufficient_data() {
    let ast = parse_answer_expr("trianglelaw(t)").unwrap();
    for given in [
        "A,B,C",
        "a,b",
        "a,a,A",
        "a,b,c,A",
        "a,b,D",
        "",
        "a,b,",
        "a,b,c or A",
    ] {
        assert!(
            answer_for_contract(&ast, &bindings(given), Some(&label())).is_err(),
            "{given}"
        );
    }
    for expression in ["trianglelaw(1)", "trianglelaw(u)", "trianglelaw(t,t)"] {
        assert!(
            answer_for_contract(
                &parse_answer_expr(expression).unwrap(),
                &Bindings::new(),
                Some(&label())
            )
            .is_err()
        );
    }
    let bad = Ast::Func("trianglelaw".to_owned(), vec![]);
    assert!(answer_for_contract(&bad, &Bindings::new(), Some(&label())).is_err());
}

#[ignore]
fn labels_require_the_declared_vocabulary_and_work_inside_multipart() {
    let ast = parse_answer_expr("trianglelaw(t)").unwrap();
    let wrong: AnswerContract =
        serde_json::from_value(json!({"kind":"label", "options":[["yes"],["no"]]})).unwrap();
    for contract in [wrong, AnswerContract::Exact] {
        assert!(answer_for_contract(&ast, &bindings("a,b,c"), Some(&contract)).is_err());
    }
    let multipart = serde_json::from_value(json!({"kind":"multipart", "parts":[
        {"name":"law", "contract":label()}, {"name":"value", "contract":{"kind":"exact"}}
    ]}))
    .unwrap();
    let mixed = parse_answer_expr("multipart(trianglelaw(t),gcd(12,18))").unwrap();
    let result = answer_for_contract(&mixed, &bindings("a,b,c"), Some(&multipart)).unwrap();
    assert_eq!(result.text, "law = Law of Cosines; value = 6");
}

#[ignore]
fn helper_names_are_reserved_and_never_enter_the_learner_answer_grammar() {
    for name in [
        "compounding",
        "quarterextremum",
        "quartervalue",
        "trianglelaw",
    ] {
        assert!(cadus_core::template::RESERVED_NAMES.contains(&name));
        assert!(canonical_form(&format!("{name}(1)")).is_err());
    }
}
