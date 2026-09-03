//! U2 divergences: every pair where 2.0 does not give the 1.0 verdict.
//!
//! Each test records the 1.0 verdict as a literal, pins the 2.0 verdict as a
//! literal, and states the reason. The 1.0 verdicts come from
//! `cadus_web.sympy_check.answers_equivalent`, run on 2026-08-26 with
//! `/home/deploy/dev/cadus/.venv/bin/python`, and they match
//! `docs/reference/checker-1.0-spec.md` sections 6, 7.6, and 7.7.
//!
//! A divergence is not a defect until someone shows it is one. The four classes
//! below are the classes of spec section 9.3:
//!
//! 1. The answer leaves the decidable grammar, so 2.0 refuses a verdict (V2).
//! 2. 2.0 holds exact values only, so no float rung decides an answer (D6). A
//!    learner decimal is correct when it is the exact rounding of the value, and
//!    the verdict names the form (ruling `D6-dec`).
//! 3. 2.0 decides a wrong verifiable answer, and 1.0 handed it to the model (A3).
//! 4. 2.0 canonicalizes instead of simplifying, so it fixes some 1.0 misses and
//!    narrows some 1.0 hits (V1).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::divergence::*;

// ---------------------------------------------------------------------------
// Class 1 — outside the grammar, so 2.0 claims no verdict (V2)
// ---------------------------------------------------------------------------
#[test]
fn prose_answers_get_no_deterministic_verdict() {
    // 1.0: True for both. Prose parses into a product of one-letter symbols, and
    // multiplication commutes, so every anagram of a word answer is "correct"
    // (spec section 7.6). 167 corpus answers are in this class.
    assert_undecidable("yes", "sey", N, "a name that is not a function or variable");
    assert_undecidable(
        "even",
        "neve",
        N,
        "a name that is not a function or variable",
    );
}

#[test]
fn a_sympy_name_gets_no_deterministic_verdict() {
    // 1.0: `zoo` = `-zoo` is True, and a test pins it
    // (`tests/test_sympy_check.py:123`). `zoo` is SymPy's complex infinity, and
    // every SymPy name is in scope of a learner answer box (spec section 3.1).
    assert_undecidable(
        "zoo",
        "-zoo",
        E,
        "a name that is not a function or variable",
    );
    // 1.0: True. `III` parses as `I*I*I`, which is `-I`, the imaginary unit.
    assert_undecidable("III", "-I", E, "a name that is not a function or variable");
    // 1.0: True for `Interval(0, 1)` = `Interval(0,1)`, False for `Interval(0,2)`.
    // 2.0 has no constructor calls: `Interval` is not a whitelisted function.
    assert_undecidable(
        "Interval(0, 1)",
        "Interval(0,1)",
        E,
        "a name that is not a function or variable",
    );
    assert_undecidable(
        "Interval(0, 1)",
        "Interval(0,2)",
        E,
        "a name that is not a function or variable",
    );
    // 1.0: True. `oo` is SymPy's infinity. 2.0 has no infinity value.
    assert_undecidable("∞", "oo", E, "a name that is not a function or variable");
    assert_undecidable("-∞", "-oo", E, "a name that is not a function or variable");
}

#[test]
fn an_answer_past_the_work_bound_gets_no_verdict() {
    // 1.0: False, in 276 ms of the 300 ms L2 budget (spec section 3.2). That one
    // 12-character answer is the concrete case for V1. 2.0 bounds the work and
    // refuses instead of spending the budget.
    assert_undecidable(
        "(x+1)**200",
        "x**200+1",
        E,
        "the answer goes past the work bound",
    );
}

#[test]
fn a_space_grouped_number_after_a_factor_gets_no_deterministic_verdict() {
    // Review round 2, finding #11. 1.0, measured on 2026-08-26: True for
    // `0` against `x/1 000` and True for `250*x` against `x/2 500`. SymPy reads
    // the second group as its own factor, so `x/1 000` is `x/1*0`, which is 0.
    // The V4 table strips a space-grouped number on a full match of the whole
    // answer and nowhere else, so a group that reaches the parser stands after a
    // factor and 2.0 claims no verdict instead of inventing the value 0 (C4).
    assert_undecidable(
        "0",
        "x/1 000",
        E,
        "a space-grouped number stands after a factor",
    );
    assert_undecidable(
        "250*x",
        "x/2 500",
        E,
        "a space-grouped number stands after a factor",
    );
    assert_undecidable(
        "0",
        "sin x/1 000",
        E,
        "a space-grouped number stands after a factor",
    );
    // 1.0: False. The correct learner answer was already wrong in 1.0, and it
    // stays undecidable in 2.0, so the model grades it (V2).
    assert_undecidable(
        "x/1000",
        "x/1 000",
        E,
        "a space-grouped number stands after a factor",
    );
}

