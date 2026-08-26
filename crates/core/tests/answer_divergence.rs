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
//! 2. 2.0 holds exact values only, so no float rung decides an answer (D6).
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

use cadus_core::answer::{Outcome, Verdict, check};
use cadus_core::curriculum::AnswerKind;

/// The `numeric` answer kind.
const N: AnswerKind = AnswerKind::Numeric;
/// The `expression` answer kind.
const E: AnswerKind = AnswerKind::Expression;

/// The outcome of a decided check.
fn decided(correct: bool, notation: bool) -> Outcome {
    Outcome::Decided(Verdict { correct, notation })
}

/// Assert that 2.0 refuses a verdict, and name the refusal.
fn assert_undecidable(expected: &str, learner: &str, kind: AnswerKind, reason: &str) {
    match check(expected, learner, kind) {
        Outcome::Undecidable(refusal) => assert_eq!(
            refusal.reason, reason,
            "{expected:?} against {learner:?} on {kind}"
        ),
        other => panic!("{expected:?} against {learner:?} on {kind} gave {other:?}"),
    }
}

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
// Class 2 — no float rung (D6, V1)
// ---------------------------------------------------------------------------

#[test]
fn a_rounded_decimal_is_not_the_exact_value() {
    // 1.0: True for all four. The SymPy rung compares `evalf()` results at a 1e-6
    // relative tolerance (spec section 3.2), so a learner who rounds is "correct"
    // and a learner who rounds a little more is not. 2.0 has no float in any
    // equality decision (D6), so a rounded decimal is a different value.
    assert_eq!(check("1/3", "0.333333", N), decided(false, false));
    assert_eq!(check("1/3", "0.3333333333", N), decided(false, false));
    assert_eq!(check("sqrt(2)", "1.41421356", N), decided(false, false));
    assert_eq!(check("pi", "3.14159", N), decided(false, false));
    // The 1.0 boundary case stays wrong in both versions.
    assert_eq!(check("2/3", "0.667", N), decided(false, false));
}

