//! U3 oracle parity: the 2.0 checker against the 1.0 checker (V3, R5, V2).
//!
//! The file holds the learner-notation generators of
//! `docs/reference/checker-1.0-spec.md` section 9.3. It applies every generator
//! to every corpus answer that the 2.0 grammar accepts, and it compares the 2.0
//! verdict with the recorded 1.0 verdict.
//!
//! # Where the 1.0 verdicts come from
//!
//! `crates/core/tests/fixtures/answers/oracle_verdicts_1_0.jsonl` holds one line
//! per generated pair, recorded once with the live 1.0 checker through
//! `scripts/oracle/check_1_0.py`. The test always compares against that file.
//!
//! To dump the generated pairs:
//!
//! ```text
//! CADUS_ORACLE_DUMP=/tmp/pairs.jsonl cargo test -p cadus-core --test answer_oracle
//! ```
//!
//! To prove the file still matches the live 1.0 checker, set
//! `CADUS_ORACLE_PYTHON` and run the file:
//!
//! ```text
//! CADUS_ORACLE_PYTHON=/home/deploy/dev/cadus/.venv/bin/python \
//!     cargo test -p cadus-core --test answer_oracle
//! ```
//!
//! No test of this file carries `#[ignore]`. The live tests read
//! `CADUS_ORACLE_PYTHON` and they print a skip line when it is unset, so
//! `-- --ignored` selects nothing and proves nothing (review finding #12).
//!
//! # Where the rewrite spellings come from
//!
//! The six `rewrite_*` families take their learner text from
//! `crates/core/tests/fixtures/answers/rational_rewrites_1_0.jsonl`, which
//! `scripts/oracle/rewrite_1_0.py` wrote from SymPy through the 1.0 parser
//! (M2 review 3, finding #14). SymPy GENERATES a spelling there; it judges
//! nothing. Every verdict still comes from `check_1_0.py`.
//!
//! # Regenerating the two fixtures, in this order
//!
//! ```text
//! CADUS_REWRITE_REGEN=1 CADUS_ORACLE_PYTHON=/home/deploy/dev/cadus/.venv/bin/python \
//!     cargo test -p cadus-core --test answer_oracle regenerate_the_rewrite_fixture
//! CADUS_ORACLE_RECORD=1 CADUS_ORACLE_PYTHON=/home/deploy/dev/cadus/.venv/bin/python \
//!     cargo test -p cadus-core --test answer_oracle record_the_oracle_verdicts
//! ```
//!
//! The spellings change the generated pair set, so the verdict file follows the
//! spelling file and never the other way around.
//!
//! # The four divergence classes (spec section 9.3)
//!
//! 1. Either side leaves the decidable grammar, so 2.0 refuses a verdict (V2).
//!    Not a parity failure.
//! 2. The authored answer is prose, so the topic is mis-kinded. Not a parity
//!    failure; the list goes to `docs/reference/undecidable-answers.md`.
//! 3. Both sides are inside the grammar and the two verdicts agree, or they
//!    differ for no documented reason. A difference here is a 2.0 bug (R5).
//! 4. Both sides are inside the grammar, the two verdicts differ, and the
//!    difference is one of the documented 2.0 divergences.
//!
//! Every count in this file is a literal. No expected value is read back from
//! the code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::oracle::*;

