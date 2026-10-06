//! Lane B3: the `arc` function names and the bar pair `|a|` (freeze pack, `rust-api.md` section 3).
//!
//! `arctan(a)`, `arcsin(a)`, and `arccos(a)` give the tree of `atan(a)`, `asin(a)`,
//! and `acos(a)`. `|a|` gives the tree of `abs(a)` when the full answer has exactly
//! two bars. Each other count of bars keeps the refusal of the base commit
//! `d2ca1421`, and each text that the base reads keeps its tree.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_core::answer::{
    AnswerContract, Outcome, canonical_form, check_contract, lexer::lex, parse,
};

/// The refusal that the base gives to each text with a bar.
const BAR_REFUSAL: &str = "a character outside the grammar";

/// The refusal that the base gives to a name outside the grammar.
const NAME_REFUSAL: &str = "a name that is not a function or variable";

/// The pairs of a second spelling and its first spelling.
const SAME_TREE: [(&str, &str); 22] = [
    ("arctan(x/2)", "atan(x/2)"),
    ("arcsin(x/2)", "asin(x/2)"),
    ("arccos(x/2)", "acos(x/2)"),
    ("arctan x", "atan x"),
    ("2 arcsin(x) + arccos(x)^2", "2 asin(x) + acos(x)^2"),
    ("\\arctan(x)", "atan(x)"),
    ("arctan^2 x", "atan^2 x"),
    ("√arctan(x)", "√atan(x)"),
    ("|x+1|", "abs(x+1)"),
    ("|x|", "abs(x)"),
    ("|x| + 1", "abs(x) + 1"),
    ("ln|x+1|", "ln(abs(x+1))"),
    ("ln |x+1| + C", "ln(abs(x+1)) + C"),
    ("sqrt(|x|)", "sqrt(abs(x))"),
    ("2|x|", "2*abs(x)"),
    ("2 |x|", "2 abs(x)"),
    ("|x|^2", "abs(x)^2"),
    ("|x|²", "abs(x)^2"),
    ("|x|y", "abs(x) y"),
    ("\\frac{|x|}{2}", "\\frac{abs(x)}{2}"),
    ("|(x+1)(x-2)|", "abs((x+1)(x-2))"),
    ("x^2/2 - x + ln|x+1| + C", "x^2/2 - x + ln(abs(x+1)) + C"),
];

#[test]
fn a_second_spelling_gives_the_tree_of_the_first_spelling() {
    for (second, first) in SAME_TREE {
        let expected = parse(first);
        assert_eq!(parse(second), expected, "{second} against {first}");
    }
    // `√` takes no function name, so the refusal is the same for the two names.
    assert!(parse("√arctan(x)").is_err());
}

#[test]
fn the_trees_of_the_second_spellings_are_the_frozen_trees() {
    assert_eq!(
        format!("{:?}", parse("arctan(x/2)").unwrap()),
        r#"Func("atan", [Div(Var("x"), Integer(2))])"#
    );
    assert_eq!(
        format!("{:?}", parse("|x+1|").unwrap()),
        r#"Func("abs", [Add([Var("x"), Integer(1)])])"#
    );
    assert_eq!(
        format!("{:?}", parse("2|x|").unwrap()),
        r#"Mul([Integer(2), Func("abs", [Var("x")])])"#
    );
    // The decision for a power: the power binds to the bar pair, as it binds
    // to `abs(x)` in `abs(x)^2`.
    assert_eq!(
        format!("{:?}", parse("|x|^2").unwrap()),
        r#"Pow(Func("abs", [Var("x")]), 2)"#
    );
}

#[test]
fn the_canonical_form_is_the_same_for_the_two_spellings() {
    for (second, first) in [
        ("arctan(x/2)", "atan(x/2)"),
        ("arcsin(x)", "asin(x)"),
        ("arccos(x)", "acos(x)"),
        ("|x+1|", "abs(x+1)"),
        ("ln|x+1|", "ln(abs(x+1))"),
        ("2|x|", "2*abs(x)"),
        ("|x|^2", "abs(x)^2"),
    ] {
        let expected = canonical_form(first).expect(first);
        assert_eq!(canonical_form(second).expect(second), expected, "{second}");
    }
}

