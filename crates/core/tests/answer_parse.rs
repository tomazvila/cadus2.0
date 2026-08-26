//! U1 acceptance: normalization (V4) and the grammar parser (V1, V2).
//!
//! Every count and every tree in this file is a literal. No expected value is read
//! back from the code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeSet;

use cadus_core::answer::{
    Ast, Canon, Const, IneqOp, MAX_ANSWER_CHARS, canonical_form, normalize, parse,
};
use num_bigint::BigInt;

/// One corpus row of `crates/core/tests/fixtures/answers/corpus_1_0.jsonl`.
#[derive(serde::Deserialize)]
struct CorpusRow {
    answer: String,
    shape: String,
    topic_id: String,
    kp_id: String,
    exemplar_index: i64,
}

/// One row of the committed undecidable fixture.
#[derive(serde::Deserialize)]
struct ResidueRow {
    answer: String,
    topic_id: String,
    kp_id: String,
    exemplar_index: i64,
}

/// The identity of one answer in the corpus.
type Key = (String, String, i64, String);

/// Read the corpus.
fn corpus() -> Vec<CorpusRow> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/answers/corpus_1_0.jsonl");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}")))
        .collect()
}

/// Read the committed set of answers the grammar refuses.
fn committed_residue() -> BTreeSet<Key> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/answers/undecidable_1_0.jsonl");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let row: ResidueRow =
                serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}"));
            (row.topic_id, row.kp_id, row.exemplar_index, row.answer)
        })
        .collect()
}

/// Normalize and parse one answer, and fail the test when the grammar refuses it.
fn ast(text: &str) -> Ast {
    let source = normalize(text).source;
    parse(&source).unwrap_or_else(|e| panic!("{text:?} -> {source:?}: {}", e.reason))
}

/// Build an integer node.
fn int(value: i64) -> Ast {
    Ast::Integer(BigInt::from(value))
}

/// Build a variable node.
fn var(name: &str) -> Ast {
    Ast::Var(name.to_string())
}

/// Build a function call node with one argument.
fn call(name: &str, argument: Ast) -> Ast {
    Ast::Func(name.to_string(), vec![argument])
}

/// Canonicalize one answer, and fail the test when the grammar refuses it.
fn value(text: &str) -> Canon {
    canonical_form(text).unwrap_or_else(|e| panic!("{text:?}: {}", e.reason))
}

// ---------------------------------------------------------------------------
// V4 — normalization
// ---------------------------------------------------------------------------

#[test]
fn the_pinned_pairs_of_spec_section_6_1_give_the_literal_source() {
    let pins: [(&str, &str); 8] = [
        ("7,329.", "7329"),
        ("$7,329$", "7329"),
        ("7\u{00a0}329", "7329"),
        ("15√3", "15*sqrt(3)"),
        ("x²+1", "x**2+1"),
        ("30°", "30"),
        ("½", "(1/2)"),
        ("1 + 2x.", "1 + 2x"),
    ];
    for (input, want) in pins {
        assert_eq!(normalize(input).source, want, "source of {input:?}");
    }
}

#[test]
fn the_2_0_additions_of_the_v4_table_give_the_literal_source() {
    let pins: [(&str, &str); 12] = [
        ("\\frac{1}{2}", "((1)/(2))"),
        ("\\sqrt{2}", "sqrt(2)"),
        ("x^{2}", "x**(2)"),
        ("50%", "(50)/100"),
        // The label stays in the source. The parser reads it, and `check`
        // compares the two labels (review findings #2, #10, #16).
        ("x = 5", "x = 5"),
        ("2\\cdot 3", "2* 3"),
        ("6\\times 7", "6* 7"),
        ("8÷2", "8/2"),
        ("\\left(x\\right)", "(x)"),
        ("-1 ≤ x ≤ 3", "-1 <= x <= 3"),
        ("2√3/3", "2*sqrt(3)/3"),
        ("x/√(x^2 + 9)", "x/sqrt(x**2 + 9)"),
    ];
    for (input, want) in pins {
        assert_eq!(normalize(input).source, want, "source of {input:?}");
    }
}

#[test]
fn the_unicode_table_of_1_0_gives_the_literal_source() {
    let pins: [(&str, &str); 10] = [
        ("π", "pi"),
        ("τ", "(2*pi)"),
        ("∞", "oo"),
        ("2·3", "2*3"),
        ("−5", "-5"),
        ("–5", "-5"),
        ("θ", "theta"),
        ("⅓", "(1/3)"),
        ("¾", "(3/4)"),
        ("2³", "2**3"),
    ];
    for (input, want) in pins {
        assert_eq!(normalize(input).source, want, "source of {input:?}");
    }
}