#[test]
fn a_divergence_leaves_class_3_only_when_a_predicate_names_a_1_0_line() {
    // M2 review 2, findings 10 and 14. The old ladder ended in a substring test
    // for a function name, so every 1.0-True / 2.0-False pair whose text held
    // `sin`, `cos`, `ln`, `log`, or `exp` left class 3 with the reason "a
    // transcendental identity is not simplified". The five `cos 2*x` pairs of
    // the `explicit_multiplication` generator hold no identity: they are the
    // juxtaposed-function-argument parse divergence, and they belong in class 3.
    let parse_divergence = probe_pair("cos 2x", "cos 2*x", "expression_symbolic");
    let one_zero_says_yes = OracleVerdict {
        equivalent: true,
        notation: false,
    };
    assert_eq!(
        documented_reason(&parse_divergence, false, one_zero_says_yes),
        None
    );
    // The same answer with a `sin` in it, and with a `log` in it.
    let with_sin = probe_pair("(4/3)sin 3t", "(4/3)*sin 3*t", "expression_symbolic");
    assert_eq!(documented_reason(&with_sin, false, one_zero_says_yes), None);
    let with_log = probe_pair("log(2x)", "2*log(x)", "expression_symbolic");
    assert_eq!(documented_reason(&with_log, false, one_zero_says_yes), None);
    // The identity that IS the documented narrowing gets no reason here either:
    // no test of the two strings reads a 1.0 `simplify` result. The pair is
    // pinned by its literal verdict in `answer_divergence.rs`.
    let identity = probe_pair("sin(x)**2+cos(x)**2", "1", "expression_symbolic");
    assert_eq!(documented_reason(&identity, false, one_zero_says_yes), None);
    // The one predicate of this branch still names its own pairs: two numbers
    // that 1.0 calls equal at its float tolerance, and 2.0 does not (D6).
    let two_numbers = probe_pair("1/1000", "1/1001", "fraction");
    assert_eq!(
        documented_reason(&two_numbers, false, one_zero_says_yes),
        Some("no float tolerance rung (D6)")
    );
    // A prose answer keeps its own reason, whatever the two verdicts are.
    let prose = probe_pair("yes", "no", "prose_or_words");
    assert_eq!(
        documented_reason(&prose, false, one_zero_says_yes),
        Some("prose is not a value (V2)")
    );
}

#[test]
fn a_numeric_divergence_needs_the_1_0_float_tolerance() {
    // M2 review 2 asked for a specific predicate and not a catch-all. "Both
    // sides are numbers" excused every numeric disagreement; the rung the
    // predicate ports compares two floats at a tolerance, so the predicate
    // compares two floats at that tolerance.
    let one_zero_says_yes = OracleVerdict {
        equivalent: true,
        notation: false,
    };
    // Inside the 1e-6 `evalf` rung (`sympy_check.py:345-352`).
    let inside = probe_pair("1/1000", "1/1001", "fraction");
    assert_eq!(
        documented_reason(&inside, false, one_zero_says_yes),
        Some("no float tolerance rung (D6)")
    );
    let decimal_of_a_fraction = probe_pair("5/12", "0.4166666667", "fraction");
    assert_eq!(
        documented_reason(&decimal_of_a_fraction, false, one_zero_says_yes),
        Some("no float tolerance rung (D6)")
    );
    let decimal_of_a_radical = probe_pair("8*sqrt(2)", "11.31370850", "expression_numeric");
    assert_eq!(
        documented_reason(&decimal_of_a_radical, false, one_zero_says_yes),
        Some("no float tolerance rung (D6)")
    );
    let nested_radical = probe_pair("√(2 + √3)/2", "0.9659258263", "expression_numeric");
    assert_eq!(
        documented_reason(&nested_radical, false, one_zero_says_yes),
        Some("no float tolerance rung (D6)")
    );
    // Outside every rung. Two numbers that differ keep no reason, and the pair
    // stays in class 3 (R5).
    let coarse = probe_pair("2/3", "0.667", "fraction");
    assert_eq!(documented_reason(&coarse, false, one_zero_says_yes), None);
    let far_apart = probe_pair("7329", "7330", "integer");
    assert_eq!(
        documented_reason(&far_apart, false, one_zero_says_yes),
        None
    );
    // Two plain decimals take the tighter 1e-9 rung (`sympy_check.py:172-174`),
    // so a gap inside the 1e-6 rung keeps no reason here.
    let two_decimals = probe_pair("1.0000000", "1.0000005", "decimal");
    assert_eq!(
        documented_reason(&two_decimals, false, one_zero_says_yes),
        None
    );
    // A free symbol leaves the rung: the reader refuses the side, so no reason.
    let with_a_symbol = probe_pair("x/3", "0.3333333333*x", "expression_symbolic");
    assert_eq!(
        documented_reason(&with_a_symbol, false, one_zero_says_yes),
        None
    );
}