#[test]
fn a_decimal_of_ten_significant_digits_is_not_the_value_it_rounds() {
    // The `significant_decimal` generator family of spec section 9.3, added in
    // FIXM2f. It is the largest documented divergence of the generated set: 154
    // of the 156 pairs under the reason "no float tolerance rung (D6)".
    //
    // 1.0: True for all five, on the 1e-6 `evalf` rung
    // (`sympy_check.py:345-352`). 2.0: False, because a decimal is an exact
    // rational and it is not the rational or the radical it approximates (D6).
    // Each pair below is one shape of the family, taken from the generated set.
    // A rational with no exact decimal:
    assert_eq!(check("5/12", "0.4166666667", N), decided(false, false));
    // The same, with a negative value:
    assert_eq!(check("-5/6", "-0.8333333333", E), decided(false, false));
    // A radical with a whole coefficient:
    assert_eq!(check("8*sqrt(2)", "11.31370850", E), decided(false, false));
    // A radical over a divisor:
    assert_eq!(check("2√3/3", "1.154700538", E), decided(false, false));
    // A nested radical. `both_sides_are_numbers` refused this shape, so the
    // harness left it in class 3; the tolerance predicate names it.
    assert_eq!(
        check("√(2 + √3)/2", "0.9659258263", E),
        decided(false, false)
    );
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

#[test]
fn the_bracket_free_argument_stops_at_a_function_and_moves_three_verdicts() {
    // Review round 3, finding #5, and the orchestrator ruling of that round: a
    // juxtaposed function argument stops at a function name. The ruling is a
    // DIVERGENCE from 1.0, not a port of it. 1.0 hands the whole string to
    // SymPy, whose `implicit_multiplication_application` transformation swallows
    // the second function name into the first argument. The 1.0 reading is
    // recorded in the `canonical` field of the three authored corpus rows of
    // `crates/core/tests/fixtures/answers/corpus_1_0.jsonl`:
    //
    //   derivatives-trig kp1 1  `sec x tan x`    1.0 canonical `sec(x*tan(x))`
    //   derivatives-trig kp1 2  `-csc x cot x`   1.0 canonical `-csc(x*cot(x))`
    //   derivatives-trig kp2 2  `2 sin x cos x`  1.0 canonical `2*sin(x*cos(x))`
    //
    // The 1.0 verdicts below come from the oracle on 2026-08-26. They show why
    // the ruling moves them: 1.0 grades the meaningless composite correct and
    // the correct derivative wrong on the three topics that teach it (C4).
    //
    // 1.0: True. `sec(x tan x)` is the same nested composite as the authored
    // side, so the answer that no learner means is correct.
    assert_eq!(
        check("sec x tan x", "sec(x tan x)", E),
        decided(false, false)
    );
    assert_eq!(
        check("2 sin x cos x", "2 sin(x cos x)", E),
        decided(false, false)
    );
    // 1.0: False for every row below. The learner who wrote the right derivative
    // lost the item, and A3 makes that verdict final.
    assert_eq!(
        check("sec x tan x", "sec(x)tan(x)", E),
        decided(true, false)
    );
    assert_eq!(
        check("sec x tan x", "sec(x)*tan(x)", E),
        decided(true, false)
    );
    assert_eq!(
        check("-csc x cot x", "-csc(x)cot(x)", E),
        decided(true, false)
    );
    assert_eq!(
        check("2 sin x cos x", "2*sin(x)*cos(x)", E),
        decided(true, false)
    );
    // The chain stops in the same place after an explicit `*`, so the two
    // spellings of the product are one answer. 1.0: True for `cos 2x` against
    // `cos 2*x`, and 2.0 keeps that agreement, because a number is no function.
    assert_eq!(
        check("sec x tan x", "sec x * tan x", E),
        decided(true, false)
    );
    assert_eq!(check("cos 2x", "cos 2*x", E), decided(true, false));
    // 1.0: False. A root is the name `sqrt` in another spelling, so it stops the
    // chain as a name does.
    assert_eq!(
        check("cos(2)*sqrt(3)", "cos 2\\sqrt{3}", E),
        decided(true, false)
    );
}

#[test]
fn the_grammar_rulings_of_review_round_3_move_the_construct_verdicts() {
    // The measured divergences of FIXM2g. Every 1.0 verdict below comes from the
    // oracle on 2026-08-26, and every one of them follows a ruling of
    // `docs/reviews/M2-review-3.md`, not a defect.
    //
    // 1. `\frac` is one token, whatever whitespace its braces hold (findings #1,
    //    #2). 1.0: False for every row, because `to_sympy_source` deletes the
    //    backslash and SymPy cannot read `frac{ 1}{2}`. Round 2 of 2.0 graded
    //    the wrong answer two and a half correct against the authored `1`.
    assert_eq!(check("1", "2\\frac{ 1}{2}", E), decided(false, false));
    assert_eq!(check("5/2", "2\\frac{ 1}{2}", E), decided(true, false));
    assert_eq!(check("4 1/2", "4\\frac{ 1}{2}", E), decided(true, false));
    assert_eq!(check("9/4", "3\\frac{ 3 }{ 4 }", E), decided(false, false));
    assert_eq!(check("15/4", "3\\frac{ 3 }{ 4 }", E), decided(true, false));
    assert_eq!(
        check("1/6", "\\frac{1}{2}\\frac{1}{3}", E),
        decided(true, false)
    );
    assert_eq!(check("(x+1)/2", "\\frac{x+1}{2}", E), decided(true, false));
    // A number in front of a `\frac` whose braces hold an expression is neither
    // a mixed number nor a product, so 2.0 claims no verdict. 1.0: False.
    assert_undecidable(
        "3",
        "2\\frac{x+1}{2}",
        E,
        "a mixed number whose fraction is not proper",
    );
    //
    // 2. A percent binds to the primary in front of it (findings #3, #4). 1.0
    //    has no percent reading at all, so every row is False there. Round 2 of
    //    2.0 read `15/30%` as 1/200 on the topic `percent-finding-the-whole`,
    //    whose own solution sketch is `15 ÷ 0.30 = 50`.
    assert_eq!(check("50", "15/30%", N), decided(true, false));
    assert_eq!(check("1/200", "15/30%", N), decided(false, false));
    assert_eq!(check("25", "1/4%", N), decided(true, false));
    assert_eq!(check("0.0025", "1/4%", N), decided(false, false));
    assert_eq!(check("0.0016", "4%^2", N), decided(true, false));
    assert_eq!(check("3.04", "3 + 4%", N), decided(true, false));
    // An exponent of `50%` is a half, and the grammar holds a whole exponent
    // only, so `2^50%` gets no verdict instead of a second reading. 1.0: False.
    assert_undecidable(
        "sqrt(2)",
        "2^50%",
        E,
        "an exponent that is not a whole number",
    );
    assert_undecidable("0.5", "50%%", N, "two percent signs on one number");
    //
    // 3. A thousands group is one value on a full match of the whole answer and
    //    nowhere else (finding #6). Round 2 of 2.0 read `1 500%` as 15 and
    //    `3 + 1 500%` as 8, so one string had two readings and the second graded
    //    a learner who wrote 18 correct against an authored 8.
    assert_undecidable("15", "1 500%", N, "two numbers stand side by side");
    assert_undecidable("18", "3 + 1 500%", E, "two numbers stand side by side");
    assert_undecidable("8", "3 + 1 500%", E, "two numbers stand side by side");
    assert_undecidable(
        "5*x",
        "x/1 500%",
        E,
        "a space-grouped number stands after a factor",
    );
    // The comma separator reads alike, so the two separators of the V4 table get
    // one answer. 1.0: True for `(4, 500)` against `3 + 1,500`, because SymPy
    // reads the comma as a tuple separator and the group as its own number.
    assert_undecidable(
        "15",
        "1,500%",
        N,
        "a comma-grouped number stands in a longer answer",
    );
    assert_undecidable(
        "(4, 500)",
        "3 + 1,500",
        E,
        "a comma-grouped number stands in a longer answer",
    );
    // The full-match rule itself is unchanged. 1.0: True.
    assert_eq!(check("1500", "1,500", N), decided(true, false));
    //
    // 4. A mixed number keeps the sign of a `-0` whole part (finding #7). 1.0:
    //    False for both rows, because 1.0 has no mixed-number reading. Round 2
    //    of 2.0 dropped the minus and graded minus one half correct against an
    //    authored `1/2`.
    assert_eq!(check("1/2", "-0 1/2", E), decided(false, false));
    assert_eq!(check("-1/2", "-0 1/2", E), decided(true, false));
    assert_eq!(check("1/2", "-0½", E), decided(false, false));
    assert_eq!(check("-1/2", "-0½", E), decided(true, false));
    //
    // 5. A LaTeX product sign in front of a root is a product (finding #8). 1.0:
    //    False for every row, because it deletes the backslash and reads the
    //    name `timessqrt`. Round 2 of 2.0 wrote the power `2**sqrt(3)` and gave
    //    the pair no verdict at all.
    assert_eq!(
        check("2*sqrt(3)", "2\\times\\sqrt{3}", E),
        decided(true, false)
    );
    assert_eq!(
        check("5*sqrt(2)", "5\\cdot\\sqrt{2}", E),
        decided(true, false)
    );
    assert_eq!(check("216", "27\\times\\sqrt{64}", E), decided(true, false));
}

// ---------------------------------------------------------------------------
// Pinned agreements: the traps of spec section 7 that 2.0 must not reopen
// ---------------------------------------------------------------------------

#[test]
fn the_traps_of_spec_7_keep_the_1_0_verdict() {
    // 1.0: False. The 1000x hole of spec section 7.2, closed by the non-zero
    // leading group of `_DOT_GROUPS_RE`. A learner's unit slip stays wrong.
    assert_eq!(check("8", "0.008", N), decided(false, false));
    // 1.0: False. There is no decimal-comma reading, in 1.0 or in 2.0.
    assert_eq!(check("0.5", "0,5", N), decided(false, false));
    // 1.0: True, and `dot_thousands_variant` is True as well.
    assert_eq!(check("7329", "7.329", N), decided(true, true));
    // 1.0: True. Exact arithmetic decides it in 2.0.
    assert_eq!(check("7", "49/7", N), decided(true, false));
}
