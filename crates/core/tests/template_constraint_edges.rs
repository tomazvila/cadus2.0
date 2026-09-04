//! The refusal sites of the constraint language, reached one by one (D6, C4).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeSet;

use cadus_core::template::{
    Bindings, Cmp, Constraint, ConstraintError, Term, Value, all_hold, constraint_params,
    eval_term, holds, term_params,
};
use num_bigint::BigInt;
use num_rational::BigRational;

/// A literal term.
fn lit(value: i64) -> Term {
    Term::Lit(BigRational::from_integer(BigInt::from(value)))
}

/// A literal fraction term.
fn ratio(numerator: i64, denominator: i64) -> Term {
    Term::Lit(BigRational::new(
        BigInt::from(numerator),
        BigInt::from(denominator),
    ))
}

/// A parameter term.
fn param(name: &str) -> Term {
    Term::Param(name.to_string())
}

/// One bound whole number.
fn bind(name: &str, value: BigInt) -> Bindings {
    let mut bindings = Bindings::new();
    bindings.insert(
        name.to_string(),
        Value::Num(BigRational::from_integer(value)),
    );
    bindings
}

/// One constraint.
fn constraint(op: Cmp, left: Term, right: Term) -> Constraint {
    Constraint { op, left, right }
}

/// The error of an unbound name.
fn unknown(name: &str) -> Result<BigRational, ConstraintError> {
    Err(ConstraintError::UnknownParam {
        name: name.to_string(),
    })
}

#[test]
fn an_unbound_name_is_refused_inside_every_term_shape() {
    let none = Bindings::new();
    let zz = || param("zz");
    let shapes = [
        zz(),
        Term::Abs(Box::new(zz())),
        Term::Add(vec![lit(1), zz()]),
        Term::Mul(vec![lit(1), zz()]),
        Term::Sub(Box::new(lit(1)), Box::new(zz())),
        Term::Sub(Box::new(zz()), Box::new(lit(1))),
        Term::Mod(Box::new(lit(1)), Box::new(zz())),
        Term::Mod(Box::new(zz()), Box::new(lit(1))),
        Term::DigitSum(Box::new(zz())),
    ];
    for shape in shapes {
        assert_eq!(eval_term(&shape, &none), unknown("zz"), "{shape:?}");
    }
    let relation = constraint(Cmp::Eq, lit(1), zz());
    assert_eq!(
        holds(&relation, &none),
        Err(ConstraintError::UnknownParam {
            name: "zz".to_string()
        })
    );
    assert_eq!(
        all_hold(&[constraint(Cmp::Eq, lit(1), lit(1)), relation], &none),
        Err(ConstraintError::UnknownParam {
            name: "zz".to_string()
        })
    );
    assert_eq!(
        all_hold(&[constraint(Cmp::Eq, lit(1), lit(2))], &none),
        Ok(false)
    );
}

#[test]
fn a_mod_by_zero_and_a_mod_over_a_fraction_are_refused() {
    let none = Bindings::new();
    assert_eq!(
        eval_term(&Term::Mod(Box::new(lit(7)), Box::new(lit(0))), &none),
        Err(ConstraintError::ModByZero)
    );
    let not_whole = |value: &str| {
        Err(ConstraintError::NotWhole {
            op: "mod",
            value: value.to_string(),
        })
    };
    assert_eq!(
        eval_term(&Term::Mod(Box::new(ratio(1, 2)), Box::new(lit(3))), &none),
        not_whole("1/2")
    );
    assert_eq!(
        eval_term(&Term::Mod(Box::new(lit(3)), Box::new(ratio(-3, 10))), &none),
        not_whole("-0.3")
    );
    assert_eq!(
        eval_term(&Term::Mod(Box::new(lit(-7)), Box::new(lit(3))), &none),
        Ok(BigRational::from_integer(BigInt::from(2)))
    );
}

#[test]
fn a_digit_sum_reads_a_whole_number_of_bounded_width() {
    let none = Bindings::new();
    assert_eq!(
        eval_term(&Term::DigitSum(Box::new(lit(1234))), &none),
        Ok(BigRational::from_integer(BigInt::from(10)))
    );
    assert_eq!(
        eval_term(&Term::DigitSum(Box::new(ratio(1, 2))), &none),
        Err(ConstraintError::NotWhole {
            op: "digit_sum",
            value: "1/2".to_string()
        })
    );
    let wide = bind("a", BigInt::from(10).pow(4500));
    assert_eq!(
        eval_term(&Term::DigitSum(Box::new(param("a"))), &wide),
        Err(ConstraintError::TooWide)
    );
}

