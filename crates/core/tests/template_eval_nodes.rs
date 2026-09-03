//! The evaluator and the writer on every node kind of the answer tree (D6, V2).
//!
//! The gate tests reach the evaluator through documents. This file reaches it
//! through trees built by hand, so every node kind, every refusal, and every
//! bracket rule of the writer has one literal expectation.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeMap;

use cadus_core::answer::{Ast, Const, IneqOp};
use cadus_core::template::{
    Answer, Bindings, EvalError, Scalar, Value, answer, evaluate, parse_answer_expr, write,
};
use num_bigint::BigInt;
use num_rational::BigRational;

/// An integer literal node.
fn int(value: i64) -> Ast {
    Ast::Integer(BigInt::from(value))
}

/// A fraction literal node.
fn frac(numerator: i64, denominator: i64) -> Ast {
    Ast::Fraction {
        numerator: BigInt::from(numerator),
        denominator: BigInt::from(denominator),
    }
}

/// A variable node.
fn var(name: &str) -> Ast {
    Ast::Var(name.to_string())
}

/// The tree of one answer expression.
fn expr(source: &str) -> Ast {
    parse_answer_expr(source).unwrap_or_else(|e| panic!("{source:?}: {}", e.reason))
}

/// Evaluate one expression with no binding.
fn eval(source: &str) -> Result<Ast, EvalError> {
    evaluate(&expr(source), &Bindings::new())
}

/// Evaluate one expression and write the result.
fn written(source: &str) -> String {
    write(&eval(source).expect("the expression evaluates")).expect("the value writes")
}

/// The bindings of whole-number parameters.
fn bind(pairs: &[(&str, i64)]) -> Bindings {
    pairs
        .iter()
        .map(|(name, value)| {
            (
                (*name).to_string(),
                Value::Num(BigRational::from(BigInt::from(*value))),
            )
        })
        .collect()
}

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

#[test]
fn an_evaluation_only_name_never_reaches_the_answer_string() {
    let call = Ast::Func("gcd".to_string(), vec![var("x"), int(2)]);
    assert_eq!(
        write(&call),
        Err(EvalError::NotNumber {
            func: "an evaluation-only function"
        })
    );
    assert_eq!(eval("gcd(x, 2)"), Err(EvalError::NotNumber { func: "gcd" }));
    assert_eq!(
        eval("floor(x)"),
        Err(EvalError::NotNumber { func: "floor" })
    );
    // `abs` is in the M2 grammar, so a symbolic argument keeps the call.
    assert_eq!(written("abs(x)"), "abs(x)");
    assert_eq!(written("abs(-3)"), "3");
    // A function the evaluator does not own keeps its evaluated arguments.
    assert_eq!(written("sin(1 + 1)"), "sin(2)");
}

#[test]
fn a_call_with_the_wrong_count_of_arguments_is_refused() {
    assert_eq!(
        eval("gcd(4)"),
        Err(EvalError::Arity {
            func: "gcd".to_string(),
            want: 2,
            given: 1,
        })
    );
    assert_eq!(
        eval("floor(1, 2)"),
        Err(EvalError::Arity {
            func: "floor".to_string(),
            want: 1,
            given: 2,
        })
    );
}

#[test]
fn the_ten_functions_compute_exact_values() {
    assert_eq!(written("sqrt(4/9)"), "2/3");
    assert_eq!(written("sqrt(8)"), "sqrt(8)");
    assert_eq!(written("floor(7/2)"), "3");
    assert_eq!(written("ceiling(7/2)"), "4");
    assert_eq!(written("factorial(5)"), "120");
    assert_eq!(written("gcd(12, 18)"), "6");
    assert_eq!(written("lcm(4, 6)"), "12");
    assert_eq!(written("lcm(-4, 6)"), "12");
    assert_eq!(written("lcm(0, 0)"), "0");
    assert_eq!(written("min(1/2, 1/3)"), "1/3");
    assert_eq!(written("max(1/2, 1/3)"), "1/2");
    assert_eq!(written("binomial(5, 2)"), "10");
    assert_eq!(written("binomial(2, 5)"), "0");
    assert_eq!(written("binomial(5, -1)"), "0");
}