#[test]
fn the_harness_reader_reads_a_number_and_refuses_a_name() {
    assert_eq!(numeric_value("1/8"), Some(0.125));
    assert_eq!(numeric_value("-5/6"), Some(-5.0 / 6.0));
    assert_eq!(numeric_value("2*sqrt(4)/4"), Some(1.0));
    assert_eq!(numeric_value("sqrt(2 + sqrt(4))"), Some(2.0));
    assert_eq!(numeric_value("(2 + 4)/3"), Some(2.0));
    assert_eq!(numeric_value("2**3"), Some(8.0));
    assert_eq!(numeric_value("-2**2"), Some(-4.0));
    assert_eq!(numeric_value("2**-1"), Some(0.5));
    assert_eq!(numeric_value("pi"), Some(std::f64::consts::PI));
    assert_eq!(numeric_value("e"), Some(std::f64::consts::E));
    // A juxtaposed name is a product, and a bracket-free radical is a call,
    // because 1.0 parses with `implicit_multiplication_application`
    // (`sympy_check.py:253`): `3pi` is `3*pi` and `2sqrt 2 - 2` is
    // `2*sqrt(2) - 2`. FIXM2i: the old reader refused both, and the float rung
    // then explained none of the 78 `pi` and radical pairs of the
    // `significant_decimal` family.
    assert_eq!(numeric_value("3pi"), Some(3.0 * std::f64::consts::PI));
    assert_eq!(numeric_value("2sqrt(3)"), Some(2.0 * 3.0_f64.sqrt()));
    assert_eq!(numeric_value("2*sqrt 2"), Some(2.0 * 2.0_f64.sqrt()));
    assert_eq!(numeric_value("2(3)"), Some(6.0));
    // A free symbol, an unknown name, an implicit product of a number and a
    // free symbol, an unbalanced bracket, and a division by zero are all
    // refusals.
    assert_eq!(numeric_value("x"), None);
    assert_eq!(numeric_value("2*cos(0)"), None);
    assert_eq!(numeric_value("2x"), None);
    assert_eq!(numeric_value("(1 + 2"), None);
    assert_eq!(numeric_value("1/0"), None);
    // Two numbers that only touch are two answers, not one.
    assert_eq!(numeric_value("1 2"), None);
}

#[test]
fn the_case_flip_family_writes_the_answer_in_upper_case() {
    assert_eq!(
        generate_case_flip(&probe_row("sqrt(2)", "expression_numeric")),
        Some("SQRT(2)".to_string())
    );
    assert_eq!(
        generate_case_flip(&probe_row("2*x + 1", "expression_symbolic")),
        Some("2*X + 1".to_string())
    );
    // An answer with no lower-case letter is no variant.
    assert_eq!(generate_case_flip(&probe_row("7329", "integer")), None);
    assert_eq!(
        generate_case_flip(&probe_row("(4, 17)", "ordered_tuple")),
        None
    );
}

