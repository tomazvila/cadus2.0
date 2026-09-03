//! Part 3 of the `answer_parse` tests. The header of `answer_parse_1.rs` names the sources.

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
fn the_mixed_number_production_refuses_a_thousands_group() {
    // Review finding #7. `1 000/3` is a space-grouped numerator, not a mixed
    // number, so the checker refuses it instead of inventing the value 1.
    for text in [
        "1 000/3",
        "1 200/300",
        "2 000/500",
        "1\u{a0}000/3",
        "3 0/2",
        "3 3/2",
        "3 2/2",
        "3 05/10",
        "3 1/0",
    ] {
        assert_eq!(
            parse(&normalize(text).source).unwrap_err().reason,
            "two numbers stand side by side",
            "{text:?} must stay undecidable"
        );
    }
    assert_eq!(value("1000/3"), value("1000/3"));
    assert_ne!(canonical_form("1 000/3").ok(), Some(value("1")));
}

#[test]
fn a_value_label_stays_on_the_tree() {
    // Review findings #2, #10 and #16. The label named the answer's variable, and
    // 1.0 deleted it, so `x = 4` and `y = 4` were one answer.
    assert_eq!(
        ast("x = 5"),
        Ast::Assign {
            var: "x".to_string(),
            value: Box::new(int(5))
        }
    );
    assert_eq!(
        ast("Y=-2"),
        Ast::Assign {
            var: "Y".to_string(),
            value: Box::new(Ast::Neg(Box::new(int(2))))
        }
    );
    assert_eq!(
        ast("y = x"),
        Ast::Assign {
            var: "y".to_string(),
            value: Box::new(var("x"))
        }
    );
    assert_eq!(
        ast("theta = 2"),
        Ast::Assign {
            var: "theta".to_string(),
            value: Box::new(int(2))
        }
    );
    assert_ne!(ast("x = 5"), ast("y = 5"));
    assert_ne!(ast("y = x"), ast("x = y"));
    for text in ["x = y = 5", "2 = 3", "sin = 2", "x =", "= 5", "x + 1 = 5"] {
        assert!(
            parse(&normalize(text).source).is_err(),
            "{text:?} must stay undecidable"
        );
    }
}

#[test]
fn a_bracket_free_function_argument_takes_the_whole_juxtaposed_chain() {
    // Review findings #3, #4 and #6, with the 1.0 reading of the five authored
    // corpus answers. The second literal of each row is the 1.0 `sympy_source`
    // and the third is the 1.0 `canonical` of `corpus_1_0.jsonl`.
    assert_eq!(ast("cos 2x"), call("cos", Ast::Mul(vec![int(2), var("x")])));
    assert_eq!(
        ast("$\\cos 2t$"),
        call("cos", Ast::Mul(vec![int(2), var("t")]))
    );
    assert_eq!(
        ast("$(4/3)\\sin 3t$"),
        Ast::Mul(vec![
            Ast::Fraction {
                numerator: BigInt::from(4),
                denominator: BigInt::from(3)
            },
            call("sin", Ast::Mul(vec![int(3), var("t")])),
        ])
    );
    assert_eq!(
        ast("$2\\cos 2t + (5/2)\\sin 2t$"),
        two_cos_2t_plus_five_halves_sin_2t()
    );
    assert_eq!(
        ast("$\\cos 3t + 2\\sin 3t$"),
        Ast::Add(vec![
            call("cos", Ast::Mul(vec![int(3), var("t")])),
            Ast::Mul(vec![int(2), call("sin", Ast::Mul(vec![int(3), var("t")]))]),
        ])
    );
    // The chain stops where the ruling says it stops.
    assert_eq!(
        ast("sin 3t^2"),
        call(
            "sin",
            Ast::Mul(vec![int(3), Ast::Pow(Box::new(var("t")), 2)])
        )
    );
    assert_eq!(
        ast("sqrt 2/2"),
        Ast::Div(Box::new(root(int(2))), Box::new(int(2)))
    );
    assert_eq!(
        ast("cos 2 + x"),
        Ast::Add(vec![call("cos", int(2)), var("x")])
    );
    // C4: the new reading must not admit the old, meaningless value.
    assert_eq!(value("cos 2x"), value("cos(2*x)"));
    assert_ne!(value("cos 2x"), value("x*cos(2)"));
    assert_ne!(value("cos 2x"), value("cos(2)*x"));
    assert_ne!(value("cos 2x"), value("cos(2*y)"));
    assert_eq!(value("$(4/3)\\sin 3t$"), value("(4/3)*sin(3*t)"));
    assert_ne!(value("$(4/3)\\sin 3t$"), value("(4/3)*t*sin(3)"));
}

