//! The evaluator `Ast` -> `f64` of the `function` contract (`answer::evalf`).
//!
//! The tests use the real parser. A tree that the parser cannot write (a zero
//! denominator, an exponent outside the grammar, a very deep tree) is built by hand.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::f64::consts::{E, PI};

use cadus_core::answer::evalf::{Env, eval, free_vars};
use cadus_core::answer::{Ast, IneqOp, normalize, parse};
use num_bigint::BigInt;

const TOLERANCE: f64 = 1e-12;

/// The 8 default sample values of the `function` contract.
const SAMPLES: [f64; 8] = [
    0.40625, 0.71875, 1.03125, 1.34375, 1.65625, 1.96875, 2.28125, 2.59375,
];

fn p(text: &str) -> Ast {
    parse(&normalize(text).source).unwrap_or_else(|error| panic!("{text}: {error}"))
}

fn env(pairs: &[(&str, f64)]) -> Env {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), *value))
        .collect()
}

fn at_x(text: &str, x: f64) -> Option<f64> {
    eval(&p(text), &env(&[("x", x)]))
}

fn int(value: i64) -> Ast {
    Ast::Integer(BigInt::from(value))
}

fn var(name: &str) -> Ast {
    Ast::Var(name.to_string())
}

fn pow(base: Ast, exponent: i64) -> Ast {
    Ast::Pow(Box::new(base), exponent)
}

fn root(base: i64, numerator: i64, denominator: i64) -> Ast {
    Ast::RationalPow {
        base: Box::new(int(base)),
        numerator,
        denominator,
    }
}

fn nested_neg(depth: usize) -> Ast {
    (0..depth).fold(var("x"), |tree, _| Ast::Neg(Box::new(tree)))
}

fn names(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|name| (*name).to_string()).collect()
}

#[track_caller]
fn assert_close(got: Option<f64>, want: f64) {
    let value = got.unwrap_or_else(|| panic!("no value, expected {want}"));
    let scale = want.abs().max(1.0);
    assert!(
        (value - want).abs() <= TOLERANCE * scale,
        "got {value}, expected {want}"
    );
}

// ---------------------------------------------------------------------------
// The examples of the brief
// ---------------------------------------------------------------------------
#[test]
fn the_examples_with_a_value() {
    assert_eq!(at_x("x/sqrt(x^2+9)", 4.0), Some(0.8));
    assert_eq!(at_x("10x(x^2+1)^4", 1.0), Some(160.0));
    assert_eq!(eval(&p("y = t*e^(-t)"), &env(&[("t", 0.0)])), Some(0.0));
    assert_eq!(eval(&p("(-8)^(1/3)"), &Env::new()), Some(-2.0));
    assert_eq!(eval(&p("2 1/2"), &Env::new()), Some(2.5));
    assert_eq!(eval(&p("50%"), &Env::new()), Some(0.5));
    assert_eq!(at_x("log(x)", E), Some(1.0));
    let left = at_x("sec(x)^2", 1.0).unwrap();
    let right = at_x("1 + tan(x)^2", 1.0).unwrap();
    assert!((left - right).abs() < TOLERANCE);
}

#[test]
fn the_examples_with_no_value() {
    assert_eq!(at_x("ln(x)", -1.0), None);
    assert_eq!(at_x("1/(x-1)", 1.0), None);
    for text in [
        "x + y", "x*y", "-y", "y/x", "x/y", "y^2", "y^(1/2)", "sqrt(y)",
    ] {
        assert_eq!(at_x(text, 1.0), None, "{text}");
    }
    assert_eq!(eval(&p("(-4)^(1/2)"), &Env::new()), None);
    assert_eq!(eval(&p("e^(1000)"), &Env::new()), None);
    assert_eq!(eval(&p("(1, 2)"), &Env::new()), None);
    assert_eq!(eval(&p("{2, 3}"), &Env::new()), None);
    assert_eq!(at_x("x < 3", 1.0), None);
    assert_eq!(eval(&pow(int(0), -1), &Env::new()), None);
    if let Ok(tree) = parse(&normalize("0^(-1)").source) {
        assert_eq!(eval(&tree, &Env::new()), None);
    }
}

#[test]
fn the_examples_of_free_vars() {
    assert_eq!(free_vars(&p("y = x^2/2 + C")), names(&["x", "C"]));
    assert_eq!(free_vars(&p("2pi + e")), names(&[]));
}

