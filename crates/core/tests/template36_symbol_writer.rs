//! Closed symbolic operands preserve formula objectives without free-name gate changes.
#![allow(clippy::unwrap_used)]
use cadus_core::answer::canonical_form;
use cadus_core::template::{Bindings, answer, domain::Value, parse_answer_expr};
use num_rational::BigRational;

fn values(text: &str) -> Bindings {
    Bindings::from([
        ("j".to_owned(), Value::Text(text.to_owned())),
        (
            "a".to_owned(),
            Value::Num(BigRational::from_integer(13.into())),
        ),
    ])
}

#[test]
#[ignore]
fn explicit_symbol_is_an_exact_operand_in_a_computed_expression() {
    for name in ["u", "v", "w", "x", "y", "z", "k", "n", "r", "t", "theta"] {
        let ast = parse_answer_expr("(symbol(j)+2)/a").unwrap();
        let output = answer(&ast, &values(name)).unwrap();
        assert_eq!(
            output.canon,
            canonical_form(&format!("({name}+2)/13")).unwrap()
        );
        assert!(!output.text.contains("symbol"));
    }
}

#[test]
#[ignore]
fn symbol_rejects_reserved_constants_prose_and_expression_injection() {
    let ast = parse_answer_expr("symbol(j)").unwrap();
    for invalid in [
        "e",
        "pi",
        "i",
        "I",
        "E",
        "oo",
        "NaN",
        "sin",
        "a",
        "X",
        "xy",
        "x+1",
        "x=7",
        "x; y",
        "x ",
        " x",
        "\\theta",
        "θ",
        "",
        "__import__",
    ] {
        assert!(
            answer(&ast, &values(invalid)).is_err(),
            "accepted {invalid:?}"
        );
    }
}

#[test]
#[ignore]
fn symbol_requires_one_direct_bound_text_parameter() {
    for invalid in [
        "symbol(1)",
        "symbol(a)",
        "symbol(x)",
        "symbol(j+1)",
        "symbol([j])",
        "symbol((j,a))",
        "symbol(symbol(j))",
    ] {
        let ast = parse_answer_expr(invalid).unwrap();
        assert!(answer(&ast, &values("x")).is_err(), "accepted {invalid}");
    }
    for invalid in ["symbol()", "symbol(j,a)"] {
        if let Ok(ast) = parse_answer_expr(invalid) {
            assert!(answer(&ast, &values("x")).is_err(), "accepted {invalid}");
        }
    }
}

#[test]
#[ignore]
fn explicit_operand_preserves_a_nonzero_symbolic_denominator() {
    let ast = parse_answer_expr("1/(a*symbol(j)+3*symbol(k))").unwrap();
    let mut bindings = values("u");
    bindings.insert("k".to_owned(), Value::Text("v".to_owned()));
    let output = answer(&ast, &bindings).unwrap();
    assert_eq!(output.canon, canonical_form("1/(13*u+3*v)").unwrap());
}