#[test]
fn the_significant_decimal_family_writes_ten_significant_digits() {
    assert_eq!(
        generate_significant_decimal(&probe_row("1/3", "fraction")),
        Some("0.3333333333".to_string())
    );
    assert_eq!(
        generate_significant_decimal(&probe_row("-5/6", "fraction")),
        Some("-0.8333333333".to_string())
    );
    assert_eq!(
        generate_significant_decimal(&probe_row("8*sqrt(2)", "expression_numeric")),
        Some("11.31370850".to_string())
    );
    assert_eq!(
        generate_significant_decimal(&probe_row("√(2 + √3)/2", "expression_numeric")),
        Some("0.9659258263".to_string())
    );
    // FIXM2i widened the gate to every irrational number the grammar holds, so
    // `pi` and `e` join the roots. The old gate read the source for `sqrt(`, and
    // it missed both, and it missed `√` after FIXM2g.
    assert_eq!(
        generate_significant_decimal(&probe_row("2π", "expression_numeric")),
        Some("6.283185307".to_string())
    );
    assert_eq!(
        generate_significant_decimal(&probe_row("$3\\pi$", "expression_numeric")),
        Some("9.424777961".to_string())
    );
    // A rational with an exact decimal, an integer, and a value with a free
    // symbol are all refusals: the pair carries no float rung.
    assert_eq!(
        generate_significant_decimal(&probe_row("1/2", "fraction")),
        None
    );
    assert_eq!(
        generate_significant_decimal(&probe_row("7329", "integer")),
        None
    );
    assert_eq!(
        generate_significant_decimal(&probe_row("sqrt(x)", "expression_symbolic")),
        None
    );
}

#[test]
fn the_product_reorder_family_rotates_the_factors() {
    assert_eq!(
        generate_product_reorder(&probe_row("2*x", "expression_symbolic")),
        Some("x*2".to_string())
    );
    assert_eq!(
        generate_product_reorder(&probe_row("8*sqrt(2)", "expression_numeric")),
        Some("sqrt(2)*8".to_string())
    );
    assert_eq!(
        generate_product_reorder(&probe_row("2*sqrt(3)/3", "expression_numeric")),
        Some("sqrt(3)/3*2".to_string())
    );
    // A sum is no product, and a rotation across its `-` builds another value.
    assert_eq!(
        generate_product_reorder(&probe_row("9*pi - 18", "expression_numeric")),
        None
    );
    assert_eq!(
        generate_product_reorder(&probe_row("x**2", "expression_symbolic")),
        None
    );
}

#[test]
fn the_algebraic_refactor_family_factors_and_multiplies_out() {
    // The pair spec section 9.3 names.
    assert_eq!(
        generate_algebraic_refactor(&probe_row("x**2 - 1", "expression_symbolic")),
        Some("(x - 1)*(x + 1)".to_string())
    );
    assert_eq!(
        generate_algebraic_refactor(&probe_row("4x**2 - 49", "expression_symbolic")),
        Some("(2*x - 7)*(2*x + 7)".to_string())
    );
    // The same step in the other direction: the corpus authors the factored
    // form and the learner multiplies it out.
    assert_eq!(
        generate_algebraic_refactor(&probe_row("(x + 3)(x - 3)", "expression_symbolic")),
        Some("(x + 3)*(x) - (x + 3)*(3)".to_string())
    );
    // The family reads the PRINTED tree, so the juxtaposed `2(x + 3)` of the
    // author reaches the rule as the product `2*(x + 3)` (FIXM2i).
    assert_eq!(
        generate_algebraic_refactor(&probe_row("2(x + 3)(x - 3)", "expression_symbolic")),
        Some("2*(x + 3)*(x) - 2*(x + 3)*(3)".to_string())
    );
    // A function argument is no factor, and a sum of two terms that are not
    // both squares takes neither rule.
    assert_eq!(
        generate_algebraic_refactor(&probe_row("sqrt(1 + x**2)", "expression_symbolic")),
        None
    );
    assert_eq!(
        generate_algebraic_refactor(&probe_row("x**2 - 3", "expression_symbolic")),
        None
    );
    assert_eq!(
        generate_algebraic_refactor(&probe_row("1/(x + 1)", "expression_symbolic")),
        None
    );
}

