//! Part 2 of the `answer_divergence` tests. The header of `answer_divergence_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::divergence::*;

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
    // The glyph `√` is a token that takes one primary, and a second `√` is no
    // primary, so a doubled radical claims no verdict. 1.0: False, because the
    // radical pattern leaves the outer glyph and SymPy reads a bare name
    // (FIXM2i, `docs/reference/undecidable-answers.md`).
    assert_undecidable("2", "√√16", E, "a root with no argument");
    // The `+` of a brace body is no plain digit run, so the fractional part of
    // the mixed number takes no reading. 1.0: False.
    assert_undecidable(
        "5/2",
        "2\\frac{+1}{2}",
        E,
        "a mixed number whose fraction is not proper",
    );
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

#[test]
fn the_canonical_rational_form_runs_no_polynomial_gcd() {
    // FIXM2h replaced the `Inverse` atom with the rational-function form: every
    // value is `Poly / Poly`, both expanded, and NO polynomial GCD runs. The
    // narrowing is a decision of `docs/plans/M2.md`, and FIXM2i measured it: 6
    // pairs of the rational-rewrite sweep leave class 3 under the reason "no
    // polynomial GCD (V1 narrowing)", and every one of them is a shared
    // polynomial factor between two denominators.
    //
    // The 1.0 verdicts below come from the oracle on 2026-08-27. 1.0 reaches
    // True through `simplify(lhs - rhs) == 0`, which is the search 2.0 refuses
    // (D6, L2).
    //
    // 1.0: True. The two denominators share the factor `(x - 1)`.
    assert_eq!(
        check("-4(x + 1)/(x - 1)^3", "-4/(x - 1)**2 - 8/(x - 1)**3", E),
        decided(false, false)
    );
    // 1.0: True. The learner cancelled the common factor of the quotient.
    assert_eq!(
        check("(x^2 - 1)/(x - 1)", "x + 1", E),
        decided(false, false)
    );
    // The same form keeps every agreement it can reach without a GCD: two
    // spellings of one quotient are one value, because both sides expand.
    assert_eq!(
        check("2/x + 1/(x + 1)", "(3*x + 2)/(x*(x + 1))", E),
        decided(true, false)
    );
    assert_eq!(
        check("1/(x*(x + 1))", "1/x * 1/(x + 1)", E),
        decided(true, false)
    );
}

#[test]
fn the_canonical_form_rationalizes_no_radical() {
    // The second narrowing FIXM2i measured: 6 pairs leave class 3 under the
    // reason "no radical rationalization (V1 narrowing)". 2.0 keeps a root of a
    // non-rational radicand as one atom, and it never folds `sqrt(A)*sqrt(A)`
    // into `A`, because that identity needs `A >= 0` and the grammar carries no
    // domain (V1, D6).
    //
    // 1.0: True for every row, through `radsimp` inside `simplify`. The verdicts
    // come from the oracle on 2026-08-27.
    assert_eq!(check("1/(2√x)", "sqrt(x)/(2*x)", E), decided(false, false));
    assert_eq!(
        check("x/√(x^2 + 9)", "x*sqrt(x**2 + 9)/(x**2 + 9)", E),
        decided(false, false)
    );
    assert_eq!(
        check("1/√(1 - x^2)", "-sqrt(1 - x**2)/(x**2 - 1)", E),
        decided(false, false)
    );
    // A root of a RATIONAL radicand reduces on both sides, so the narrowing
    // takes no pair there (FIXM2h, review round 3, findings 9 and 11).
    assert_eq!(check("1/sqrt(2)", "sqrt(2)/2", E), decided(true, false));
    assert_eq!(check("sqrt(4/9)", "2/3", E), decided(true, false));
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