/// Whether the exact contract marks `learner` correct against `expected`.
fn exact_is_correct(expected: &str, learner: &str) -> bool {
    match check_contract(expected, learner, AnswerContract::Exact) {
        Outcome::Decided(verdict) => verdict.correct,
        Outcome::Undecidable(refusal) => panic!("{expected} / {learner}: {}", refusal.reason),
    }
}

#[test]
fn the_exact_contract_accepts_the_second_spellings() {
    for (expected, learner) in [
        ("atan(x)", "arctan(x)"),
        ("asin(x)", "arcsin(x)"),
        ("acos(x)", "arccos(x)"),
        ("arctan(x/2)", "atan(x/2)"),
        ("abs(x+1)", "|x+1|"),
        ("x^2/2 - x + ln(abs(x+1)) + C", "x^2/2 - x + ln|x+1| + C"),
        ("2*abs(x)", "2|x|"),
        ("abs(x)^2", "|x|^2"),
    ] {
        assert!(
            exact_is_correct(expected, learner),
            "{expected} / {learner}"
        );
    }
    // The two corpus answers that `recovered_2_0.jsonl` names for the
    // `arc_function_name` production. The 1.0 checker read `arctan x` as a
    // product of letters, so the verdicts of the two answers are here.
    for (expected, learner, correct) in [
        ("arctan x + x/(1 + x^2)", "arctan x + x/(1 + x^2)", true),
        ("arctan x + x/(1 + x^2)", "x/(x^2 + 1) + atan(x)", true),
        ("arctan x + x/(1 + x^2)", "arctan x - x/(1 + x^2)", false),
        (
            "arctan(2x) + 2x/(1 + 4x^2)",
            "2x/(1 + 4x^2) + atan(2x)",
            true,
        ),
        (
            "arctan(2x) + 2x/(1 + 4x^2)",
            "arctan(x) + 2x/(1 + 4x^2)",
            false,
        ),
    ] {
        assert_eq!(
            exact_is_correct(expected, learner),
            correct,
            "{expected} / {learner}"
        );
    }
    assert!(!exact_is_correct("atan(x)", "arcsin(x)"));
    assert!(!exact_is_correct("abs(x+1)", "|x-1|"));
}

/// The texts that stay outside the grammar, with the reason of each.
const REFUSED: [(&str, &str); 17] = [
    ("|x| + |y|", BAR_REFUSAL),
    ("|x", BAR_REFUSAL),
    ("x|", BAR_REFUSAL),
    ("|", BAR_REFUSAL),
    ("||x||", BAR_REFUSAL),
    ("|x||", BAR_REFUSAL),
    ("|||", BAR_REFUSAL),
    // The two bars stand in two runs.
    ("\\frac{|x}{|y}", BAR_REFUSAL),
    // A bracket of the body closes outside the body.
    ("|(x|)", BAR_REFUSAL),
    ("(|x)|", BAR_REFUSAL),
    ("|x)+(y|", BAR_REFUSAL),
    // The body is the argument list of `abs`, so these get the refusals of
    // `abs()`, `abs(x, y)`, and `abs(1.2.3)`.
    ("||", "a function call with the wrong count of arguments"),
    (
        "|x, y|",
        "a function call with the wrong count of arguments",
    ),
    ("|1.2.3|", "a number with two points"),
    ("arctanh(x)", NAME_REFUSAL),
    ("arcsec(x)", NAME_REFUSAL),
    ("Arctan(x)", NAME_REFUSAL),
];

#[test]
fn each_other_shape_keeps_its_refusal() {
    for (text, reason) in REFUSED {
        let refusal = parse(text).expect_err(text);
        assert_eq!(refusal.reason, reason, "{text}");
        assert!(canonical_form(text).is_err(), "{text}");
    }
    for text in ["|x| + |y|", "|x", "||x||", "arctanh(x)", "arcsec(x)"] {
        let outcome = check_contract("x", text, AnswerContract::Exact);
        assert!(matches!(outcome, Outcome::Undecidable(_)), "{text}");
    }
}