#[test]
fn the_bracket_free_argument_runs_through_an_explicit_product_sign() {
    // Review round 2, findings #4 and #15. The chain stopped at `*`, so `cos 2*x`
    // was `x*cos(2)`: the meaningless value was correct against the authored
    // `cos 2*x`, and the correct `cos 2*x` was wrong against the authored
    // `cos 2x` on five corpus answers. 1.0 reads both spellings as `cos(2*x)`.
    assert_eq!(
        ast("cos 2*x"),
        call("cos", Ast::Mul(vec![int(2), var("x")]))
    );
    assert_eq!(
        ast("sin 3*t^2"),
        call(
            "sin",
            Ast::Mul(vec![int(3), Ast::Pow(Box::new(var("t")), 2)])
        )
    );
    assert_eq!(
        ast("$2\\cos 2*t + (5/2)\\sin 2*t$"),
        two_cos_2t_plus_five_halves_sin_2t()
    );
    // The five authored corpus answers of the shape take the learner spelling.
    assert_eq!(value("cos 2x"), value("cos 2*x"));
    assert_eq!(value("$\\cos 2t$"), value("cos 2*t"));
    assert_eq!(value("$(4/3)\\sin 3t$"), value("(4/3)*sin 3*t"));
    assert_eq!(
        value("$2\\cos 2t + (5/2)\\sin 2t$"),
        value("2*cos 2*t + (5/2)*sin 2*t")
    );
    assert_eq!(
        value("$\\cos 3t + 2\\sin 3t$"),
        value("cos 3*t + 2*sin 3*t")
    );
    // C4: the chain admits no wrong value. The old reading is a different value,
    // and the stops of the ruling stay where round 1 put them.
    assert_ne!(value("cos 2*x"), value("x*cos(2)"));
    assert_ne!(value("cos 2*x"), value("cos(2)*x"));
    assert_ne!(value("cos 2*x"), value("cos(2*y)"));
    assert_ne!(value("cos 2*x"), value("cos(x)"));
    assert_eq!(value("sqrt 2*2"), value("sqrt(4)"));
    assert_eq!(value("sqrt 2/2"), value("sqrt(2)/2"));
    assert_ne!(value("sqrt 2/2"), value("sqrt(1)"));
    assert_eq!(value("cos 2 + x"), value("cos(2) + x"));
    assert_ne!(value("cos 2 + x"), value("cos(2 + x)"));
    assert_eq!(value("cos 2 - x"), value("cos(2) - x"));
    assert_eq!(value("cos 2, 3"), value("(cos(2), 3)"));
}

#[test]
fn a_spaced_x_between_two_numbers_is_the_times_sign() {
    // Review finding #18. 27 authored corpus answers of 5 topics spell the times
    // sign `x`; every other `x` stays the variable.
    assert_eq!(
        ast("6 x 10^3"),
        Ast::Mul(vec![int(6), Ast::Pow(Box::new(int(10)), 3)])
    );
    assert_eq!(ast("2 x 2 x 3"), Ast::Mul(vec![int(2), int(2), int(3)]));
    assert_eq!(ast("5 x 5 x 5"), Ast::Mul(vec![int(5), int(5), int(5)]));
    assert_eq!(
        ast("2.5 x 10^-4"),
        Ast::Mul(vec![
            Ast::Decimal {
                mantissa: BigInt::from(25),
                scale: 1
            },
            Ast::Pow(Box::new(int(10)), -4)
        ])
    );
    // Every other `x` is the variable.
    assert_eq!(ast("2x"), Ast::Mul(vec![int(2), var("x")]));
    assert_eq!(ast("3 x"), Ast::Mul(vec![int(3), var("x")]));
    assert_eq!(ast("3 x y"), Ast::Mul(vec![int(3), var("x"), var("y")]));
    assert_eq!(ast("x 3"), Ast::Mul(vec![var("x"), int(3)]));
    assert_eq!(ast("3x 4"), Ast::Mul(vec![int(3), var("x"), int(4)]));
    // The times sign needs a space on both sides. `x4` is a label, as `R2` is.
    assert_eq!(
        parse(&normalize("3 x4").source).unwrap_err().reason,
        "a number glued to a name reads as a label"
    );
    assert_eq!(
        ast("3x 4x"),
        Ast::Mul(vec![int(3), var("x"), int(4), var("x")])
    );
    // C4: the value of the product, and not the value of a polynomial.
    assert_eq!(value("6 x 10^3"), value("6000"));
    assert_ne!(value("6 x 10^3"), value("6000*x"));
    assert_eq!(value("2 x 2 x 3"), value("12"));
    assert_ne!(value("2 x 2 x 3"), value("12x^2"));
    assert_ne!(value("2 x 2 x 3"), value("11"));
    assert_eq!(value("6 x 10^3"), value("6 × 10^3"));
    assert_eq!(value("6 x 10^3"), value("6*10**3"));
}

