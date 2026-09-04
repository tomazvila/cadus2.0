//! The evaluator and the writer on every node kind of the answer tree (D6, V2).
//!
//! The gate tests reach the evaluator through documents. This file reaches it
//! through trees built by hand, so every node kind, every refusal, and every
//! bracket rule of the writer has one literal expectation.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::eval_nodes::*;

#[test]
fn a_decimal_a_fraction_and_a_mixed_number_evaluate_to_exact_rationals() {
    let no_bindings = Bindings::new();
    let decimal = Ast::Decimal {
        mantissa: BigInt::from(15),
        scale: 1,
    };
    assert_eq!(evaluate(&decimal, &no_bindings), Ok(frac(3, 2)));
    assert_eq!(evaluate(&frac(6, 4), &no_bindings), Ok(frac(3, 2)));
    assert_eq!(
        evaluate(&frac(1, 0), &no_bindings),
        Err(EvalError::DivideByZero)
    );
    let mixed = Ast::Mixed {
        whole: BigInt::from(2),
        numerator: BigInt::from(1),
        denominator: BigInt::from(2),
    };
    assert_eq!(evaluate(&mixed, &no_bindings), Ok(frac(5, 2)));
    let broken = Ast::Mixed {
        whole: BigInt::from(2),
        numerator: BigInt::from(1),
        denominator: BigInt::from(0),
    };
    assert_eq!(
        evaluate(&broken, &no_bindings),
        Err(EvalError::DivideByZero)
    );
}

#[test]
fn a_bound_name_takes_its_number_and_a_text_binding_is_refused() {
    let mut bindings = bind(&[("a", 7)]);
    bindings.insert("d".to_string(), Scalar::Text("0.5".to_string()).value());
    bindings.insert("op".to_string(), Value::Text("\\times".to_string()));
    assert_eq!(evaluate(&var("a"), &bindings), Ok(int(7)));
    assert_eq!(evaluate(&var("d"), &bindings), Ok(frac(1, 2)));
    assert_eq!(evaluate(&var("x"), &bindings), Ok(var("x")));
    assert_eq!(
        evaluate(&var("op"), &bindings),
        Err(EvalError::NotNumeric {
            name: "op".to_string(),
            text: "\\times".to_string(),
        })
    );
}

#[test]
fn the_collections_and_the_relations_evaluate_their_members() {
    let bindings = bind(&[("a", 2)]);
    let two = Box::new(int(2));
    let sum = || Ast::Add(vec![var("a"), int(1)]);
    let three = || Box::new(int(3));
    assert_eq!(
        evaluate(&Ast::Tuple(vec![sum(), var("x")]), &bindings),
        Ok(Ast::Tuple(vec![int(3), var("x")]))
    );
    assert_eq!(
        evaluate(&Ast::Set(vec![sum()]), &bindings),
        Ok(Ast::Set(vec![int(3)]))
    );
    assert_eq!(
        evaluate(&Ast::List(vec![sum()]), &bindings),
        Ok(Ast::List(vec![int(3)]))
    );
    let interval = Ast::Interval {
        lo: Box::new(var("a")),
        hi: Box::new(sum()),
        lo_closed: true,
        hi_closed: false,
    };
    assert_eq!(
        evaluate(&interval, &bindings),
        Ok(Ast::Interval {
            lo: two.clone(),
            hi: three(),
            lo_closed: true,
            hi_closed: false,
        })
    );
    let inequality = Ast::Ineq {
        var: "x".to_string(),
        op: IneqOp::Le,
        bound: Box::new(sum()),
    };
    assert_eq!(
        evaluate(&inequality, &bindings),
        Ok(Ast::Ineq {
            var: "x".to_string(),
            op: IneqOp::Le,
            bound: three(),
        })
    );
    let labeled = Ast::Assign {
        var: "y".to_string(),
        value: Box::new(sum()),
    };
    assert_eq!(
        evaluate(&labeled, &bindings),
        Ok(Ast::Assign {
            var: "y".to_string(),
            value: three(),
        })
    );
    let chain = Ast::Chain {
        lo: Box::new(var("a")),
        lo_closed: false,
        var: "x".to_string(),
        hi_closed: true,
        hi: Box::new(sum()),
    };
    assert_eq!(
        evaluate(&chain, &bindings),
        Ok(Ast::Chain {
            lo: two,
            lo_closed: false,
            var: "x".to_string(),
            hi_closed: true,
            hi: three(),
        })
    );
}