#[test]
fn the_printer_writes_the_tree_and_not_the_spelling() {
    // FIXM2g leaves every construct in the source as a token, so the printed
    // tree is the only ASCII reading of an answer the builders can trust.
    let printed = |answer: &str| probe_row(answer, "expression_symbolic").printed;
    assert_eq!(printed("\\frac{1}{2}"), "1/2");
    assert_eq!(printed("½"), "1/2");
    assert_eq!(printed("$(-2, 5\\pi/4)$"), "(-2, 5*pi/4)");
    assert_eq!(printed("$(1, \\sqrt 3)$"), "(1, sqrt(3))");
    assert_eq!(printed("36x^2y^2"), "36*x**2*y**2");
    assert_eq!(printed("15√3"), "15*sqrt(3)");
    assert_eq!(printed("x^3 - 6x^2 + 12x - 8"), "x**3 - 6*x**2 + 12*x - 8");
    assert_eq!(printed("1/(x*(x + 1))"), "1/(x*(x + 1))");
    assert_eq!(printed("7.2 x 10^-4"), "7.2*10**(-4)");
    // A mixed number prints as a bracketed sum, because 1.0 reads the bare
    // juxtaposition `2 1/2` as the product `2*(1/2)`.
    assert_eq!(printed("2\\frac{1}{2}"), "(2 + 1/2)");
    assert_eq!(printed("-1 ≤ x ≤ 3"), "-1 <= x <= 3");
    assert_eq!(printed("{1, 3, 5}"), "{1, 3, 5}");
}

#[test]
fn the_rewrite_family_reads_the_committed_sympy_spelling() {
    // The six families take their spelling from the committed fixture, and the
    // fixture holds the `str()` of the SymPy rule. The four rows below are
    // literals of `crates/core/tests/fixtures/answers/rational_rewrites_1_0.jsonl`.
    let row = probe_row("2/x + 1/(x + 1)", "expression_symbolic");
    assert_eq!(
        generate_rewrite_together(&row),
        Some("(3*x + 2)/(x*(x + 1))".to_string())
    );
    assert_eq!(
        generate_rewrite_cancel(&row),
        Some("(3*x + 2)/(x**2 + x)".to_string())
    );
    let radical = probe_row("1/(2√x)", "expression_symbolic");
    assert_eq!(
        generate_rewrite_radsimp(&radical),
        Some("sqrt(x)/(2*x)".to_string())
    );
    // An answer with no denominator and no radical takes no spelling, whatever
    // the fixture holds.
    let whole = probe_row("x**2 - 1", "expression_symbolic");
    assert_eq!(generate_rewrite_factor(&whole), None);
    // A mixed number takes none either: 1.0 reads `3 1/2` as `3*(1/2)`, so the
    // gate keeps the whole row out of the file and out of the family.
    let mixed = probe_row("3 1/2", "fraction");
    assert!(!the_two_checkers_read_the_answer_alike(&mixed));
    assert_eq!(generate_rewrite_together(&mixed), None);
    // A spaced `x` is the times sign in 2.0 and a free symbol in 1.0.
    let times = probe_row("7.2 x 10^-4", "decimal");
    assert!(!the_two_checkers_read_the_answer_alike(&times));
    assert_eq!(generate_rewrite_together(&times), None);
    // The two rows above are the only shapes the gate refuses. A plain rational
    // and a radical both pass it.
    assert!(the_two_checkers_read_the_answer_alike(&row));
    assert!(the_two_checkers_read_the_answer_alike(&radical));
    assert!(the_two_checkers_read_the_answer_alike(&whole));
}