#[test]
fn a_mixed_number_whose_fraction_is_not_proper_gets_no_verdict() {
    // Review round 2, finding #1. `2\frac{3}{2}` is neither the mixed number 7/2
    // nor the product 3. 1.0, measured on 2026-08-26: False for `3` against
    // `2\frac{3}{2}`, because 1.0 deleted the backslash and could not parse the
    // string. 2.0 refuses the string instead of picking one of the two readings.
    assert_undecidable(
        "3",
        "2\\frac{3}{2}",
        N,
        "a mixed number whose fraction is not proper",
    );
}

// ---------------------------------------------------------------------------
// Class 2 — no float rung, and the exact rounding instead (D6, ruling `D6-dec`)
// ---------------------------------------------------------------------------
#[test]
fn a_rounded_decimal_is_the_value_with_a_note_on_its_form() {
    // 1.0: True for the first three, on a 1e-6 relative tolerance of two
    // `evalf()` results (spec section 3.2). The tolerance reads the SIZE of the
    // number, so a learner who rounds to six digits is "correct" and a learner
    // who rounds to five is not.
    //
    // 2.0 reads the DIGITS the learner typed (ruling `D6-dec`): every decimal
    // below is the half-to-even rounding of the exact value to its own digit
    // count, so every one of them is correct and carries the notation tag. The
    // decision is exact rational arithmetic and no float enters it (D6).
    assert_eq!(check("1/3", "0.333333", N), decided(true, true));
    assert_eq!(check("1/3", "0.3333333333", N), decided(true, true));
    assert_eq!(check("sqrt(2)", "1.41421356", N), decided(true, true));
    // 1.0: False. Five digits leave the 1e-6 tolerance, and five digits are a
    // rounding all the same.
    assert_eq!(check("1/3", "0.33333", N), decided(true, true));
    // 1.0: False, and 2.0 agrees. `0.3334` is no rounding of one third.
    assert_eq!(check("1/3", "0.3334", N), decided(false, false));
    // The 1.0 boundary case. 1.0: False at both tolerances. 2.0: correct, with
    // the tag, because `0.667` is two thirds to three digits.
    assert_eq!(check("2/3", "0.667", N), decided(true, true));
    assert_eq!(check("2/3", "0.666", N), decided(false, false));
    // 1.0: True. `pi` is not a square root of a positive rational, so 2.0 brings
    // no exact bound to the question and refuses the pair (V2). The refusal is a
    // model grade and never a verdict.
    assert_undecidable(
        "pi",
        "3.14159",
        N,
        "a rounding of a constant is not decidable",
    );
}

#[test]
fn a_decimal_of_ten_significant_digits_is_the_value_it_rounds() {
    // The `significant_decimal` generator family of spec section 9.3, added in
    // FIXM2f. It was the largest documented divergence of the generated set: 238
    // of the 240 pairs under the reason "no float tolerance rung (D6)". FIXM2i
    // widened the family from a rational and a radical to every irrational
    // number the grammar holds, so `pi` and `e` joined the roots.
    //
    // 1.0: True for all five, on the 1e-6 `evalf` rung
    // (`sympy_check.py:345-352`). 2.0 now decides the family by the rounding
    // rule, and `crates/core/tests/answer_oracle.rs` pins the split of the 240
    // pairs: 151 correct with the tag, 5 wrong, 84 refused.
    // A rational with no exact decimal:
    assert_eq!(check("5/12", "0.4166666667", N), decided(true, true));
    // The same, with a negative value:
    assert_eq!(check("-5/6", "-0.8333333333", E), decided(true, true));
    // A radical with a whole coefficient:
    assert_eq!(check("8*sqrt(2)", "11.31370850", E), decided(true, true));
    // A radical over a divisor:
    assert_eq!(check("2√3/3", "1.154700538", E), decided(true, true));
    // A nested radical. The canonical form of `sqrt(2 + sqrt(3))` is a
    // polynomial over a `sqrt` call, because the radicand is not a rational, so
    // the rounding rule does not read it and the pair keeps the wrong verdict.
    // It is one of the 5 pairs of the residue.
    assert_eq!(
        check("√(2 + √3)/2", "0.9659258263", E),
        decided(false, false)
    );
    // A fraction is the other shape of the residue. 1.0: True, at 1e-6 on two
    // values 1e-6 apart. A fraction carries no digit count, so no rounding reads
    // it and 2.0 grades it wrong.
    assert_eq!(check("1/1000", "1/1001", N), decided(false, false));
}