#[test]
fn the_writer_spells_every_node_kind_inside_the_grammar() {
    let decimal = Ast::Decimal {
        mantissa: BigInt::from(-15),
        scale: 1,
    };
    assert_eq!(write(&decimal), Ok("-3/2".to_string()));
    let mixed = Ast::Mixed {
        whole: BigInt::from(2),
        numerator: BigInt::from(1),
        denominator: BigInt::from(2),
    };
    assert_eq!(write(&mixed), Ok("2 1/2".to_string()));
    assert_eq!(write(&Ast::Const(Const::Pi)), Ok("pi".to_string()));
    assert_eq!(
        write(&Ast::Sqrt(Box::new(var("x")))),
        Ok("sqrt(x)".to_string())
    );
    assert_eq!(
        write(&Ast::Func("sin".to_string(), vec![var("x"), int(2)])),
        Ok("sin(x, 2)".to_string())
    );
    assert_eq!(
        write(&Ast::Tuple(vec![int(1), int(2)])),
        Ok("(1, 2)".to_string())
    );
    assert_eq!(write(&Ast::Set(vec![int(1)])), Ok("{1}".to_string()));
    assert_eq!(
        write(&Ast::List(vec![int(1), var("x")])),
        Ok("[1, x]".to_string())
    );
    let interval = Ast::Interval {
        lo: Box::new(int(1)),
        hi: Box::new(int(2)),
        lo_closed: true,
        hi_closed: false,
    };
    assert_eq!(write(&interval), Ok("[1, 2)".to_string()));
    let open = Ast::Interval {
        lo: Box::new(int(1)),
        hi: Box::new(int(2)),
        lo_closed: false,
        hi_closed: true,
    };
    assert_eq!(write(&open), Ok("(1, 2]".to_string()));
    let inequality = Ast::Ineq {
        var: "x".to_string(),
        op: IneqOp::Gt,
        bound: Box::new(int(3)),
    };
    assert_eq!(write(&inequality), Ok("x > 3".to_string()));
    let labeled = Ast::Assign {
        var: "y".to_string(),
        value: Box::new(int(4)),
    };
    assert_eq!(write(&labeled), Ok("y = 4".to_string()));
    let chain = Ast::Chain {
        lo: Box::new(int(1)),
        lo_closed: true,
        var: "x".to_string(),
        hi_closed: false,
        hi: Box::new(int(3)),
    };
    assert_eq!(write(&chain), Ok("1 <= x < 3".to_string()));
    let strict = Ast::Chain {
        lo: Box::new(int(1)),
        lo_closed: false,
        var: "x".to_string(),
        hi_closed: true,
        hi: Box::new(int(3)),
    };
    assert_eq!(write(&strict), Ok("1 < x <= 3".to_string()));
}

#[test]
fn the_writer_brackets_by_precedence() {
    // A negative literal is a sum, so a power brackets it (M4 review 1, finding 17).
    assert_eq!(
        write(&Ast::Pow(Box::new(int(-3)), 2)),
        Ok("(-3)**2".to_string())
    );
    let negative_decimal = Ast::Decimal {
        mantissa: BigInt::from(-15),
        scale: 1,
    };
    assert_eq!(
        write(&Ast::Pow(Box::new(negative_decimal), 2)),
        Ok("(-3/2)**2".to_string())
    );
    assert_eq!(
        write(&Ast::Pow(Box::new(frac(-1, 2)), 2)),
        Ok("(-1/2)**2".to_string())
    );
    assert_eq!(
        write(&Ast::Pow(Box::new(var("x")), -2)),
        Ok("x**(-2)".to_string())
    );
    let tower = Ast::Pow(Box::new(Ast::Pow(Box::new(var("x")), 2)), 3);
    assert_eq!(write(&tower), Ok("(x**2)**3".to_string()));
    let negated_sum = Ast::Neg(Box::new(Ast::Add(vec![var("x"), int(1)])));
    assert_eq!(write(&negated_sum), Ok("-(x + 1)".to_string()));
    let quotient = Ast::Div(
        Box::new(Ast::Mul(vec![int(2), var("x")])),
        Box::new(Ast::Add(vec![var("y"), int(1)])),
    );
    assert_eq!(write(&quotient), Ok("2*x/(y + 1)".to_string()));
    let product = Ast::Mul(vec![frac(1, 2), Ast::Add(vec![var("x"), int(1)])]);
    assert_eq!(write(&product), Ok("(1/2)*(x + 1)".to_string()));
    let relation = Ast::Ineq {
        var: "x".to_string(),
        op: IneqOp::Lt,
        bound: Box::new(int(1)),
    };
    assert_eq!(
        write(&Ast::Tuple(vec![relation])),
        Ok("(x < 1)".to_string())
    );
}
