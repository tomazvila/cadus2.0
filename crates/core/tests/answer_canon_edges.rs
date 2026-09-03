//! The refusal sites of the canonicalizer, reached one by one (V2, C4).
//!
//! The corpus and the fuzz reach the common paths. This file reaches the
//! bounds: the work budget at every site that charges it, the size bound, the
//! term bound, the factoring bound, and the refusals of a tree built by hand.
//! Every expected value is a literal.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use cadus_core::answer::canon::canon_with_budget;
use cadus_core::answer::{Ast, Canon, Undecidable, canon, canonical_form, normalize, parse};
use num_bigint::BigInt;
use num_rational::BigRational;

/// The tree of one answer.
fn tree(text: &str) -> Ast {
    parse(&normalize(text).source).unwrap_or_else(|e| panic!("{text:?}: {}", e.reason))
}

/// The refusal of one answer.
fn refusal(text: &str) -> &'static str {
    match canonical_form(text) {
        Ok(value) => panic!("{text:?} decided {value:?}"),
        Err(reason) => reason.reason,
    }
}

/// An integer literal node.
fn int(value: i64) -> Ast {
    Ast::Integer(BigInt::from(value))
}

#[test]
fn a_zero_base_with_a_non_positive_exponent_is_refused() {
    assert_eq!(refusal("0**0"), "a zero base with a non-positive exponent");
    assert_eq!(
        refusal("0**(-1)"),
        "a zero base with a non-positive exponent"
    );
    assert_eq!(
        canonical_form("0**2"),
        Ok(Canon::Rational(BigRational::from_integer(BigInt::from(0))))
    );
}

#[test]
fn a_power_of_a_wide_number_goes_past_the_size_bound() {
    assert_eq!(refusal("(2**1000)**1000"), "a number past the size bound");
    assert_eq!(refusal("(2**1000/3)**1000"), "a number past the size bound");
}

#[test]
fn a_hand_built_literal_with_a_zero_denominator_is_refused() {
    let fraction = Ast::Fraction {
        numerator: BigInt::from(1),
        denominator: BigInt::from(0),
    };
    assert_eq!(
        canon(&fraction),
        Err(Undecidable::new("a division by zero"))
    );
    let mixed = Ast::Mixed {
        whole: BigInt::from(1),
        numerator: BigInt::from(1),
        denominator: BigInt::from(0),
    };
    assert_eq!(canon(&mixed), Err(Undecidable::new("a division by zero")));
    // The parser never builds a negative whole part; the reader still reads one.
    let negative = Ast::Mixed {
        whole: BigInt::from(-2),
        numerator: BigInt::from(1),
        denominator: BigInt::from(2),
    };
    assert_eq!(
        canon(&negative),
        Ok(Canon::Rational(BigRational::new(
            BigInt::from(-5),
            BigInt::from(2)
        )))
    );
}

#[test]
fn arithmetic_on_a_labeled_value_is_refused() {
    let labeled = Ast::Assign {
        var: "x".to_string(),
        value: Box::new(int(1)),
    };
    assert_eq!(
        canon(&Ast::Add(vec![labeled, int(1)])),
        Err(Undecidable::new("arithmetic on a labeled value"))
    );
}

#[test]
fn a_hand_built_tree_that_nests_too_deeply_is_refused() {
    let mut deep = int(1);
    for _ in 0..130 {
        deep = Ast::Neg(Box::new(deep));
    }
    assert_eq!(
        canon(&deep),
        Err(Undecidable::new("the answer nests too deeply"))
    );
}

#[test]
fn a_division_by_a_sum_that_cancels_is_a_division_by_zero() {
    assert_eq!(refusal("1/(x-x)"), "a division by zero");
    assert_eq!(refusal("x/(2-2)"), "a division by zero");
}

#[test]
fn a_radicand_wider_than_a_machine_word_is_a_perfect_square_or_a_refusal() {
    let root = BigInt::from(2).pow(65);
    assert_eq!(
        canonical_form("sqrt(2**130)"),
        Ok(Canon::Rational(BigRational::from_integer(root)))
    );
    assert_eq!(
        refusal("sqrt(2**129)"),
        "a radicand past the factoring bound"
    );
}