#[test]
fn the_function_domains_are_refused_outside_their_bounds() {
    assert_eq!(
        eval("sqrt(-4)"),
        Err(EvalError::NegativeRoot {
            value: "-4".to_string()
        })
    );
    assert_eq!(
        eval("factorial(-1)"),
        Err(EvalError::FactorialRange {
            value: "-1".to_string()
        })
    );
    assert_eq!(
        eval("factorial(1/2)"),
        Err(EvalError::FactorialRange {
            value: "1/2".to_string()
        })
    );
    assert_eq!(
        eval("factorial(201)"),
        Err(EvalError::FactorialRange {
            value: "201".to_string()
        })
    );
    assert_eq!(
        eval("factorial(10**10)"),
        Err(EvalError::FactorialRange {
            value: "10000000000".to_string()
        })
    );
    assert_eq!(
        eval("gcd(1/2, 2)"),
        Err(EvalError::NotWhole { func: "gcd" })
    );
    assert_eq!(
        eval("binomial(-1, 2)"),
        Err(EvalError::Domain {
            func: "binomial",
            value: "-1".to_string()
        })
    );
    // 5,000 steps of the multiplicative walk go past the step bound.
    assert_eq!(eval("binomial(100000, 5000)"), Err(EvalError::TooWide));
    // 4,000 steps stay inside the step bound, and the coefficient goes past
    // the width bound on the way.
    assert_eq!(eval("binomial(9000, 4000)"), Err(EvalError::TooWide));
    // The smaller side of the walk does not fit a machine word.
    assert_eq!(eval("binomial(10**10, 5*10**9)"), Err(EvalError::TooWide));
}

#[test]
fn a_power_and_a_product_stay_inside_the_width_bound() {
    assert_eq!(written("2**0"), "1");
    assert_eq!(written("2**(-2)"), "1/4");
    assert_eq!(written("x**2"), "x**2");
    assert_eq!(eval("0**(-1)"), Err(EvalError::DivideByZero));
    assert_eq!(eval("(10**1000)**5"), Err(EvalError::TooWide));
    assert_eq!(eval("(10**1000)*(10**1000)"), Err(EvalError::TooWide));
    assert_eq!(
        eval("(10**1000)+(10**1000)*(10**1000)"),
        Err(EvalError::TooWide)
    );
    assert_eq!(eval("1/(2-2)"), Err(EvalError::DivideByZero));
    assert_eq!(eval("(1/2)/(3-3)"), Err(EvalError::DivideByZero));
}

#[test]
fn a_fold_drops_its_identity_and_a_zero_factor_wins() {
    assert_eq!(written("0*x"), "0");
    assert_eq!(written("1*x"), "x");
    assert_eq!(written("0 + x"), "x");
    assert_eq!(written("2*x*3"), "6*x");
    assert_eq!(written("x + 1 + 2"), "3 + x");
    assert_eq!(written("0 + 0"), "0");
    assert_eq!(written("-x"), "-x");
    assert_eq!(written("-(2*3)"), "-6");
    assert_eq!(written("x/y"), "x/y");
    assert_eq!(written("sqrt(x)"), "sqrt(x)");
}

#[test]
fn an_answer_that_does_not_canonicalize_is_refused() {
    let no_bindings = Bindings::new();
    let symbolic_over_zero = Ast::Div(Box::new(var("x")), Box::new(int(0)));
    let refused = answer(&symbolic_over_zero, &no_bindings);
    assert!(
        matches!(&refused, Err(EvalError::NotCanonical { text, reason }) if text == "x/0" && reason.reason == "a quotient with a zero divisor"),
        "{refused:?}"
    );
    let fine = answer(&expr("a + 1"), &bind(&[("a", 1)])).expect("the answer canonicalizes");
    assert_eq!(fine.text, "2");
    assert_eq!(
        fine,
        Answer {
            text: "2".to_string(),
            canon: cadus_core::answer::canonical_form("2").expect("2 canonicalizes"),
        }
    );
    let _: BTreeMap<String, Value> = no_bindings;
}

