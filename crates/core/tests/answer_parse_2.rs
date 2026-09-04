//! Part 2 of the `answer_parse` tests. The header of `answer_parse_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::parse::*;

#[test]
fn a_latex_product_sign_in_front_of_a_root_is_a_product() {
    // Review round 3, finding #8. Round 2 wrote a `*` into the source in front
    // of a `\sqrt`, while `\cdot` and `\times` were still backslash words: the
    // `*` glued to the `t` of `\cdot`, the later word rewrite made it `**`, and
    // `2\times\sqrt{3}` became the power `2**sqrt(3)`, which got no verdict at
    // all. 326 fuzz expressions carried that refusal.
    for text in [
        "2\\times\\sqrt{3}",
        "2\\cdot\\sqrt{3}",
        "2 \\times \\sqrt{3}",
    ] {
        assert_eq!(
            ast(text),
            Ast::Mul(vec![int(2), root(int(3))]),
            "{text:?} is a product"
        );
        assert_eq!(value(text), value("2*sqrt(3)"));
        assert_ne!(value(text), value("2**3"));
    }
    assert_eq!(value("5\\cdot\\sqrt{2}"), value("5*sqrt(2)"));
    assert_eq!(value("x\\cdot\\sqrt{2}"), value("x*sqrt(2)"));
    assert_eq!(value("\\pi\\sqrt{2}"), value("pi*sqrt(2)"));
    assert_eq!(value("27\\times\\sqrt{64}"), value("216"));
    assert_eq!(value("2\\cdot\\sqrt 3"), value("2*sqrt(3)"));
    assert_eq!(value("2\\times√3"), value("2*sqrt(3)"));
    // C4: the product is not the power, and it is not a different radicand.
    assert_ne!(value("2\\times\\sqrt{3}"), value("2*sqrt(2)"));
    assert_ne!(value("2\\times\\sqrt{3}"), value("6"));
    // The power the round 2 rewrite invented has no whole exponent at all, so a
    // learner who really writes it still gets no verdict.
    assert_eq!(
        refusal("5**sqrt(2)"),
        "an exponent that is not a whole number"
    );
}