// ---------------------------------------------------------------------------
// Each variant and each function
// ---------------------------------------------------------------------------
#[test]
fn each_scalar_variant_has_its_value() {
    let table: [(&str, f64); 16] = [
        ("42", 42.0),
        ("0.75", 0.75),
        ("-0.1", -0.1),
        ("3/4", 0.75),
        ("-2 1/2", -2.5),
        ("x", 1.5),
        ("pi", PI),
        ("e", E),
        ("sqrt(x + 0.75)", 1.5),
        ("x^3", 3.375),
        ("x^(-2)", 1.0 / 2.25),
        ("16^(3/4)", 8.0),
        ("-x", -1.5),
        ("x + 2 + pi", 3.5 + PI),
        ("2x*x", 4.5),
        ("f = cos(x)/x", 1.5_f64.cos() / 1.5),
    ];
    for (text, want) in table {
        assert_close(at_x(text, 1.5), want);
    }
}

#[test]
fn each_function_name_agrees_with_std() {
    let x = 0.5_f64;
    let table: [(&str, f64); 17] = [
        ("sqrt", x.sqrt()),
        ("sin", x.sin()),
        ("cos", x.cos()),
        ("tan", x.tan()),
        ("sec", x.cos().recip()),
        ("csc", x.sin().recip()),
        ("cot", x.tan().recip()),
        ("asin", x.asin()),
        ("acos", x.acos()),
        ("atan", x.atan()),
        ("sinh", x.sinh()),
        ("cosh", x.cosh()),
        ("tanh", x.tanh()),
        ("exp", x.exp()),
        ("ln", x.ln()),
        ("log", x.ln()),
        ("abs", x),
    ];
    for (name, want) in table {
        assert_close(at_x(&format!("{name}(x)"), x), want);
        let by_hand = Ast::Func(name.to_string(), vec![var("x")]);
        assert_close(eval(&by_hand, &env(&[("x", x)])), want);
    }
    assert_close(at_x("abs(x)", -3.0), 3.0);
}

#[test]
fn a_function_outside_its_domain_has_no_value() {
    for (text, x) in [
        ("sqrt(x)", -1.0),
        ("ln(x)", 0.0),
        ("log(x)", -2.0),
        ("asin(x)", 1.5),
        ("acos(x)", -1.5),
        ("csc(x)", 0.0),
        ("cot(x)", 0.0),
        ("exp(x)", 1000.0),
    ] {
        assert_eq!(at_x(text, x), None, "{text} at {x}");
    }
    assert_eq!(eval(&Ast::Sqrt(Box::new(int(-4))), &Env::new()), None);
}

#[test]
fn an_unknown_name_or_a_wrong_argument_count_has_no_value() {
    let call = |name: &str, args: Vec<Ast>| eval(&Ast::Func(name.to_string(), args), &Env::new());
    assert_eq!(call("gamma", vec![int(2)]), None);
    assert_eq!(call("sin", vec![]), None);
    assert_eq!(call("sin", vec![int(1), int(2)]), None);
    assert_eq!(call("sin", vec![var("x")]), None);
}

#[test]
fn a_node_that_is_not_a_scalar_has_no_value() {
    let boxed = |value: i64| Box::new(int(value));
    let trees = [
        Ast::Tuple(vec![int(1), int(2)]),
        Ast::Set(vec![int(2), int(3)]),
        Ast::List(vec![int(2), int(3)]),
        Ast::Interval {
            lo: boxed(0),
            hi: boxed(1),
            lo_closed: true,
            hi_closed: false,
        },
        Ast::Ineq {
            var: "x".to_string(),
            op: IneqOp::Lt,
            bound: boxed(3),
        },
        Ast::Chain {
            lo: boxed(0),
            lo_closed: true,
            var: "x".to_string(),
            hi_closed: true,
            hi: boxed(1),
        },
        Ast::Quantity {
            value: boxed(5),
            unit: "cm",
        },
    ];
    for tree in trees {
        assert_eq!(eval(&tree, &env(&[("x", 0.5)])), None, "{tree:?}");
        let sum = Ast::Add(vec![int(1), tree]);
        assert_eq!(eval(&sum, &env(&[("x", 0.5)])), None);
    }
}

#[test]
fn a_value_of_the_environment_that_is_not_finite_has_no_value() {
    assert_eq!(at_x("x", f64::NAN), None);
    assert_eq!(at_x("x", f64::INFINITY), None);
    assert_eq!(at_x("0*x", f64::INFINITY), None);
}