// ---------------------------------------------------------------------------
// Class 3 — the kind gate and the wrong-answer verdict (A3)
// ---------------------------------------------------------------------------
#[test]
fn an_undecidable_kind_never_gets_a_string_match() {
    // 1.0: False, because `dot_thousands_variant` gates on the kind. But the 1.0
    // string rung runs BEFORE the kind gate (`sympy_check.py:379-382`), so an
    // exact string match returns True for `proof` and `multi-step` too. 2.0 puts
    // the kind gate first, so an undecidable kind has no verdict at all.
    assert_undecidable(
        "7329",
        "7.329",
        AnswerKind::MultiStep,
        "the answer kind is not decidable",
    );
    assert_undecidable(
        "7329",
        "7.329",
        AnswerKind::Proof,
        "the answer kind is not decidable",
    );
    assert_undecidable(
        "a proof",
        "a proof",
        AnswerKind::Proof,
        "the answer kind is not decidable",
    );
}

#[test]
fn a_wrong_verifiable_answer_is_decided_and_not_deferred() {
    // 1.0 `answers_equivalent`: False. 1.0 `deterministic_grade`: None — a miss
    // went to the model, deliberately (`deterministic_grade.py:26-31`). A3 drops
    // that restriction: the checker answers for a right AND a wrong answer.
    assert_eq!(check("12", "14", N), decided(false, false));
    assert_eq!(check("7239", "7.329", N), decided(false, false));
}

// ---------------------------------------------------------------------------
// Class 4 — canonical forms instead of simplification (V1)
// ---------------------------------------------------------------------------
#[test]
fn the_false_negatives_of_spec_7_7_are_fixed() {
    // 1.0: False for every row. Each miss marked a correct learner wrong.
    let rows: [(&str, &str, AnswerKind); 8] = [
        // Implicit multiplication read `3 1/2` as `3*(1/2)`.
        ("3 1/2", "7/2", N),
        ("3 1/2", "3.5", N),
        // The rewriter deleted the backslash and left `frac{1}{2}`.
        ("1/2", "\\frac{1}{2}", N),
        // The rewriter left `x**{2}`, which SymPy cannot parse.
        ("x**2", "x^{2}", E),
        // 1.0 has no percent reading.
        ("0.5", "50%", N),
        // 1.0 has no `x =` prefix reading.
        ("5", "x=5", N),
        // A chained relational raised inside SymPy.
        ("-1 ≤ x ≤ 3", "-1 <= x <= 3", E),
        // Tuples compared structurally, so `17` and `17.0` were different.
        ("(4, 17)", "(4, 17.0)", N),
    ];
    for (expected, learner, kind) in rows {
        assert_eq!(
            check(expected, learner, kind),
            decided(true, false),
            "{expected:?} against {learner:?} on {kind}"
        );
    }
}

#[test]
fn e_is_eulers_number_on_both_sides() {
    // 1.0: False. `to_sympy_source` keeps the case, and SymPy reads lowercase `e`
    // as a plain symbol while `E` is Euler's number, so `e**2` is `e**2` and
    // `exp(2)` is a number. 2.0 reads `e` and `E` as one constant.
    assert_eq!(check("e**2", "exp(2)", E), decided(true, false));
}

#[test]
fn a_value_label_is_a_tolerance_and_not_a_value() {
    // 1.0, measured on 2026-08-26 with the oracle: False for `5` against `x=5`
    // and False for `x=5` against `5`. 1.0 has no label reading, so the whole
    // string goes to SymPy and the assignment raises. 2.0 adds the leading
    // `<var> =` label as a V4 tolerance (`docs/plans/M2.md`): a label on one
    // side alone falls away, and the two values compare.
    assert_eq!(check("5", "x=5", N), decided(true, false));
    assert_eq!(check("x=5", "5", N), decided(true, false));
    // Review round 1, the label ruling: the tolerance is symmetric, so the four
    // string-level pairs below are the whole rule. The 1.0 verdicts come from
    // the oracle on 2026-08-27; 1.0 has no label reading, so only its casefolded
    // string rung ever says True here.
    assert_eq!(check("x = 5", "5", N), decided(true, false));
    assert_eq!(check("5", "x = 5", N), decided(true, false));
    // A label on both sides names the unknown the answer answers for, so two
    // different names are two different answers. 1.0: False.
    assert_eq!(check("x = 4", "y = 4", N), decided(false, false));
    // The two names compare casefolded, as the string rung does (V4). 1.0: True,
    // through that same casefolded string rung.
    assert_eq!(check("x = 4", "X = 4", N), decided(true, false));
}