#[test]
fn every_construct_keeps_its_value_beside_every_operator() {
    // C4 neighbors of the round 3 structural ruling. Each construct is a token
    // now, so it must hold one value under `/`, `^`, `*`, a leading sign, a
    // nesting, and a function name. Round 2 built each construct with a string
    // rewrite, and four of the five re-associated under one of those operators
    // (findings #1, #2, #3, #4, #6, #8).
    let rows: [(&str, &str); 24] = [
        // Under `/`.
        ("x/\\frac{1}{2}", "2*x"),
        ("\\frac{1}{2}/2", "1/4"),
        ("2/\\sqrt{2}", "sqrt(2)"),
        ("15/30%", "50"),
        ("1/½", "2"),
        // Under `^`.
        ("\\frac{1}{2}^2", "1/4"),
        ("½²", "1/4"),
        ("\\sqrt{2}^2", "2"),
        ("\\sqrt{2}²", "2"),
        ("4%^2", "0.0016"),
        // Under `*`.
        ("2*\\frac{1}{2}", "1"),
        ("2\\times\\sqrt{3}", "2*sqrt(3)"),
        ("2*½", "1"),
        ("2*50%", "1"),
        // After a leading sign.
        ("-\\frac{1}{2}", "-0.5"),
        ("-\\sqrt{2}", "-sqrt(2)"),
        ("-½", "-0.5"),
        ("-50%", "-0.5"),
        // Nested inside each other.
        ("\\frac{\\frac{1}{2}}{3}", "1/6"),
        ("\\sqrt{\\sqrt{16}}", "2"),
        ("\\frac{1}{2}%", "1/200"),
        ("½%", "1/200"),
        // After a function name.
        ("sin \\frac{1}{2}", "sin(0.5)"),
        ("sin \\sqrt{2}", "sin(sqrt(2))"),
    ];
    for (learner, want) in rows {
        assert_eq!(value(learner), value(want), "{learner:?} is {want:?}");
        // The same pair reaches a verdict on both decidable answer kinds.
        assert_eq!(
            canonical_form(learner).ok(),
            canonical_form(want).ok(),
            "{learner:?} canonicalizes like {want:?}"
        );
    }
    // C4: a wrong value in the same spelling is refused.
    assert_ne!(value("x/\\frac{1}{2}"), value("x/2"));
    assert_ne!(value("2/\\sqrt{2}"), value("sqrt(2)/2"));
    assert_ne!(value("\\frac{1}{2}^2"), value("1/2"));
    assert_ne!(value("\\sqrt{2}^2"), value("4"));
    assert_ne!(value("-\\sqrt{2}"), value("sqrt(2)"));
    assert_ne!(value("\\frac{\\frac{1}{2}}{3}"), value("3/2"));
    assert_ne!(value("sin \\sqrt{2}"), value("sin(2)"));
    assert_ne!(value("\\frac{1}{2}%"), value("1/2"));
    // A mixed number is a primary too, so it takes the postfix percent.
    assert_eq!(value("3 1/2%"), value("3.5/100"));
    assert_eq!(value("3\\frac{1}{2}%"), value("3.5/100"));
    assert_eq!(value("3½%"), value("3.5/100"));
    assert_eq!(value("-3½%"), value("-3.5/100"));
    assert_ne!(value("3½%"), value("3.5"));
    // The `\left` and `\right` words drop, and the bracket they carry stays.
    assert_eq!(ast("\\left[0, 1\\right)"), ast("[0, 1)"));
    assert_eq!(ast("\\left\\{1, 2\\right\\}"), ast("{1, 2}"));
    // A construct with no argument gets no verdict, and never a value.
    for (text, reason) in [
        ("\\sqrt{}", "a root with no argument"),
        ("√", "a root with no argument"),
        ("√+", "a root with no argument"),
        ("\\sqrt", "a function name with no argument"),
        ("\\frac", "a name that is not a function or variable"),
        ("²", "a character outside the grammar"),
        ("2 ²", "a character outside the grammar"),
    ] {
        assert_eq!(refusal(text), reason, "{text:?} takes no reading");
    }
}

#[test]
fn a_fraction_token_that_is_not_proper_takes_no_mixed_number_reading() {
    // C4. `2\frac{3}{2}` is neither the mixed number 7/2 nor the product 3, and
    // a checker that picks one of the two readings grades a wrong answer
    // correct. The `0 < b < c` and plain-digit rules of the `a b/c` spelling
    // hold for the token spelling, and a failure refuses the whole answer.
    for text in [
        "2\\frac{3}{2}",
        "2\\frac{2}{2}",
        "2\\frac{0}{5}",
        "2\\frac{01}{2}",
        "2 \\frac{5}{4}",
        "-2\\frac{3}{2}",
    ] {
        assert_eq!(
            parse(&normalize(text).source).unwrap_err().reason,
            "a mixed number whose fraction is not proper",
            "{text:?} takes no reading"
        );
    }
    // A number token in front of a fraction is a mixed number or it is nothing.
    // The `b/c` spelling refuses the same shapes through the round 1 rule, so
    // the two spellings of one shape get one answer.
    for text in ["x 2½", "2.5½", "1/2 ½"] {
        assert_eq!(
            parse(&normalize(text).source).unwrap_err().reason,
            "a fraction stands after a number that is no whole part",
            "{text:?} takes no reading"
        );
    }
    for text in ["x 3 1/2", "2.5 1/2", "1/2 1/2"] {
        assert_eq!(
            parse(&normalize(text).source).unwrap_err().reason,
            "two numbers stand side by side",
            "{text:?} takes no reading"
        );
    }
    // A token that is no number in front of the fraction makes a product.
    assert_eq!(value("x½"), value("x/2"));
    assert_eq!(value("(2)½"), value("1"));
    // The rule reads the same inside a bracket-free function argument, so the
    // brackets change no value.
    assert_eq!(value("sin 2½"), value("sin(5/2)"));
    assert_eq!(value("sin 2½"), value("sin(2 ½)"));
    assert_eq!(value("sin 2 ½"), value("sin(5/2)"));
    assert_ne!(value("sin 2½"), value("sin(1)"));
    // The same fraction with no whole number in front of it keeps its value.
    assert_eq!(value("\\frac{3}{2}"), value("3/2"));
    assert_eq!(value("\\frac{0}{5}"), value("0"));
    assert_eq!(value("2*\\frac{3}{2}"), value("3"));
    // A zero denominator refuses the answer, as `1/0` does.
    assert_eq!(
        parse(&normalize("\\frac{1}{0}").source).unwrap_err().reason,
        "a fraction with a zero denominator"
    );
}