#[test]
fn a_remainder_past_the_trial_division_limit_is_a_square_or_a_refusal() {
    // 10007 is the first prime above the trial-division limit of 10,000.
    assert_eq!(
        canonical_form("sqrt(100140049)"),
        Ok(Canon::Rational(BigRational::from_integer(BigInt::from(
            10007
        ))))
    );
    // 10007 * 10009: two primes above the limit, and no square factor.
    assert_eq!(
        refusal("sqrt(100160063)"),
        "a radicand past the factoring bound"
    );
}

#[test]
fn an_exponential_keeps_an_argument_the_atom_e_cannot_take() {
    let quotient = canonical_form("exp(1/(x+1))").expect("decides");
    assert_eq!(canonical_form("e**(1/(x+1))"), Ok(quotient.clone()));
    assert!(matches!(quotient, Canon::Poly(_)), "{quotient:?}");
    let wide = canonical_form("exp(2**70)").expect("decides");
    assert_eq!(canonical_form("e**(2**70)"), Ok(wide.clone()));
    assert!(matches!(wide, Canon::Poly(_)), "{wide:?}");
    assert_ne!(Some(wide), canonical_form("e**2").ok());
}

#[test]
fn a_large_budget_reaches_the_size_bound_and_the_term_bound() {
    let budget = 1_000_000;
    assert_eq!(
        canon_with_budget(&tree("(2**1000)**4 * 2**1000"), budget),
        Err(Undecidable::new("a number past the size bound"))
    );
    assert_eq!(
        canon_with_budget(&tree("1/(x/2**1000 + y/3**1000 + z/5**1000)"), budget),
        Err(Undecidable::new("a number past the size bound"))
    );
    let product = "(x+1)*(x**2+1)*(x**4+1)*(x**8+1)*(x**16+1)*(x**32+1)*(x**64+1)*(x**128+1)*(x**256+1)*(x**512+1)";
    assert_eq!(
        canon_with_budget(&tree(product), budget),
        Err(Undecidable::new("the answer goes past the term bound"))
    );
    assert_eq!(refusal(product), "the answer goes past the work bound");
}

/// The answers whose budget the sweep walks.
///
/// Every number is wider than one machine word, so every size check charges
/// the budget and every charging site has one step at which it refuses.
const SWEPT: [&str; 18] = [
    "exp(x + 5/2**70) + e**(y + 5/2**70)",
    "sqrt(3/2**70) + sqrt(3*2**126)",
    "e**(x + 1/2 + 2**70) * e**(1/2 - x)",
    "(2**70*x + 1)**2 - (2**70*x)**2 + sqrt(2**70*6)*sqrt(2**70*6)",
    "(2**70 + 1)*(2**70*x + 3) - 2**70*x**2 + 7/2**70",
    "(2**70*x + 3)/(2**70*x + 3) + (2**70*x + 2**70)/(x + 1)",
    "1/(2**70*x) + 1/(2**70*x + 2**71*y) + 2/(x*(x + 2**70))",
    "sqrt(2**70*2)*sqrt(2**70*3)*sqrt(6) + sqrt(2**70*3)**3 + sqrt(2**140/9)",
    "e**(2**70*x + 5/2)*exp(3*2**70*x)*exp(-2**70*x + 1/2) + e**(2**70)",
    "(2**70/3)**3 + (2**70*x + 1)**(-2) + (x + 2**70)**3 + (2**70)**(-1)",
    "(sqrt(2) + 1)*(sqrt(2) - 1)*2**70 + (sqrt(2)*2**70 + pi*e**2)*(sqrt(3) + 1)",
    "(1/(x + 2**70))*(x + 2**70) + 2**70*0.5 + 2 3/4*2**70 + sin(2**70*x)*ln(x)",
    "1/(2**70*sqrt(2)*(x + 1)) + 1/(sqrt(2)*2**70) * 1/(x + 1)",
    "(2**70*x + 2**70*y)/(2**71*x + 2**71*y) + (6*x**3 - 17*x**2 + 15*x)/(x**4 - 3*x**3)",
    "2**70 - 2**70 + (x - x)*2**70 + 0*sqrt(2**70) + (2**70*x)**0",
    "-(2**70)*(-(2**70)) + (-2**70)**3 + sqrt(2**70*2)/sqrt(2**70*2)",
    "{2**70, 2**70 + 1} + 1",
    "x = 2**70/(x + 2**70) + e**(2**70*x)/e**(2**70*x + 1/3)",
];