#[test]
fn a_transcendental_identity_is_not_decided() {
    // 1.0: True, through `simplify(lhs - rhs) == 0` in 16 ms (spec section 3.2).
    // 2.0 compares canonical forms and runs no simplification (V1), so a function
    // application equals only the same function application.
    assert_eq!(check("sin(x)**2+cos(x)**2", "1", E), decided(false, false));
}

#[test]
fn a_rational_function_is_not_cancelled() {
    // 1.0: True, through `simplify`. 2.0 normalizes the content of the divisor,
    // so a scalar multiple still matches, but it runs no polynomial greatest
    // common divisor, so a common factor does not cancel.
    assert_eq!(check("(x**2-1)/(x-1)", "x+1", E), decided(false, false));
    assert_eq!(check("1/(x+1)", "2/(2*x+2)", E), decided(true, false));
}

#[test]
fn a_polynomial_identity_is_decided_by_the_canonical_form() {
    // 1.0: True, in 62 ms of `simplify` (spec section 3.2). 2.0: True, by
    // expanding both sides into the same sparse polynomial. Same verdict, and it
    // costs no search.
    assert_eq!(check("(x+1)**2", "x**2+2*x+1", E), decided(true, false));
    assert_eq!(check("x**2 - 1", "(x-1)*(x+1)", E), decided(true, false));
}

#[test]
fn a_range_and_a_list_of_two_numbers_stay_different_answers() {
    // 1.0: False, because the chained relational raises. 2.0: False on purpose. A
    // chained inequality is a predicate over a variable and a list is a pair of
    // numbers, so the two are different answers.
    assert_eq!(check("-1 ≤ x ≤ 3", "[-1, 3]", E), decided(false, false));
}

#[test]
fn the_grammar_rulings_of_review_round_1_move_three_verdicts() {
    // The three measured divergences of FIXM2a. Every 1.0 verdict below comes
    // from the oracle on 2026-08-27, and every one of them is a ruling of
    // `docs/reviews/M2-review-1.md`, not a defect.
    //
    // 1.0: False. `to_sympy_source` rewrites `×` into `*`
    // (`/home/deploy/dev/cadus/cadus_web/sympy_check.py:97`) but it keeps the
    // letter `x`, so the left side is the polynomial `6*x*10**3` and the right
    // side is the number 6000. 2.0 reads a spaced `x` between two number
    // literals as the times sign (finding #18), so both sides are 6000.
    assert_eq!(check("6 x 10^3", "6 × 10^3", N), decided(true, false));
    assert_eq!(check("6 x 10^3", "6000", N), decided(true, false));
    // The same reading takes the upper-case letter (the times-`x` ruling).
    assert_eq!(check("6 X 10^3", "6000", N), decided(true, false));
    assert_eq!(check("3 X 4", "12", N), decided(true, false));
    //
    // 1.0: True. `2 x 2 x 3` becomes the SymPy source `2*x*2*x*3`, which is
    // `12*x**2`, and that is the right side. 2.0 reads the two spaced letters as
    // times signs, so the left side is the number 12.
    assert_eq!(check("2 x 2 x 3", "12x^2", E), decided(false, false));
    assert_eq!(check("2 x 2 x 3", "12", E), decided(true, false));
    //
    // 1.0: True. `_UNICODE_SIMPLE` maps `⅓` to the string `(1/3)`
    // (`/home/deploy/dev/cadus/cadus_web/sympy_check.py:141`), so `2⅓` becomes
    // `2(1/3)`, which SymPy reads as the product 2/3. 2.0 reads a vulgar glyph
    // after a digit run as the fractional part of a mixed number (findings #1
    // and #9), so `2⅓` is 7/3.
    assert_eq!(check("2/3", "2⅓", N), decided(false, false));
    assert_eq!(check("7/3", "2⅓", N), decided(true, false));
}