#[test]
fn a_number_token_a_divisor_or_an_exponent_took_is_no_whole_part() {
    // M2 review 4, finding #1. `read_mixed_number` tested the previous TOKEN and
    // read the whole part from the previous FACTOR. A `/` or a `^` folds the
    // number token into an `Ast::Div` or an `Ast::Pow`, the caller's backstop
    // refuses a number literal only, and the `b/c` spelling took the product
    // reading: `t/4 3/4` parsed to `((t/4)*3)/4` = 3t/16, and the checker graded
    // the wrong learner answer correct (C4). The four other spellings of the
    // same shape refused it, so one rule gave two verdicts.
    for text in [
        "t/4 3/4",
        "x/2 1/2",
        "x^2 1/2",
        "cos(x)/2 1/2",
        "pi/2 1/2",
        "x 2^3 1/2",
        "sqrt(2)/2 1/2",
    ] {
        assert_eq!(
            refusal(text),
            "a fraction stands after a number that is no whole part",
            "{text:?} takes no reading"
        );
    }
    // The glyph and the `\frac` spellings of the same shape keep the same
    // refusal, which is the point of the fix: one shape, one verdict.
    for text in ["t/4 ¾", "t/4 \\frac{3}{4}", "x^2 ½", "pi/2 ½"] {
        assert_eq!(
            refusal(text),
            "a fraction stands after a number that is no whole part",
            "{text:?} takes no reading"
        );
    }
    // The two shapes the caller already refused keep their own message, because
    // the factor in front of the fraction is a number literal there.
    for text in ["9/2 1/2", "x 2 1/2", "2*3 1/2"] {
        assert_eq!(
            refusal(text),
            "two numbers stand side by side",
            "{text:?} takes no reading"
        );
    }
    // The mixed number itself is untouched.
    assert_eq!(ast("2 1/2"), mixed(2, 1, 2));
    assert_eq!(ast("t/4"), Ast::Div(Box::new(var("t")), Box::new(int(4))));
}

#[test]
fn a_latex_root_is_a_token_and_a_factor_of_the_product_beside_it() {
    // Review round 2 finding #9, restated for the token grammar of round 3.
    // Round 2 wrote a product sign into the source in front of `\sqrt`, and that
    // sign landed on the last letter of a `\cdot` or a `\times` that touched the
    // root: `2\times\sqrt{3}` became the power `2**sqrt(3)` and got no verdict
    // at all (round 3, finding #8). A root is a token now, and a token needs no
    // sign to stand beside another factor.
    assert_eq!(
        ast("5x\\sqrt{2}"),
        Ast::Mul(vec![int(5), var("x"), root(int(2))])
    );
    assert_eq!(
        ast("5x\\sqrt 2"),
        Ast::Mul(vec![int(5), var("x"), root(int(2))])
    );
    assert_eq!(
        ast("3x\\sqrt{2x}"),
        Ast::Mul(vec![
            int(3),
            var("x"),
            root(Ast::Mul(vec![int(2), var("x")]))
        ])
    );
    assert_eq!(ast("2\\sqrt{3}"), Ast::Mul(vec![int(2), root(int(3))]));
    assert_eq!(
        ast("(x+1)\\sqrt{2}"),
        Ast::Mul(vec![Ast::Add(vec![var("x"), int(1)]), root(int(2))])
    );
    assert_eq!(ast("\\sqrt{2}"), root(int(2)));
    assert_eq!(value("5x\\sqrt{2}"), value("5*x*sqrt(2)"));
    assert_eq!(value("5x\\sqrt{2}"), value("5x√2"));
    assert_eq!(value("5x\\sqrt 2"), value("5*x*sqrt(2)"));
    assert_eq!(value("3x\\sqrt{2x}"), value("3*x*sqrt(2*x)"));
    assert_eq!(value("2\\sqrt{3}"), value("2*sqrt(3)"));
    // C4: the product sign changes no value, and it admits no wrong one.
    assert_ne!(value("5x\\sqrt{2}"), value("5*x*sqrt(3)"));
    assert_ne!(value("5x\\sqrt{2}"), value("5*sqrt(2)"));
    assert_ne!(value("5x\\sqrt{2}"), value("10*x"));
    // The 11 authored corpus answers of the bare `\sqrt` form keep their value.
    assert_eq!(value("$2\\sqrt 2 - 2$"), value("2*sqrt(2) - 2"));
    assert_eq!(value("$\\pi \\sqrt 2$"), value("pi*sqrt(2)"));
}