// ---------------------------------------------------------------------------
// Literals and powers that only a hand-built tree holds
// ---------------------------------------------------------------------------
#[test]
fn a_zero_denominator_has_no_value() {
    let fraction = Ast::Fraction {
        numerator: BigInt::from(1),
        denominator: BigInt::from(0),
    };
    let mixed = Ast::Mixed {
        whole: BigInt::from(1),
        numerator: BigInt::from(1),
        denominator: BigInt::from(0),
    };
    assert_eq!(eval(&fraction, &Env::new()), None);
    assert_eq!(eval(&mixed, &Env::new()), None);
}

#[test]
fn a_rational_power_gives_the_real_root() {
    let table = [
        (root(32, 1, 5), Some(2.0)),
        (root(-32, 3, 5), Some(-8.0)),
        (root(-8, 2, 3), Some(4.0)),
        (root(9, -1, 2), Some(1.0 / 3.0)),
        (root(7, 3, 1), Some(343.0)),
        (root(-16, 1, 4), None),
        (root(0, -1, 2), None),
        (root(4, 1, 0), None),
        (root(4, 1, -2), None),
    ];
    for (tree, want) in table {
        match want {
            Some(value) => assert_close(eval(&tree, &Env::new()), value),
            None => assert_eq!(eval(&tree, &Env::new()), None, "{tree:?}"),
        }
    }
}

#[test]
fn a_whole_exponent_outside_the_i32_range_keeps_the_sign_rule() {
    let value = |base: i64, exponent: i64| eval(&pow(int(base), exponent), &Env::new());
    assert_eq!(value(1, i64::MAX), Some(1.0));
    assert_eq!(value(-1, i64::MAX), Some(-1.0));
    assert_eq!(value(-1, i64::MIN), Some(1.0));
    assert_eq!(value(2, i64::MAX), None);
    assert_eq!(value(2, i64::MIN), Some(0.0));
}

// ---------------------------------------------------------------------------
// No panic
// ---------------------------------------------------------------------------
#[test]
fn a_huge_literal_does_not_panic() {
    let huge = BigInt::from(10).pow(400);
    assert_eq!(eval(&Ast::Integer(huge.clone()), &Env::new()), None);
    assert_eq!(
        eval(&p(&format!("1{}", "0".repeat(400))), &Env::new()),
        None
    );
    let quotient = Ast::Fraction {
        numerator: huge.clone(),
        denominator: BigInt::from(10).pow(399),
    };
    assert_eq!(eval(&quotient, &Env::new()), Some(10.0));
    let small = Ast::Decimal {
        mantissa: huge,
        scale: 401,
    };
    assert_close(eval(&small, &Env::new()), 0.1);
    let zero = Ast::Decimal {
        mantissa: BigInt::from(7),
        scale: u32::MAX,
    };
    assert_eq!(eval(&zero, &Env::new()), Some(0.0));
}

#[test]
fn a_large_exponent_does_not_panic() {
    assert_eq!(at_x("x^1000", 10.0), None);
    assert_close(at_x("x^1000", 2.0), 2.0_f64.powf(1000.0));
    assert_eq!(at_x("x^1000", 1.0), Some(1.0));
    assert_close(at_x("x^(-1000)", 2.0), 2.0_f64.powi(-1000));
    assert_eq!(at_x("x^(-1000)", 0.0), None);
    assert_eq!(at_x("e^(-1000)", 0.0), Some(0.0));
}

#[test]
fn a_deep_tree_does_not_panic() {
    // The deepest bracket nest that the parser accepts (its depth limit is 96).
    let nest = |count: usize| format!("{}x{}", "(".repeat(count), ")".repeat(count));
    let deepest = (1..=96)
        .rev()
        .find(|count| parse(&normalize(&nest(*count)).source).is_ok())
        .unwrap();
    assert!(deepest >= 10, "deepest nest: {deepest}");
    assert_eq!(at_x(&nest(deepest), 1.25), Some(1.25));
    let point = env(&[("x", 1.25)]);
    assert_eq!(eval(&nested_neg(200), &point), Some(1.25));
    assert_eq!(eval(&nested_neg(201), &point), Some(-1.25));
    assert_eq!(eval(&nested_neg(2000), &point), None);
    assert_eq!(free_vars(&nested_neg(2000)), names(&["x"]));
}