#[test]
fn the_grammar_rulings_of_review_round_2_move_five_verdicts() {
    // The measured divergences of FIXM2d. Every 1.0 verdict below comes from the
    // oracle on 2026-08-26, and every one of them follows a ruling of
    // `docs/reviews/M2-review-2.md`, not a defect.
    //
    // 1. The mixed number, in all five spellings (findings #1, #2, #3, #5, #6,
    //    #7). 1.0: True for `1` against `2 ½`, because `_UNICODE_SIMPLE` maps the
    //    glyph to `(1/2)` and SymPy reads the product. That True is the C4 false
    //    positive: a learner who wrote two and a half was correct against the
    //    authored answer 1 on the topic that teaches mixed numbers.
    assert_eq!(check("1", "2 ½", N), decided(false, false));
    assert_eq!(check("1", "2\\frac{1}{2}", N), decided(false, false));
    assert_eq!(check("1", "2 ½", E), decided(false, false));
    assert_eq!(check("1", "2\\frac{1}{2}", E), decided(false, false));
    // 1.0: False for the same learner answer against its own value. 2.0 reads
    // one value for the five spellings.
    assert_eq!(check("5/2", "2 ½", N), decided(true, false));
    assert_eq!(check("5/2", "2½", N), decided(true, false));
    assert_eq!(check("5/2", "2 1/2", N), decided(true, false));
    assert_eq!(check("5/2", "2\\frac{1}{2}", N), decided(true, false));
    assert_eq!(check("5/2", "2 \\frac{1}{2}", N), decided(true, false));
    assert_eq!(check("-5/2", "-2 ½", N), decided(true, false));
    assert_eq!(check("-2.5", "-2 ½", N), decided(true, false));
    assert_eq!(check("7/2", "2 ½ + 1", N), decided(true, false));
    // 1.0: True. The two spellings of the product keep the product, in 1.0 and
    // in 2.0, so the mixed-number reading takes no learner answer from them.
    assert_eq!(check("1", "2(1/2)", N), decided(true, false));
    assert_eq!(check("1", "2*½", N), decided(true, false));
    assert_eq!(check("1", "(2)½", N), decided(true, false));
    assert_eq!(check("x/2", "x½", E), decided(true, false));
    //
    // 2. The bracket-free function argument runs through an explicit `*`
    //    (findings #4, #15). 1.0: True for all five authored corpus answers of
    //    the shape, and False for `cos 2*x` against `x*cos(2)`. Round 1 read the
    //    learner spelling as `x*cos(2)`, which gave the correct learner a False
    //    and the meaningless answer a True.
    assert_eq!(check("cos 2x", "cos 2*x", E), decided(true, false));
    assert_eq!(check("$\\cos 2t$", "cos 2*t", E), decided(true, false));
    assert_eq!(
        check("$(4/3)\\sin 3t$", "(4/3)*sin 3*t", E),
        decided(true, false)
    );
    assert_eq!(check("cos 2*x", "x*cos(2)", E), decided(false, false));
    //
    // 3. The times-`x` reading takes a negated literal on its left (finding
    //    #12). 1.0: False for `-0.00025` against `-2.5 x 10^-4`, because 1.0
    //    keeps the letter and reads the polynomial `-0.00025*x`. 22 of the 27
    //    authored times-`x` answers are scientific notation.
    assert_eq!(check("-0.00025", "-2.5 x 10^-4", N), decided(true, false));
    assert_eq!(check("-300000", "-3 x 10^5", N), decided(true, false));
    // 1.0: True for `-1617x` against `-3 x 539`, which is the mirror C4 hole:
    // the polynomial accepted a learner who meant the number. 2.0 closes it.
    assert_eq!(check("-1617x", "-3 x 539", E), decided(false, false));
    assert_eq!(check("-1617", "-3 x 539", E), decided(true, false));
    //
    // 4. A `\sqrt` takes a product sign after a letter, a digit, or a `)`
    //    (finding #9). 1.0: False for every row, because `to_sympy_source`
    //    deletes the backslash and SymPy reads the name `xsqrt`. Round 1 gave
    //    the same answer no verdict, while `5x√2` was decided correct.
    assert_eq!(check("5*x*sqrt(2)", "5x\\sqrt{2}", E), decided(true, false));
    assert_eq!(check("5*x*sqrt(2)", "5x\\sqrt 2", E), decided(true, false));
    assert_eq!(
        check("3*x*sqrt(2*x)", "3x\\sqrt{2x}", E),
        decided(true, false)
    );
    // 1.0: False for the radical glyph as well, and 2.0 keeps its round 1
    // reading of it, so the two spellings of one value get one verdict.
    assert_eq!(check("5*x*sqrt(2)", "5x√2", E), decided(true, false));
}