#[test]
fn no_mix_of_bars_and_brackets_panics() {
    let pieces = [
        "|", "(", ")", "[", "{", "}", "x", "2", "^", "\\frac{", " ", "arctan", ",",
    ];
    for first in pieces {
        for second in pieces {
            for third in pieces {
                for fourth in pieces {
                    let text = format!("{first}{second}{third}{fourth}");
                    // The result is a tree or a refusal. The two are correct here.
                    let _ = lex(&text);
                    let _ = parse(&text);
                    let _ = parse(&format!("|{text}|"));
                }
            }
        }
    }
}

#[test]
fn the_token_reader_keeps_each_old_construct() {
    // The bar flag goes through each nested run, so each construct of the
    // token reader runs here with and without a bar pair.
    for (text, same) in [
        ("|\\frac{x}{2}|", "abs(x/2)"),
        ("|\\sqrt{x}|", "abs(sqrt(x))"),
        ("|x^{3}|", "abs(x^3)"),
        ("|2 \\cdot x|", "abs(2*x)"),
        ("|2 \\times x|", "abs(2*x)"),
        ("\\left|x\\right|", "abs(x)"),
        ("|½ x|", "abs(½ x)"),
        ("|√x|", "abs(sqrt(x))"),
        ("|x²|", "abs(x^2)"),
        ("|50%|", "abs(50%)"),
        ("\\sqrt{|x|}", "sqrt(abs(x))"),
        ("2^{|x|}", "2^(abs(x))"),
    ] {
        assert_eq!(parse(text), parse(same), "{text} against {same}");
    }
    assert!(parse("|\\pi|").is_ok());
    for text in ["|30°|", "|x ²|", "|x # y|", "|\\frac{1}{1.2.3}|"] {
        assert!(parse(text).is_err(), "{text}");
    }
    let deep = format!("|{}x{}|", "\\sqrt{".repeat(40), "}".repeat(40));
    assert_eq!(
        parse(&deep).expect_err("deep").reason,
        "the answer nests too deeply"
    );
}

/// Read a contract from its authored JSON text.
fn contract(json: &str) -> AnswerContract {
    serde_json::from_str(json).expect(json)
}