#[test]
fn the_two_rewrite_narrowings_need_the_recorded_sympy_evidence() {
    let one_zero_says_yes = OracleVerdict {
        equivalent: true,
        notation: false,
    };
    // A denominator that needs a polynomial GCD. The evidence line of the
    // fixture says `cancel(expected - learner) == 0`, and the two canonical
    // forms differ in a denominator.
    let gcd = probe_pair(
        "-4(x + 1)/(x - 1)^3",
        "-4/(x - 1)**2 - 8/(x - 1)**3",
        "expression_symbolic",
    );
    assert_eq!(
        documented_reason(&gcd, false, one_zero_says_yes),
        Some("no polynomial GCD (V1 narrowing)")
    );
    // A radical the rewrite moves out of the denominator.
    let radical = probe_pair("1/(2√x)", "sqrt(x)/(2*x)", "expression_symbolic");
    assert_eq!(
        documented_reason(&radical, false, one_zero_says_yes),
        Some("no radical rationalization (V1 narrowing)")
    );
    // The SAME two answers, without the recorded evidence, keep no reason: the
    // predicate is not "the two canonical forms differ".
    let unrecorded = probe_pair("1/(2*sqrt(x))", "sqrt(x)/(2*x)", "expression_symbolic");
    assert_eq!(
        documented_reason(&unrecorded, false, one_zero_says_yes),
        None
    );
    // A pair of the fixture whose canonical forms agree in every denominator and
    // every radical atom keeps no reason either. The fixture records
    // `radsimp_zero: true` for the row below, and neither side holds a radical,
    // so the canonical-form test is the one that refuses the reason.
    let no_radical = probe_pair("$(4/3)\\sin 3t$", "4*sin(3*t)/3", "expression_symbolic");
    assert_eq!(
        documented_reason(&no_radical, false, one_zero_says_yes),
        None
    );
    let same_shape = probe_pair(
        "2/x + 1/(x + 1)",
        "(3*x + 2)/(x*(x + 1))",
        "expression_symbolic",
    );
    assert_eq!(
        documented_reason(&same_shape, false, one_zero_says_yes),
        None
    );
}

#[test]
fn the_three_grammar_rulings_name_their_own_pairs() {
    let one_zero_says_no = OracleVerdict {
        equivalent: false,
        notation: false,
    };
    // 1.0 has no lowercase `e`, so `3e**x` is a free symbol to the power `x`.
    let euler = probe_pair("3e^(3x)", "3exp(3x)", "expression_symbolic");
    assert_eq!(
        documented_reason(&euler, true, one_zero_says_no),
        Some("the 1.0 namespace reads a bare `e` as a free symbol (spec 3.1)")
    );
    // Two sides that both write the bare `e` reach one free symbol in 1.0.
    let both_sides = probe_pair("3e^(3x)", "3*e^(3*x)", "expression_symbolic");
    assert_eq!(documented_reason(&both_sides, true, one_zero_says_no), None);
    // The spaced times sign.
    let times = probe_pair("2 x 2 x 3", "2*3*2", "expression_symbolic");
    assert_eq!(
        documented_reason(&times, true, one_zero_says_no),
        Some("2.0 reads a spaced `x` as the times sign (review 1, finding 18)")
    );
    // An answer that really holds the variable `x` keeps no times-sign reason.
    let variable = probe_pair("2*x", "x*2", "expression_symbolic");
    assert_eq!(documented_reason(&variable, true, one_zero_says_no), None);
    // The juxtaposed argument that stops at a function name.
    let chain = probe_pair("sec x tan x", "tan(x)*sec(x)", "expression_symbolic");
    assert_eq!(
        documented_reason(&chain, true, one_zero_says_no),
        Some("the juxtaposed argument stops at a function name (review 3, finding 5)")
    );
    // One function name with a bracket-free argument is no chain.
    let single = probe_pair("ln x + C", "C + log(x)", "expression_symbolic");
    assert_eq!(documented_reason(&single, true, one_zero_says_no), None);
    // Two function names that both carry their own brackets are no chain either:
    // SymPy swallows nothing there, and both checkers read one product.
    let bracketed = probe_pair("sin(x)*cos(x)", "cos(x)*sin(x)", "expression_symbolic");
    assert_eq!(documented_reason(&bracketed, true, one_zero_says_no), None);
}