/// The refusals a budget sweep produces.
const SWEEP_REFUSALS: [&str; 2] = [
    "the answer goes past the work bound",
    "arithmetic on a collection",
];

#[test]
fn every_budget_site_refuses_at_its_own_step_and_the_full_budget_decides() {
    for text in SWEPT {
        let ast = tree(text);
        let full = canon(&ast);
        let mut steps = 0;
        loop {
            match canon_with_budget(&ast, steps) {
                Ok(value) => {
                    assert_eq!(Ok(value), full, "{text:?} at {steps} steps");
                    break;
                }
                Err(reason) => {
                    if full == Err(reason) {
                        break;
                    }
                    assert!(
                        SWEEP_REFUSALS.contains(&reason.reason),
                        "{text:?} at {steps} steps: {}",
                        reason.reason
                    );
                }
            }
            steps += 1;
            assert!(steps < 20_000, "{text:?} never decides");
        }
        assert!(steps > 0, "{text:?} costs nothing");
    }
}

/// `x` to the `k`th power, as a hand-built node.
fn power(base: Ast, exponent: i64) -> Ast {
    Ast::Pow(Box::new(base), exponent)
}

#[test]
fn an_exponent_past_a_machine_word_is_refused_at_every_site() {
    let past = Err(Undecidable::new("an exponent past the size bound"));
    assert_eq!(canon(&power(Ast::Var("x".to_string()), i64::MIN)), past);
    assert_eq!(canon(&power(int(2), i64::MAX)), past);
    assert_eq!(canon(&power(Ast::Var("x".to_string()), i64::MAX)), past);
    // Seven powers of 1,000 take the exponent of `x` past the machine word.
    let tower = "((((((x**1000)**1000)**1000)**1000)**1000)**1000)**1000";
    assert_eq!(canonical_form(tower), past);
    // Two exponents of nine quintillion add to more than the word holds.
    let wide = "(((((x**1000)**1000)**1000)**1000)**1000)**1000";
    assert_eq!(canonical_form(&format!("({wide})**9 * ({wide})**9")), past);
    // The whole part of an exponential is the atom `e`, and its exponent adds.
    assert_eq!(
        canonical_form("e**(x + 1/2 + 9223372036854775807) * e**(1/2 - x)"),
        past
    );
}

#[test]
fn a_sum_that_goes_past_the_term_bound_is_refused() {
    let product = "(a+1)*(b+1)*(c+1)*(f+1)*(g+1)*(h+1)*(k+1)*(m+1)*(n+1)";
    assert!(canon_with_budget(&tree(product), 1_000_000).is_ok());
    assert_eq!(
        canon_with_budget(&tree(&format!("{product} + p")), 1_000_000),
        Err(Undecidable::new("the answer goes past the term bound"))
    );
}

#[test]
fn a_wide_squarefree_radicand_bounds_its_power_and_its_product() {
    let primorial = "2*3*5*7*11*13*17*19*23*29*31*37*41*43*47*53*59*61*67*71*73*79*83";
    assert_eq!(
        refusal(&format!("sqrt({primorial})**100")),
        "a number past the size bound"
    );
    let other = "3*5*7*11*13*17*19*23*29*31*37*41*43*47*53*59*61*67*71*73*79*83*89";
    assert_eq!(
        refusal(&format!("sqrt({primorial})*sqrt({other})")),
        "a radicand past the factoring bound"
    );
    assert_eq!(refusal("(1/2**1000)**5"), "a number past the size bound");
}

#[test]
fn a_range_end_and_a_bound_carry_their_own_refusal() {
    assert_eq!(refusal("[1/(x-x), 2)"), "a division by zero");
    assert_eq!(refusal("[1, 1/(x-x))"), "a division by zero");
    assert_eq!(refusal("x < 1/(y-y)"), "a division by zero");
    assert_eq!(refusal("1/(y-y) < x < 2"), "a division by zero");
}