#[test]
fn a_split_contract_counts_the_bars_of_the_full_answer() {
    let refused_with = |expected: &str, learner: &str, json: &str, reason: &str| {
        let outcome = check_contract(expected, learner, contract(json));
        let Outcome::Undecidable(refusal) = outcome else {
            panic!("{learner} under {json}: {outcome:?}");
        };
        assert_eq!(refusal.reason, reason, "{learner} under {json}");
    };
    let refused = |expected: &str, learner: &str, json: &str| {
        refused_with(expected, learner, json, BAR_REFUSAL);
    };
    let ordered = r#"{"kind":"list","ordered":true,"member":{"kind":"exact"}}"#;
    let unordered = r#"{"kind":"list","ordered":false,"member":{"kind":"exact"}}"#;
    let parts = r#"{"kind":"multipart","parts":[{"name":"a","contract":{"kind":"exact"}},{"name":"b","contract":{"kind":"exact"}}]}"#;
    for list in [ordered, unordered] {
        // Four bars in the full answer: the refusal of the base.
        refused("1, 3", "|-1|, |-3|", list);
        refused("|-1|, |-3|", "1, 3", list);
        refused("1, 3", "|1|, |3|, 5", list);
        refused("1, 3", "|1|, 3|", list);
        // A text that is not a complete list keeps the refusal of the split.
        refused_with(
            "1, 3",
            "|1|, |3|,",
            list,
            "a list requires one to 32 complete members",
        );
    }
    refused("{1, 3}", "{|-1|, |-3|}", r#"{"kind":"set"}"#);
    refused(
        "(1, 3)",
        "(|-1|, |-3|)",
        r#"{"kind":"coordinates","arity":2}"#,
    );
    // The comma-named multipart reading reaches each part, and a part with a bar
    // pair is never correct against a key without the bars.
    let outcome = check_contract("a = 1, b = 3", "a = |-1|, b = |-3|", contract(parts));
    assert!(
        !matches!(outcome, Outcome::Decided(verdict) if verdict.correct),
        "{outcome:?}"
    );
    // Two bars in the full answer: one member has the bar pair.
    for (learner, correct) in [("|-1|, 3", false), ("abs(1), 3", false), ("1, 3", true)] {
        let outcome = check_contract("1, 3", learner, contract(ordered));
        let Outcome::Decided(verdict) = outcome else {
            panic!("{learner}: {outcome:?}");
        };
        assert_eq!(verdict.correct, correct, "{learner}");
    }
    assert!(exact_list_is_correct("abs(x), 3", "|x|, 3", ordered));
    assert!(exact_list_is_correct("3, abs(x)", "|x|, 3", unordered));
    // A label member does not go to the lexer, so a label list keeps its bars.
    let labels =
        r#"{"kind":"list","ordered":true,"member":{"kind":"label","options":[["A|B"],["no"]]}}"#;
    assert!(exact_list_is_correct(
        "A|B, no, A|B",
        "A|B, no, A|B",
        labels
    ));
    assert!(exact_list_is_correct(
        "A|B, A|B, A|B",
        "A|B, A|B, A|B",
        labels
    ));
}

/// Whether the list contract of `json` marks `learner` correct.
fn exact_list_is_correct(expected: &str, learner: &str, json: &str) -> bool {
    match check_contract(expected, learner, contract(json)) {
        Outcome::Decided(verdict) => verdict.correct,
        Outcome::Undecidable(refusal) => panic!("{expected} / {learner}: {}", refusal.reason),
    }
}

/// Answers of the shipped curriculum and of fact F8, with the tree of each at the
/// base commit `d2ca1421` (`format!("{:?}")`, recorded before the change of this lane).
const BASE_TREES: [(&str, &str); 48] = [
    (
        "6x cos(3x^2)",
        r#"Mul([Integer(6), Var("x"), Func("cos", [Mul([Integer(3), Pow(Var("x"), 2)])])])"#,
    ),
    (
        "-2e^(-2x)",
        r#"Mul([Neg(Integer(2)), Func("exp", [Mul([Neg(Integer(2)), Var("x")])])])"#,
    ),
    (
        "x/sqrt(x^2+9)",
        r#"Div(Var("x"), Sqrt(Add([Pow(Var("x"), 2), Integer(9)])))"#,
    ),
    (
        "(1/2)e^(-t) sin(2t)",
        r#"Mul([Fraction { numerator: 1, denominator: 2 }, Func("exp", [Neg(Var("t"))]), Func("sin", [Mul([Integer(2), Var("t")])])])"#,
    ),
    (
        "x^2/2 - x + ln(abs(x+1)) + C",
        r#"Add([Div(Pow(Var("x"), 2), Integer(2)), Neg(Var("x")), Func("ln", [Func("abs", [Add([Var("x"), Integer(1)])])]), Var("C")])"#,
    ),
    ("{2, 3}", r#"Set([Integer(2), Integer(3)])"#),
    (
        "-1 < x <= 1",
        r#"Chain { lo: Neg(Integer(1)), lo_closed: false, var: "x", hi_closed: true, hi: Integer(1) }"#,
    ),
    ("(46, 54)", r#"Tuple([Integer(46), Integer(54)])"#),
    ("sin(2x)", r#"Func("sin", [Mul([Integer(2), Var("x")])])"#),
    (
        "2 sin(x) cos(x)",
        r#"Mul([Integer(2), Func("sin", [Var("x")]), Func("cos", [Var("x")])])"#,
    ),
    ("tan(x)", r#"Func("tan", [Var("x")])"#),
    (
        "sin(x)/cos(x)",
        r#"Div(Func("sin", [Var("x")]), Func("cos", [Var("x")]))"#,
    ),
    ("sec(x)^2", r#"Pow(Func("sec", [Var("x")]), 2)"#),
    (
        "1/cos(x)^2",
        r#"Div(Integer(1), Pow(Func("cos", [Var("x")]), 2))"#,
    ),
    (
        "1 + tan(x)^2",
        r#"Add([Integer(1), Pow(Func("tan", [Var("x")]), 2)])"#,
    ),
    (
        "(x+2)/(x+1)^2",
        r#"Div(Add([Var("x"), Integer(2)]), Pow(Add([Var("x"), Integer(1)]), 2))"#,
    ),
    (
        "1/(x+1) + 1/(x+1)^2",
        r#"Add([Div(Integer(1), Add([Var("x"), Integer(1)])), Div(Integer(1), Pow(Add([Var("x"), Integer(1)]), 2))])"#,
    ),
    (
        "(x^2-1)/(x-1)",
        r#"Div(Add([Pow(Var("x"), 2), Neg(Integer(1))]), Add([Var("x"), Neg(Integer(1))]))"#,
    ),
    ("ln(x^2)", r#"Func("ln", [Pow(Var("x"), 2)])"#),
    ("2 ln(x)", r#"Mul([Integer(2), Func("ln", [Var("x")])])"#),
    ("0.3679", r#"Decimal { mantissa: 3679, scale: 4 }"#),
    ("e^(-1)", r#"Func("exp", [Neg(Integer(1))])"#),
    (
        "e^x (x-1)",
        r#"Mul([Func("exp", [Var("x")]), Add([Var("x"), Neg(Integer(1))])])"#,
    ),
    (
        "x*e^x - e^x",
        r#"Add([Mul([Var("x"), Func("exp", [Var("x")])]), Neg(Func("exp", [Var("x")]))])"#,
    ),
    (
        "sin(2t)/(2e^t)",
        r#"Div(Func("sin", [Mul([Integer(2), Var("t")])]), Mul([Integer(2), Func("exp", [Var("t")])]))"#,
    ),
    (
        "(x^2+1)^(1/2)",
        r#"RationalPow { base: Add([Pow(Var("x"), 2), Integer(1)]), numerator: 1, denominator: 2 }"#,
    ),
    (
        "sqrt(x^2+1)",
        r#"Sqrt(Add([Pow(Var("x"), 2), Integer(1)]))"#,
    ),
    (
        "x^(-1/2)",
        r#"RationalPow { base: Var("x"), numerator: -1, denominator: 2 }"#,
    ),
    ("1/sqrt(x)", r#"Div(Integer(1), Sqrt(Var("x")))"#),
    (
        "y = 3x + 2",
        r#"Assign { var: "y", value: Add([Mul([Integer(3), Var("x")]), Integer(2)]) }"#,
    ),
    ("atan(x/2)", r#"Func("atan", [Div(Var("x"), Integer(2))])"#),
    (
        "asin(x) + acos(x)",
        r#"Add([Func("asin", [Var("x")]), Func("acos", [Var("x")])])"#,
    ),
    ("abs(x+1)", r#"Func("abs", [Add([Var("x"), Integer(1)])])"#),
    ("2 abs(x)", r#"Mul([Integer(2), Func("abs", [Var("x")])])"#),
    ("abs(x)^2", r#"Pow(Func("abs", [Var("x")]), 2)"#),
    ("sec^2 x", r#"Pow(Func("sec", [Var("x")]), 2)"#),
    (
        "sec x tan x",
        r#"Mul([Func("sec", [Var("x")]), Func("tan", [Var("x")])])"#,
    ),
    (
        "\\frac{1}{2}",
        r#"Fraction { numerator: 1, denominator: 2 }"#,
    ),
    (
        "2\\frac{1}{2}",
        r#"Mixed { whole: 2, numerator: 1, denominator: 2 }"#,
    ),
    (
        "3 1/2",
        r#"Mixed { whole: 3, numerator: 1, denominator: 2 }"#,
    ),
    ("15√3", r#"Mul([Integer(15), Sqrt(Integer(3))])"#),
    ("50%", r#"Fraction { numerator: 50, denominator: 100 }"#),
    ("1,500", r#"Tuple([Integer(1500)])"#),
    (
        "[2, 5)",
        r#"Interval { lo: Integer(2), hi: Integer(5), lo_closed: true, hi_closed: false }"#,
    ),
    ("x >= 4", r#"Ineq { var: "x", op: Ge, bound: Integer(4) }"#),
    ("9 R2", r#"Tuple([Integer(9), Integer(2)])"#),
    ("6 x 10^3", r#"Mul([Integer(6), Pow(Integer(10), 3)])"#),
    ("log(8, 2)", r#"Func("log", [Integer(8), Integer(2)])"#),
];

#[test]
fn each_text_that_the_base_reads_keeps_its_tree() {
    for (text, tree) in BASE_TREES {
        assert_eq!(format!("{:?}", parse(text).expect(text)), tree, "{text}");
    }
}