#[test]
fn the_whole_number_comparisons_refuse_a_fraction_and_a_zero_divisor() {
    let none = Bindings::new();
    let not_whole = |op: &'static str, value: &str| {
        Err(ConstraintError::NotWhole {
            op,
            value: value.to_string(),
        })
    };
    assert_eq!(
        holds(&constraint(Cmp::Divides, lit(0), lit(4)), &none),
        Err(ConstraintError::DividesByZero)
    );
    assert_eq!(
        holds(&constraint(Cmp::Divides, ratio(1, 2), lit(4)), &none),
        not_whole("divides", "1/2")
    );
    assert_eq!(
        holds(&constraint(Cmp::Divides, lit(2), ratio(1, 2)), &none),
        not_whole("divides", "1/2")
    );
    assert_eq!(
        holds(&constraint(Cmp::Coprime, ratio(1, 2), lit(4)), &none),
        not_whole("coprime", "1/2")
    );
    assert_eq!(
        holds(&constraint(Cmp::Coprime, lit(4), ratio(1, 2)), &none),
        not_whole("coprime", "1/2")
    );
    assert_eq!(
        holds(&constraint(Cmp::Carries, ratio(1, 2), lit(4)), &none),
        not_whole("carries", "1/2")
    );
    assert_eq!(
        holds(&constraint(Cmp::Carries, lit(4), ratio(1, 2)), &none),
        not_whole("carries", "1/2")
    );
    assert_eq!(
        holds(&constraint(Cmp::Divides, lit(3), lit(12)), &none),
        Ok(true)
    );
    assert_eq!(
        holds(&constraint(Cmp::Coprime, lit(4), lit(9)), &none),
        Ok(true)
    );
}

#[test]
fn a_carry_over_too_many_digits_is_refused() {
    let wide = bind("a", BigInt::from(10).pow(4500));
    assert_eq!(
        holds(&constraint(Cmp::Carries, param("a"), lit(1)), &wide),
        Err(ConstraintError::TooWide)
    );
    assert_eq!(
        holds(&constraint(Cmp::Carries, lit(1), param("a")), &wide),
        Err(ConstraintError::TooWide)
    );
}

#[test]
fn a_term_past_the_width_bound_is_refused() {
    let edge = bind("a", BigInt::from(2).pow(16_384));
    let inside = bind("a", BigInt::from(2).pow(16_000));
    assert_eq!(
        eval_term(&Term::Add(vec![param("a"), param("a")]), &edge),
        Err(ConstraintError::TooWide)
    );
    assert!(eval_term(&Term::Add(vec![param("a"), param("a")]), &inside).is_ok());
    assert_eq!(
        eval_term(&Term::Mul(vec![param("a"), param("a")]), &inside),
        Err(ConstraintError::TooWide)
    );
    assert_eq!(
        eval_term(&Term::Sub(Box::new(param("a")), Box::new(lit(-1))), &edge),
        Err(ConstraintError::TooWide)
    );
    assert_eq!(
        eval_term(&Term::Mod(Box::new(param("a")), Box::new(lit(3))), &edge),
        Err(ConstraintError::TooWide)
    );
}

#[test]
fn every_comparison_has_its_wire_name() {
    let names = [
        (Cmp::Eq, "eq"),
        (Cmp::Ne, "ne"),
        (Cmp::Lt, "lt"),
        (Cmp::Le, "le"),
        (Cmp::Gt, "gt"),
        (Cmp::Ge, "ge"),
        (Cmp::Divides, "divides"),
        (Cmp::Coprime, "coprime"),
        (Cmp::Carries, "carries"),
    ];
    for (op, name) in names {
        assert_eq!(op.as_str(), name);
    }
}