#[test]
fn the_string_key_casefolds_and_keeps_the_1_0_order() {
    let pins: [(&str, &str); 6] = [
        ("$7,329$", "7,329"),
        ("7329..", "7329"),
        ("  a   b  ", "a b"),
        ("YES", "yes"),
        ("SQRT(2)", "sqrt(2)"),
        ("15√3", "15√3"),
    ];
    for (input, want) in pins {
        assert_eq!(normalize(input).string_key, want, "string key of {input:?}");
    }
}

#[test]
fn a_percent_divides_the_number_in_front_of_it_and_not_the_body() {
    // Review finding #17. The old reading wrapped the whole body, so `3 + 4%`
    // became `(3 + 4)/100` and a learner answer 43 times the authored value was
    // graded correct.
    let pins: [(&str, &str); 6] = [
        ("50%", "(50)/100"),
        ("3 + 4%", "3 + (4)/100"),
        ("1 + 49%", "1 + (49)/100"),
        ("2*4%", "2*(4)/100"),
        ("1,500%", "(1500)/100"),
        ("0.5%", "(0.5)/100"),
    ];
    for (input, want) in pins {
        assert_eq!(normalize(input).source, want, "source of {input:?}");
    }
    assert_eq!(value("3 + 4%"), value("3.04"));
    assert_ne!(value("3 + 4%"), value("7/100"));
    assert_ne!(value("1 + 49%"), value("1/2"));
    assert_eq!(value("50%"), value("1/2"));
}