#[test]
fn the_times_letter_takes_a_negated_literal_on_its_left() {
    // Review round 2, finding #12. The reading matched a bare literal, and
    // `parse_unary` puts a leading minus in `Ast::Neg`, so `3 x 10^5` was 300000
    // while `-3 x 10^5` was the polynomial `-300000*x`. 22 of the 27 authored
    // times-`x` answers are scientific notation, and a measurement is negative.
    assert_eq!(
        ast("-3 x 10^5"),
        Ast::Mul(vec![
            Ast::Neg(Box::new(int(3))),
            Ast::Pow(Box::new(int(10)), 5)
        ])
    );
    assert_eq!(value("-3 x 10^5"), value("-300000"));
    assert_eq!(value("-2.5 x 10^-4"), value("-0.00025"));
    assert_eq!(value("-2.5 x 10^-4"), value("-2.5 × 10^-4"));
    assert_eq!(value("-7.2 x 10^-4"), value("-0.00072"));
    assert_eq!(value("-3 X 4"), value("-12"));
    assert_eq!(value("-1/2 x 10^2"), value("-50"));
    // C4: the number is not a polynomial, and the mirror hole is closed.
    assert_ne!(value("-3 x 10^5"), value("-300000*x"));
    assert_ne!(value("-2.5 x 10^-4"), value("-0.00025*x"));
    assert_ne!(value("-3 x 539"), value("-1617x"));
    assert_eq!(value("-3 x 539"), value("-1617"));
    // The sign belongs to the left literal alone. Every other `x` stays the
    // variable, in both cases and with or without a space.
    assert_eq!(value("-3x"), value("-3*x"));
    assert_eq!(value("-3 x"), value("-3*x"));
    assert_eq!(value("-2X"), value("-2*X"));
    assert_eq!(value("2 - 3 x 5"), value("-13"));
    assert_eq!(value("-2 1/2 x 2"), value("-5"));
}

#[test]
fn a_spaced_upper_case_x_between_two_numbers_is_the_times_sign() {
    // Review round 1, the times-`x` ruling: the reading takes the upper-case
    // letter too, because a learner writes the times sign in both cases.
    assert_eq!(
        ast("6 X 10^3"),
        Ast::Mul(vec![int(6), Ast::Pow(Box::new(int(10)), 3)])
    );
    assert_eq!(value("6 X 10^3"), value("6000"));
    assert_eq!(ast("3 X 4"), Ast::Mul(vec![int(3), int(4)]));
    assert_eq!(value("3 X 4"), value("12"));
    assert_ne!(value("3 X 4"), value("12*X"));
    // Every other `X` stays the variable.
    assert_eq!(ast("X"), var("X"));
    assert_eq!(ast("2X"), Ast::Mul(vec![int(2), var("X")]));
    assert_eq!(ast("3 X"), Ast::Mul(vec![int(3), var("X")]));
    assert_ne!(value("2X"), value("2"));
}

#[test]
fn a_short_letter_run_splits_into_single_letter_variables() {
    // The known item of the M2 fix wave: `3xy^2` is `3*x*y**2`, and the power
    // binds to the last letter only.
    assert_eq!(
        ast("3xy^2"),
        Ast::Mul(vec![
            int(3),
            Ast::Mul(vec![var("x"), Ast::Pow(Box::new(var("y")), 2)])
        ])
    );
    assert_eq!(ast("xy"), Ast::Mul(vec![var("x"), var("y")]));
    assert_eq!(ast("$xz$"), Ast::Mul(vec![var("x"), var("z")]));
    assert_eq!(
        ast("$yz/(x + z)^2$"),
        Ast::Div(
            Box::new(Ast::Mul(vec![var("y"), var("z")])),
            Box::new(Ast::Pow(Box::new(Ast::Add(vec![var("x"), var("z")])), 2))
        )
    );
    assert_eq!(
        ast("4ab^3"),
        Ast::Mul(vec![
            int(4),
            Ast::Mul(vec![var("a"), Ast::Pow(Box::new(var("b")), 3)])
        ])
    );
    // C4: the split must not admit a different monomial.
    assert_eq!(value("3xy^2"), value("3*x*y**2"));
    assert_ne!(value("3xy^2"), value("3*x**2*y"));
    assert_ne!(value("3xy^2"), value("(3*x*y)**2"));
    assert_ne!(value("3xy^2"), value("3*x*y"));
    assert_eq!(value("xy"), value("y*x"));
    assert_ne!(value("xy"), value("x*z"));
}