#[test]
fn the_wire_reader_refuses_a_short_operand_list_and_a_bad_literal() {
    let read = |body: &str| -> String {
        serde_json::from_str::<Constraint>(body)
            .expect_err("the body is refused")
            .to_string()
    };
    let refusals = [
        (
            r#"{"op":"eq","left":{"add":["a"]},"right":"b"}"#,
            "a add term needs at least 2 operands, and it has 1",
        ),
        (
            r#"{"op":"eq","left":{"mul":[]},"right":"b"}"#,
            "a mul term needs at least 2 operands, and it has 0",
        ),
        (
            r#"{"op":"eq","left":{"sub":["a","b","c"]},"right":"b"}"#,
            "a sub term needs exactly 2 operands, and it has 3",
        ),
        (
            r#"{"op":"eq","left":{"mod":["a"]},"right":"b"}"#,
            "a mod term needs exactly 2 operands, and it has 1",
        ),
        (
            r#"{"op":"eq","left":{"lit":"x"},"right":"b"}"#,
            "a lit term needs a whole number, a decimal string, or 'n/d', not \"x\"",
        ),
    ];
    for (body, wanted) in refusals {
        let message = read(body);
        assert!(message.contains(wanted), "{body}: {message}");
    }
    // A bad literal inside every shape.
    let nested = [
        r#"{"add":["a",{"lit":"x"}]}"#,
        r#"{"mul":["a",{"lit":"x"}]}"#,
        r#"{"sub":[{"lit":"x"},"a"]}"#,
        r#"{"sub":["a",{"lit":"x"}]}"#,
        r#"{"mod":["a",{"lit":"x"}]}"#,
        r#"{"abs":{"lit":"x"}}"#,
        r#"{"digit_sum":{"lit":"x"}}"#,
    ];
    for term in nested {
        let body = format!(r#"{{"op":"eq","left":{term},"right":"b"}}"#);
        let message = read(&body);
        assert!(message.contains("a lit term needs"), "{body}: {message}");
    }
}

#[test]
fn every_term_shape_writes_its_wire_form_and_reads_back() {
    let write = |left: Term| -> String {
        serde_json::to_string(&constraint(Cmp::Eq, left, param("b"))).expect("writes")
    };
    let forms = [
        (
            Term::Mul(vec![param("a"), lit(2)]),
            r#"{"op":"eq","left":{"mul":["a",{"lit":2}]},"right":"b"}"#,
        ),
        (
            Term::Sub(Box::new(param("a")), Box::new(param("b"))),
            r#"{"op":"eq","left":{"sub":["a","b"]},"right":"b"}"#,
        ),
        (
            Term::Abs(Box::new(param("a"))),
            r#"{"op":"eq","left":{"abs":"a"},"right":"b"}"#,
        ),
        (
            Term::Mod(Box::new(param("a")), Box::new(lit(3))),
            r#"{"op":"eq","left":{"mod":["a",{"lit":3}]},"right":"b"}"#,
        ),
        (
            Term::DigitSum(Box::new(param("a"))),
            r#"{"op":"eq","left":{"digit_sum":"a"},"right":"b"}"#,
        ),
        (
            Term::Lit(BigRational::from_integer(BigInt::from(2).pow(70))),
            r#"{"op":"eq","left":{"lit":"1180591620717411303424"},"right":"b"}"#,
        ),
        (
            ratio(3, 10),
            r#"{"op":"eq","left":{"lit":"0.3"},"right":"b"}"#,
        ),
        (
            ratio(-3, 10),
            r#"{"op":"eq","left":{"lit":"-0.3"},"right":"b"}"#,
        ),
        (
            ratio(7, 100),
            r#"{"op":"eq","left":{"lit":"0.07"},"right":"b"}"#,
        ),
        (
            ratio(123, 10),
            r#"{"op":"eq","left":{"lit":"12.3"},"right":"b"}"#,
        ),
        (
            ratio(3, 2),
            r#"{"op":"eq","left":{"lit":"3/2"},"right":"b"}"#,
        ),
        (
            ratio(1, 3),
            r#"{"op":"eq","left":{"lit":"1/3"},"right":"b"}"#,
        ),
    ];
    for (left, wanted) in forms {
        let written = write(left.clone());
        assert_eq!(written, wanted);
        let read: Constraint = serde_json::from_str(&written).expect("reads back");
        assert_eq!(read, constraint(Cmp::Eq, left, param("b")));
    }
}

#[test]
fn the_parameter_names_of_every_term_shape() {
    let term = Term::Sub(
        Box::new(param("a")),
        Box::new(Term::Mod(
            Box::new(param("b")),
            Box::new(Term::DigitSum(Box::new(Term::Abs(Box::new(param("c")))))),
        )),
    );
    let names: BTreeSet<String> = ["a", "b", "c"].iter().map(|s| (*s).to_string()).collect();
    assert_eq!(term_params(&term), names);
    assert_eq!(
        constraint_params(&[constraint(Cmp::Eq, term, lit(1))]),
        names
    );
}