#[test]
fn a_space_grouped_number_after_a_factor_is_undecidable() {
    // Review round 2, finding #11. The guard fired for a literal in front alone,
    // so `x/1 000` read as `x/1 * 0` and canonicalized to 0: an authored answer
    // of 0 accepted a learner who wrote a thousandth of x (C4).
    for text in [
        "x/1 000",
        "2x/1 000",
        "pi/1 000",
        "sin x/1 000",
        "x/2 500",
        "x*1 000",
        "(x+1) 000",
        "x 0000",
    ] {
        let reason = parse(&normalize(text).source).unwrap_err().reason;
        assert!(
            reason == "a space-grouped number stands after a factor"
                || reason == "two numbers stand side by side",
            "{text:?} gave {reason:?}"
        );
    }
    assert!(canonical_form("x/1 000").is_err());
    assert!(canonical_form("x/2 500").is_err());
    // The V4 full-match rule still reads a space-grouped number as one value.
    assert_eq!(value("1 000"), value("1000"));
    assert_eq!(value("7\u{00a0}329"), value("7329"));
    // A spaced number that no number stands in front of holds no group, so it
    // keeps the product reading that 1.0 gives it (1.0: True for all four).
    assert_eq!(value("x 3"), value("3*x"));
    assert_eq!(value("x 100"), value("100*x"));
    assert_eq!(value("x 500"), value("500*x"));
    assert_eq!(value("6 y 10^3"), value("6000*y"));
    assert_eq!(value("3x 4"), value("12*x"));
    assert_eq!(value("2x 500"), value("1000*x"));
}

// ---------------------------------------------------------------------------
// V1 — the grammar
// ---------------------------------------------------------------------------
#[test]
fn the_number_productions_parse_to_exact_values() {
    assert_eq!(ast("7,329."), int(7329));
    assert_eq!(
        ast("0.7"),
        Ast::Decimal {
            mantissa: BigInt::from(7),
            scale: 1
        }
    );
    assert_eq!(
        ast("12.50"),
        Ast::Decimal {
            mantissa: BigInt::from(1250),
            scale: 2
        }
    );
    assert_eq!(
        ast("1/2"),
        Ast::Fraction {
            numerator: BigInt::from(1),
            denominator: BigInt::from(2)
        }
    );
    assert_eq!(
        ast("1/-2"),
        Ast::Fraction {
            numerator: BigInt::from(-1),
            denominator: BigInt::from(2)
        }
    );
    assert_eq!(
        ast("3 1/2"),
        Ast::Mixed {
            whole: BigInt::from(3),
            numerator: BigInt::from(1),
            denominator: BigInt::from(2)
        }
    );
    // Restated in review round 3, finding #7: the whole part holds the
    // magnitude, and the sign token puts the node inside an `Ast::Neg`.
    assert_eq!(ast("-3 1/2"), Ast::Neg(Box::new(mixed(3, 1, 2))));
    assert_eq!(
        ast("½"),
        Ast::Fraction {
            numerator: BigInt::from(1),
            denominator: BigInt::from(2)
        }
    );
    assert_eq!(
        ast("50%"),
        Ast::Fraction {
            numerator: BigInt::from(50),
            denominator: BigInt::from(100)
        }
    );
}