#[test]
fn a_letter_run_the_grammar_does_not_own_stays_undecidable() {
    for text in [
        // A differential.
        "dx",
        "3x^2 dx",
        "dy/dx",
        "2y · dy/dx",
        // A word, an upper-case label, and a name with a digit.
        "yes",
        "no",
        "oo",
        "DNE",
        "$\\{HH, HT, TH, TT\\}$",
        "$sY(s) - y(0)$",
        "x2y",
        // A repeated letter, and a run that is too long.
        "xx",
        "abcd",
        "min",
    ] {
        assert!(
            parse(&normalize(text).source).is_err(),
            "{text:?} must stay undecidable"
        );
    }
}

#[test]
fn powers_functions_and_constants_parse() {
    assert_eq!(ast("x^2"), Ast::Pow(Box::new(var("x")), 2));
    assert_eq!(ast("x^{2}"), Ast::Pow(Box::new(var("x")), 2));
    assert_eq!(ast("x^-2"), Ast::Pow(Box::new(var("x")), -2));
    assert_eq!(
        ast("e^x"),
        Ast::Func("exp".to_string(), vec![var("x")]),
        "e to a free power is the exp function of the grammar"
    );
    assert_eq!(
        ast("sec^2 x"),
        Ast::Pow(Box::new(Ast::Func("sec".to_string(), vec![var("x")])), 2)
    );
    assert_eq!(
        ast("π/6"),
        Ast::Div(Box::new(Ast::Const(Const::Pi)), Box::new(int(6)))
    );
    assert_eq!(ast("theta"), var("theta"));
    assert_eq!(
        ast("2 cos(x^2)"),
        Ast::Mul(vec![
            int(2),
            Ast::Func("cos".to_string(), vec![Ast::Pow(Box::new(var("x")), 2)])
        ])
    );
}

#[test]
fn collections_parse_by_their_brackets() {
    assert_eq!(ast("(4, 17)"), Ast::Tuple(vec![int(4), int(17)]));
    assert_eq!(ast("4, 17"), Ast::Tuple(vec![int(4), int(17)]));
    assert_eq!(ast("{1, 3, 5}"), Ast::Set(vec![int(1), int(3), int(5)]));
    assert_eq!(ast("$\\{2, 5\\}$"), Ast::Set(vec![int(2), int(5)]));
    assert_eq!(
        ast("[-3, 3]"),
        Ast::List(vec![Ast::Neg(Box::new(int(3))), int(3)])
    );
    assert_eq!(
        ast("(0, 1]"),
        Ast::Interval {
            lo: Box::new(int(0)),
            hi: Box::new(int(1)),
            lo_closed: false,
            hi_closed: true
        }
    );
    assert_eq!(
        ast("[0, 1)"),
        Ast::Interval {
            lo: Box::new(int(0)),
            hi: Box::new(int(1)),
            lo_closed: true,
            hi_closed: false
        }
    );
}

#[test]
fn inequalities_put_the_variable_on_the_left() {
    assert_eq!(
        ast("x <= -1"),
        Ast::Ineq {
            var: "x".to_string(),
            op: IneqOp::Le,
            bound: Box::new(Ast::Neg(Box::new(int(1))))
        }
    );
    assert_eq!(
        ast("4 < x"),
        Ast::Ineq {
            var: "x".to_string(),
            op: IneqOp::Gt,
            bound: Box::new(int(4))
        }
    );
    assert_eq!(
        ast("-1 ≤ x ≤ 3"),
        Ast::Chain {
            lo: Box::new(Ast::Neg(Box::new(int(1)))),
            lo_closed: true,
            var: "x".to_string(),
            hi_closed: true,
            hi: Box::new(int(3))
        }
    );
    assert_eq!(
        ast("3 ≥ x > -1"),
        Ast::Chain {
            lo: Box::new(Ast::Neg(Box::new(int(1)))),
            lo_closed: false,
            var: "x".to_string(),
            hi_closed: true,
            hi: Box::new(int(3))
        }
    );
}