#[test]
fn a_vulgar_fraction_after_a_digit_run_is_a_mixed_number() {
    // Review findings #1 and #9. The old reading made `3½` the product `3*(1/2)`,
    // so a learner who wrote three and a half was correct against `1.5`.
    let pins: [(&str, &str); 6] = [
        ("½", "(1/2)"),
        ("3½", "3 1/2"),
        ("2⅓", "2 1/3"),
        ("5¾", "5 3/4"),
        ("x½", "x(1/2)"),
        ("(2)½", "(2)(1/2)"),
    ];
    for (input, want) in pins {
        assert_eq!(normalize(input).source, want, "source of {input:?}");
    }
    assert_eq!(value("3½"), value("7/2"));
    assert_eq!(value("2⅓"), value("7/3"));
    assert_ne!(value("2⅓"), value("2/3"));
    assert_ne!(value("3½"), value("1.5"));
    assert_eq!(value("3½"), value("3 1/2"));
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
    assert_eq!(
        ast("-3 1/2"),
        Ast::Mixed {
            whole: BigInt::from(-3),
            numerator: BigInt::from(1),
            denominator: BigInt::from(2)
        }
    );
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
    assert_eq!(
        ast("15 sqrt 3"),
        Ast::Mul(vec![int(15), Ast::Func("sqrt".to_string(), vec![int(3)])])
    );
    assert_eq!(
        ast("15√3"),
        Ast::Mul(vec![int(15), Ast::Func("sqrt".to_string(), vec![int(3)])])
    );
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
    assert_eq!(
        ast("-3½"),
        Ast::Mixed {
            whole: BigInt::from(-3),
            numerator: BigInt::from(1),
            denominator: BigInt::from(2)
        }
    );
}

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
        Ast::Add(vec![
            Ast::Mul(vec![int(2), call("cos", Ast::Mul(vec![int(2), var("t")]))]),
            Ast::Mul(vec![
                Ast::Fraction {
                    numerator: BigInt::from(5),
                    denominator: BigInt::from(2)
                },
                call("sin", Ast::Mul(vec![int(2), var("t")])),
            ]),
        ])
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
        Ast::Div(Box::new(call("sqrt", int(2))), Box::new(int(2)))
    );
    assert_eq!(
        ast("cos 2*x"),
        Ast::Mul(vec![call("cos", int(2)), var("x")])
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

// ---------------------------------------------------------------------------
// V2 — everything outside the grammar is undecidable
// ---------------------------------------------------------------------------

#[test]
fn the_prose_class_never_parses() {
    for text in [
        "yes",
        "sey",
        "no",
        "even",
        "neve",
        "diverges",
        "DNE",
        "undefined",
        "all real numbers",
        "perpendicular",
        "18 degrees Celsius",
        "vertices",
        "sides",
        "true",
        "false",
        "prime",
        "binomial",
        "III",
    ] {
        assert!(
            parse(&normalize(text).source).is_err(),
            "{text:?} must stay undecidable (spec section 7.6)"
        );
    }
}

#[test]
fn out_of_grammar_shapes_never_parse() {
    // `xy` left this list in the M2 fix wave: a short run of variable letters is
    // the product `x*y` now. `a_letter_run_the_grammar_does_not_own_stays_
    // undecidable` holds the runs that stay outside the grammar.
    for text in [
        "9 R2",
        "23 R14",
        "x + 2 remainder 3",
        "log_b(x)",
        "n!",
        "3/0",
        "0/0",
        "∞",
        "-∞",
        "zoo",
        "-zoo",
        "6 ≤ ∫ ≤ 15",
        "2y · dy/dx",
        "5 <= 7, so it holds",
        "$3a_1 - a_2$",
        "",
        "   ",
        "$",
    ] {
        assert!(
            parse(&normalize(text).source).is_err(),
            "{text:?} must stay undecidable"
        );
    }
}

#[test]
fn the_evaluation_bounds_refuse_the_1_0_exponent_bombs() {
    assert_eq!(
        parse(&normalize("9^9^9").source).unwrap_err().reason,
        "a tower of powers"
    );
    assert_eq!(
        parse(&normalize("9**9**9**9").source).unwrap_err().reason,
        "a tower of powers"
    );
    assert_eq!(
        parse(&normalize("2**10000000").source).unwrap_err().reason,
        "an exponent outside the evaluation bound"
    );
    assert_eq!(
        parse(&normalize("(2)**(9999999)").source)
            .unwrap_err()
            .reason,
        "an exponent outside the evaluation bound"
    );
    assert!(parse(&normalize("2**1000").source).is_ok());
    assert!(parse(&normalize("2**1001").source).is_err());
}

#[test]
fn the_rce_payloads_of_1_0_never_parse() {
    for text in [
        "__import__('os').system('touch /tmp/_rce_marker_should_not_exist')",
        "exec(\"open('/tmp/_rce_marker_should_not_exist','w').write('x')\")",
        "eval(\"__import__('os').getenv('ANTHROPIC_API_KEY')\")",
        "print(open('.env.example').read())",
        "integrate(exp(-x**2),(x,0,oo))",
        "factorint(9)",
    ] {
        assert!(
            parse(&normalize(text).source).is_err(),
            "{text:?} must stay undecidable"
        );
    }
}

#[test]
fn the_input_cap_refuses_a_longer_answer() {
    let inside = "1".repeat(MAX_ANSWER_CHARS);
    assert_eq!(MAX_ANSWER_CHARS, 4_000);
    assert!(parse(&inside).is_ok());
    let outside = "1".repeat(MAX_ANSWER_CHARS + 1);
    assert_eq!(
        parse(&outside).unwrap_err().reason,
        "the answer is longer than the input cap"
    );
}

#[test]
fn deep_nesting_is_refused_and_never_overflows_the_stack() {
    let deep = format!("{}1{}", "(".repeat(1_500), ")".repeat(1_500));
    assert_eq!(
        parse(&deep).unwrap_err().reason,
        "the answer nests too deeply"
    );
    let shallow = format!("{}1{}", "(".repeat(10), ")".repeat(10));
    assert_eq!(parse(&shallow).unwrap(), int(1));
}

// ---------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------

/// The parse result of every corpus shape.
///
/// The literals are what this grammar decides. They differ from the estimate of
/// spec section 5 in five places, and `docs/reference/checker-1.0-spec.md` section
/// 8.2 already records that the estimate counts whole shape buckets:
///
/// - `interval_ineq` 29 parse. The M2 plan adds the interval production, so the
///   spec's residue of 34 shrinks to the 5 rows that are prose, a general
///   inequality, or the integral sign.
/// - `equation` 1 parses. `y = x` is the value `x` with the label `y`, which the
///   parser reads as `Ast::Assign` (review finding #2).
/// - `value_with_unit` 11 parse. `5 m/s`, `2x + h`, `60 km/h` and `2π cm^2` are
///   legal expressions over single-letter variables; the multi-letter unit `min`
///   holds an `i`, which no letter run splits on, so `7 L/min` still fails.
/// - `comma_list` 28 parse. 15 of the 43 rows are prose that carries a comma
///   (`slope 3, y-intercept -5`), so they belong to the section 7.6 class.
/// - `expression_symbolic` 632 and `expression_numeric` 228 parse. The 11 rows
///   the fix wave adds are the multi-letter runs (`3xy^2`, `$12xy$`, `4ab^3`) and
///   `50th`. The residue is the section 8.2 outlier set: a free or fractional
///   exponent, `log_b`, `dy/dx`, `n!`, `∞`, a label set, and a differential.
const SHAPE_COUNTS: [(&str, usize, usize); 15] = [
    ("comma_list", 28, 15),
    ("decimal", 128, 0),
    ("equation", 1, 0),
    ("expression_numeric", 228, 5),
    ("expression_symbolic", 632, 54),
    ("fraction", 350, 2),
    ("integer", 1622, 0),
    ("interval_ineq", 29, 5),
    ("mixed_number", 8, 0),
    ("ordered_tuple", 178, 0),
    ("other", 7, 0),
    ("prose_or_words", 0, 167),
    ("quotient_remainder", 0, 16),
    ("set_or_list", 5, 0),
    ("value_with_unit", 11, 1),
];

#[test]
fn the_corpus_splits_into_3227_parsed_and_265_undecidable_answers() {
    let rows = corpus();
    assert_eq!(rows.len(), 3_492, "corpus size");
    let mut parsed = 0_usize;
    let mut refused = 0_usize;
    for row in &rows {
        if parse(&normalize(&row.answer).source).is_ok() {
            parsed += 1;
        } else {
            refused += 1;
        }
    }
    assert_eq!(parsed, 3_227, "answers inside the grammar");
    assert_eq!(refused, 265, "answers outside the grammar");
}

#[test]
fn every_shape_gives_its_literal_parse_count() {
    let rows = corpus();
    for (shape, want_parsed, want_refused) in SHAPE_COUNTS {
        let mut parsed = 0_usize;
        let mut refused = 0_usize;
        for row in rows.iter().filter(|row| row.shape == shape) {
            if parse(&normalize(&row.answer).source).is_ok() {
                parsed += 1;
            } else {
                refused += 1;
            }
        }
        assert_eq!(parsed, want_parsed, "{shape}: parsed");
        assert_eq!(refused, want_refused, "{shape}: refused");
    }
}

#[test]
fn the_undecidable_answers_are_exactly_the_committed_fixture() {
    let measured: BTreeSet<Key> = corpus()
        .into_iter()
        .filter(|row| parse(&normalize(&row.answer).source).is_err())
        .map(|row| (row.topic_id, row.kp_id, row.exemplar_index, row.answer))
        .collect();
    let committed = committed_residue();
    let missing: Vec<&Key> = committed.difference(&measured).collect();
    let extra: Vec<&Key> = measured.difference(&committed).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "the residue moved: missing {missing:?}, extra {extra:?}"
    );
    assert_eq!(committed.len(), 265);
}

