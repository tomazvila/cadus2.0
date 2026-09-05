//! Part 2 of the `template_eval_nodes` tests. The header of `template_eval_nodes_1.rs` names the sources.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::eval_nodes::*;

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

/// `binomial(n, k)` by the factorial quotient, for the cases the walk computes.
fn binomial_by_factorials(top: u32, bottom: u32) -> BigInt {
    let factorial = |count: u32| (1..=count).fold(BigInt::from(1), |acc, step| acc * step);
    factorial(top) / (factorial(bottom) * factorial(top - bottom))
}

#[test]
fn a_binomial_takes_the_shorter_side_and_stops_at_the_step_bound() {
    assert_eq!(written("binomial(5, 5)"), "1");
    // The walk runs `min(k, n - k)` steps: one step here, not 4,999.
    assert_eq!(written("binomial(5000, 4999)"), "5000");
    assert_eq!(written("binomial(5000, 1)"), "5000");
    // 2,048 steps are inside the bound; 2,049 are past it, and the value the
    // walk refuses would fit the width bound.
    assert_eq!(
        eval("binomial(4096, 2048)"),
        Ok(Ast::Integer(binomial_by_factorials(4096, 2048)))
    );
    assert_eq!(eval("binomial(4098, 2049)"), Err(EvalError::TooWide));
}

/// `2**exponent` as a tree. The grammar caps a literal exponent, so the tree
/// is built by hand.
fn two_to(exponent: i64) -> Ast {
    Ast::Pow(Box::new(int(2)), exponent)
}

/// `binomial(top, 2)` as a tree.
fn choose_two(top: Ast) -> Ast {
    Ast::Func("binomial".to_string(), vec![top, int(2)])
}

#[test]
fn a_binomial_of_exactly_the_width_bound_is_accepted() {
    let none = Bindings::new();
    // n = 2**2048 + 2**1024 gives n(n - 1)/2 = 2**4095 + 2**3072 - 2**1023,
    // which has 4,096 bits: the bound itself and not past it.
    let n = (BigInt::from(1) << 2048) + (BigInt::from(1) << 1024);
    let expected = (&n * (&n - 1)) / 2;
    let top = Ast::Add(vec![two_to(2048), two_to(1024)]);
    assert_eq!(
        evaluate(&choose_two(top), &none),
        Ok(Ast::Integer(expected))
    );
    // A coefficient that passes 4,096 bits without landing on it is refused.
    assert_eq!(
        evaluate(&choose_two(two_to(4094)), &none),
        Err(EvalError::TooWide)
    );
}

#[test]
fn a_power_of_zero_is_one_and_a_power_of_exactly_the_width_bound_is_accepted() {
    let none = Bindings::new();
    assert_eq!(written("0**0"), "1");
    assert_eq!(written("2**0"), "1");
    assert_eq!(written("2**(-2)"), "1/4");
    assert_eq!(
        evaluate(&two_to(4095), &none),
        Ok(Ast::Integer(BigInt::from(1) << 4095))
    );
    assert_eq!(evaluate(&two_to(4096), &none), Err(EvalError::TooWide));
}