// ---------------------------------------------------------------------------
// free_vars
// ---------------------------------------------------------------------------
#[test]
fn free_vars_reads_each_child() {
    let table: [(&str, &[&str]); 8] = [
        ("sqrt(a) + b^2 - c^(1/2)", &["a", "b", "c"]),
        ("sin(theta)/r", &["r", "theta"]),
        ("(a, b)", &["a", "b"]),
        ("{a, 2}", &["a"]),
        ("[a, b]", &["a", "b"]),
        ("(a, b]", &["a", "b"]),
        ("x < 3", &["x"]),
        ("a <= x < b", &["a", "b", "x"]),
    ];
    for (text, want) in table {
        assert_eq!(free_vars(&p(text)), names(want), "{text}");
    }
    let bound = Ast::Ineq {
        var: "x".to_string(),
        op: IneqOp::Ge,
        bound: Box::new(var("k")),
    };
    assert_eq!(free_vars(&bound), names(&["k", "x"]));
    let quantity = Ast::Quantity {
        value: Box::new(var("w")),
        unit: "cm",
    };
    assert_eq!(free_vars(&quantity), names(&["w"]));
    assert_eq!(free_vars(&p("x = 5")), names(&[]));
}

// ---------------------------------------------------------------------------
// The curriculum table
// ---------------------------------------------------------------------------
type Closure = fn(f64) -> f64;

/// 20 answers of the shipped curriculum, each with a closure that a person wrote.
const CURRICULUM: [(&str, Closure); 20] = [
    ("x^3 - 6x^2 + 12x - 8", |x| (x - 2.0).powi(3)),
    ("(2x - 1)(x^2 - 3)", |x| {
        2.0 * x * x * x - x * x - 6.0 * x + 3.0
    }),
    ("3x^2*sqrt(5x)", |x| 3.0 * 5.0_f64.sqrt() * x.powf(2.5)),
    ("(x - 6)^2", |x| x * x - 12.0 * x + 36.0),
    ("x/sqrt(x^2+9)", |x| x / x.hypot(3.0)),
    ("10x(x^2+1)^4", |x| 10.0 * x * (x * x + 1.0).powf(4.0)),
    ("y = (x + 3)^2 - 4", |x| (x + 1.0) * (x + 5.0)),
    ("2e^(2x)", |x| 2.0 * x.exp() * x.exp()),
    ("3cos(3x)", |x| {
        3.0 * (4.0 * x.cos().powi(3) - 3.0 * x.cos())
    }),
    ("3cosh(3x)", |x| 1.5 * ((3.0 * x).exp() + (-3.0 * x).exp())),
    ("2x cos(x^2)", |x| 2.0 * x * (PI / 2.0 - x * x).sin()),
    ("-2x e^(-x^2)", |x| -2.0 * x / (x * x).exp()),
    ("2x sinh(x^2)", |x| x * ((x * x).exp() - (-x * x).exp())),
    ("2x e^(3x) + 3x^2 e^(3x)", |x| {
        x * (2.0 + 3.0 * x) * x.exp().powi(3)
    }),
    ("5e^x - 2/x", |x| (5.0 * x * x.exp() - 2.0) / x),
    ("1/cosh(x)^2", |x| 1.0 - x.tanh() * x.tanh()),
    ("3sinh(x) + 2x", |x| 1.5 * (x.exp() - (-x).exp()) + x + x),
    ("cosh(2x) + 2x sinh(2x)", |x| {
        let (up, down) = ((2.0 * x).exp(), (-2.0 * x).exp());
        (up + down) / 2.0 + x * (up - down)
    }),
    ("(1 - ln(x))/x^2", |x| (1.0 - x.log2() / E.log2()) / (x * x)),
    ("1/(1 + cos(x))", |x| 0.5 / (x / 2.0).cos().powi(2)),
];

#[test]
fn the_curriculum_table_agrees_with_the_closures() {
    for (text, closure) in CURRICULUM {
        let tree = p(text);
        for x in SAMPLES {
            let got = eval(&tree, &env(&[("x", x)]));
            let value = got.unwrap_or_else(|| panic!("{text} at {x}: no value"));
            let want = closure(x);
            assert!(value.is_finite() && want.is_finite());
            assert!(
                (value - want).abs() <= 1e-9 * want.abs().max(1.0),
                "{text} at {x}: got {value}, expected {want}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------
#[test]
fn two_calls_give_equal_bits() {
    for (text, _) in CURRICULUM {
        let tree = p(text);
        for x in SAMPLES {
            let point = env(&[("x", x)]);
            let first = eval(&tree, &point).map(f64::to_bits);
            let second = eval(&tree.clone(), &point.clone()).map(f64::to_bits);
            assert_eq!(first, second, "{text} at {x}");
            assert!(first.is_some());
        }
    }
    assert_eq!(free_vars(&p("a*b + c")), free_vars(&p("a*b + c")));
}