#[test]
fn implicit_multiplication_parses() {
    assert_eq!(ast("2x"), Ast::Mul(vec![int(2), var("x")]));
    // Restated in review round 3: every root is one `Ast::Sqrt` node, whatever
    // the answer spells it — the name, `\sqrt{a}`, `\sqrt a`, or the glyph.
    assert_eq!(ast("15 sqrt 3"), Ast::Mul(vec![int(15), root(int(3))]));
    assert_eq!(ast("15√3"), Ast::Mul(vec![int(15), root(int(3))]));
    assert_eq!(ast("15 sqrt(3)"), Ast::Mul(vec![int(15), root(int(3))]));
    assert_eq!(ast("15\\sqrt{3}"), Ast::Mul(vec![int(15), root(int(3))]));
    assert_eq!(
        ast("(x + 1)(x - 1)"),
        Ast::Mul(vec![
            Ast::Add(vec![var("x"), int(1)]),
            Ast::Add(vec![var("x"), Ast::Neg(Box::new(int(1)))]),
        ])
    );
    // `6 x 10^3` moved to `a_spaced_x_between_two_numbers_is_the_times_sign`:
    // the `x` of that answer is the times sign (review finding #18).
    assert_eq!(
        ast("6 y 10^3"),
        Ast::Mul(vec![int(6), var("y"), Ast::Pow(Box::new(int(10)), 3)])
    );
}

#[test]
fn a_vulgar_fraction_glyph_parses_to_the_mixed_number() {
    assert_eq!(
        ast("3½"),
        Ast::Mixed {
            whole: BigInt::from(3),
            numerator: BigInt::from(1),
            denominator: BigInt::from(2)
        }
    );
    assert_eq!(
        ast("2⅓"),
        Ast::Mixed {
            whole: BigInt::from(2),
            numerator: BigInt::from(1),
            denominator: BigInt::from(3)
        }
    );
    assert_eq!(ast("-3½"), Ast::Neg(Box::new(mixed(3, 1, 2))));
}

#[test]
fn a_negative_mixed_number_keeps_its_sign_when_the_whole_part_is_zero() {
    // Review round 3, finding #7. The parser folded the sign into the whole
    // part, `canon` read the sign back from that integer, and `-0` is the
    // integer zero: `-0 1/2` therefore became plus one half, so a learner who
    // subtracted in the wrong order was graded correct against an authored
    // `1/2` (C4). The sign comes from the sign token now.
    assert_eq!(ast("-0 1/2"), Ast::Neg(Box::new(mixed(0, 1, 2))));
    assert_eq!(ast("-0½"), Ast::Neg(Box::new(mixed(0, 1, 2))));
    assert_eq!(ast("-0 3/4"), Ast::Neg(Box::new(mixed(0, 3, 4))));
    assert_eq!(ast("-0\\frac{1}{2}"), Ast::Neg(Box::new(mixed(0, 1, 2))));
    assert_eq!(value("-0 1/2"), value("-1/2"));
    assert_eq!(value("-0½"), value("-1/2"));
    assert_eq!(value("-0 3/4"), value("-0.75"));
    assert_eq!(value("-0\\frac{1}{2}"), value("-1/2"));
    // C4: the wrong sign is a wrong value, in every spelling.
    assert_ne!(value("-0 1/2"), value("1/2"));
    assert_ne!(value("-0½"), value("1/2"));
    assert_ne!(value("-0 3/4"), value("0.75"));
    // The positive spellings and the non-zero whole part are unchanged.
    assert_eq!(value("0 1/2"), value("1/2"));
    assert_eq!(value("-2 1/2"), value("-5/2"));
    assert_eq!(value("-1 1/3"), value("-4/3"));
    assert_eq!(value("+0 1/2"), value("1/2"));
    assert_eq!(value("--0 1/2"), value("1/2"));
    assert_eq!(value("1 - 0 1/2"), value("1/2"));
}