#[test]
fn a_hand_built_call_reaches_every_function_name() {
    let no_bindings = Bindings::new();
    let func = |name: &str, args: Vec<Ast>| Ast::Func(name.to_string(), args);
    let arity = |name: &str, want: usize, given: usize| EvalError::Arity {
        func: name.to_string(),
        want,
        given,
    };
    assert_eq!(
        evaluate(&func("abs", vec![int(1), int(2)]), &no_bindings),
        Err(arity("abs", 1, 2))
    );
    assert_eq!(
        evaluate(&func("sqrt", vec![var("x")]), &no_bindings),
        Err(EvalError::NotNumber { func: "sqrt" })
    );
    assert_eq!(
        evaluate(&func("sqrt", vec![int(4)]), &no_bindings),
        Ok(int(2))
    );
    for name in ["ceiling", "factorial"] {
        assert_eq!(
            evaluate(&func(name, vec![var("x")]), &no_bindings),
            Err(EvalError::NotNumber { func: name })
        );
    }
    assert_eq!(eval("min(1)"), Err(arity("min", 2, 1)));
    assert_eq!(eval("max(1)"), Err(arity("max", 2, 1)));
    assert_eq!(
        evaluate(&Ast::Pow(Box::new(int(2)), i64::MIN), &no_bindings),
        Err(EvalError::TooWide)
    );
}

#[test]
fn an_error_inside_a_node_reaches_the_top() {
    let sources = [
        "-(1/(2-2))",
        "(1/(2-2))/2",
        "2/(1/(2-2))",
        "(1/(2-2))**2",
        "sqrt(1/(2-2))",
        "(1/(2-2), 1)",
        "1 + 1/(2-2)",
        "abs(1/(2-2))",
        "[1/(2-2), 1)",
        "x < 1/(2-2)",
        "y = 1/(2-2)",
        "1 < x < 1/(2-2)",
        "2*(1/(2-2))",
    ];
    for source in sources {
        assert_eq!(eval(source), Err(EvalError::DivideByZero), "{source}");
    }
    let gcd = Ast::Func("gcd".to_string(), vec![var("x"), int(2)]);
    let refused = Err(EvalError::NotNumber {
        func: "an evaluation-only function",
    });
    let trees = [
        Ast::Sqrt(Box::new(gcd.clone())),
        Ast::Pow(Box::new(gcd.clone()), 2),
        Ast::Div(Box::new(gcd.clone()), Box::new(int(1))),
        Ast::Div(Box::new(int(1)), Box::new(gcd.clone())),
        Ast::Neg(Box::new(gcd.clone())),
        Ast::Add(vec![gcd.clone(), int(1)]),
        Ast::Func("sin".to_string(), vec![gcd.clone()]),
        Ast::Tuple(vec![gcd.clone()]),
        Ast::Interval {
            lo: Box::new(gcd.clone()),
            hi: Box::new(int(1)),
            lo_closed: true,
            hi_closed: true,
        },
        Ast::Interval {
            lo: Box::new(int(1)),
            hi: Box::new(gcd.clone()),
            lo_closed: true,
            hi_closed: true,
        },
        Ast::Ineq {
            var: "x".to_string(),
            op: IneqOp::Lt,
            bound: Box::new(gcd.clone()),
        },
        Ast::Assign {
            var: "y".to_string(),
            value: Box::new(gcd.clone()),
        },
        Ast::Chain {
            lo: Box::new(gcd.clone()),
            lo_closed: true,
            var: "x".to_string(),
            hi_closed: true,
            hi: Box::new(int(1)),
        },
        Ast::Chain {
            lo: Box::new(int(1)),
            lo_closed: true,
            var: "x".to_string(),
            hi_closed: true,
            hi: Box::new(gcd.clone()),
        },
    ];
    for tree in trees {
        assert_eq!(write(&tree), refused, "{tree:?}");
    }
    assert_eq!(
        answer(&gcd, &Bindings::new()),
        Err(EvalError::NotNumber { func: "gcd" })
    );
}

#[test]
fn every_whole_number_function_refuses_a_fraction_on_either_side() {
    let cases = [
        ("gcd(2, 1/2)", "gcd"),
        ("gcd(1/2, 2)", "gcd"),
        ("lcm(1/2, 2)", "lcm"),
        ("lcm(2, 1/2)", "lcm"),
        ("binomial(1/2, 2)", "binomial"),
        ("binomial(2, 1/2)", "binomial"),
    ];
    for (source, func) in cases {
        assert_eq!(eval(source), Err(EvalError::NotWhole { func }), "{source}");
    }
    assert_eq!(eval("[1, 1/(2-2))"), Err(EvalError::DivideByZero));
    assert_eq!(eval("1/(2-2) < x < 1"), Err(EvalError::DivideByZero));
    let mut wide = Bindings::new();
    wide.insert(
        "a".to_string(),
        Value::Num(BigRational::from_integer(BigInt::from(2).pow(5000))),
    );
    assert_eq!(evaluate(&var("a"), &wide), Err(EvalError::TooWide));
}