#[test]
fn no_answer_of_the_prose_class_claims_a_verdict() {
    let parsed: Vec<String> = corpus()
        .into_iter()
        .filter(|row| row.shape == "prose_or_words")
        .filter(|row| parse(&normalize(&row.answer).source).is_ok())
        .map(|row| row.answer)
        .collect();
    assert!(
        parsed.is_empty(),
        "C4: prose must never reach a deterministic verdict, but {parsed:?} parsed"
    );
}

// ---------------------------------------------------------------------------
// The checker never panics
// ---------------------------------------------------------------------------

/// A small deterministic generator. The fuzz needs no crate and no entropy source.
struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn ten_seconds_of_random_input_never_panics() {
    // The alphabet mixes the grammar's own characters with the glyphs of the V4
    // table, so the generator reaches the productions and not only the reject path.
    let alphabet: Vec<char> = "0123456789+-*/^()[]{},.<>= xyzabcnE\\$%!_'\"\
         πτ∞·−–≤≥θαβλ½⅓⅔¼¾°√²³⁴"
        .chars()
        .collect();
    let mut rng = Xorshift(0x2026_0826_4d32_5531);
    let start = std::time::Instant::now();
    let mut cases = 0_u64;
    while start.elapsed() < std::time::Duration::from_secs(10) {
        for _ in 0..1_000 {
            let length = (rng.next() % 80) as usize;
            let text: String = if rng.next().is_multiple_of(2) {
                (0..length)
                    .map(|_| {
                        let index = (rng.next() as usize) % alphabet.len();
                        alphabet.get(index).copied().unwrap_or('0')
                    })
                    .collect()
            } else {
                let bytes: Vec<u8> = (0..length).map(|_| (rng.next() % 256) as u8).collect();
                String::from_utf8_lossy(&bytes).into_owned()
            };
            let normalized = normalize(&text);
            let _ = parse(&normalized.source);
            let _ = parse(&text);
            cases += 1;
        }
    }
    assert!(cases > 1_000, "the fuzz ran {cases} cases");
}
