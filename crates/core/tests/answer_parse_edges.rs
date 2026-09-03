//! The refusal sites of the lexer and the parser, reached one by one (V2, C4).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use cadus_core::answer::{Ast, Undecidable, normalize, parse};
use num_bigint::BigInt;

/// The refusal of one source string.
fn refusal(text: &str) -> &'static str {
    match parse(text) {
        Ok(tree) => panic!("{text:?} parsed as {tree:?}"),
        Err(reason) => reason.reason,
    }
}

/// An integer literal node.
fn int(value: i64) -> Ast {
    Ast::Integer(BigInt::from(value))
}

/// A variable node.
fn var(name: &str) -> Ast {
    Ast::Var(name.to_string())
}

/// `k` bracket pairs around a `\frac`.
fn nested_groups(k: usize) -> String {
    format!("{}\\frac{{1}}{{2}}{}", "(".repeat(k), ")".repeat(k))
}

#[test]
fn braces_nested_past_the_lexer_bound_are_refused() {
    let roots = format!("{}1{}", "\\sqrt{".repeat(34), "}".repeat(34));
    assert_eq!(refusal(&roots), "the answer nests too deeply");
    let fractions = format!("{}1{}", "\\frac{".repeat(34), "}{1}".repeat(34));
    assert_eq!(refusal(&fractions), "the answer nests too deeply");
    let powers = format!("2{}1{}", "^{2^{".repeat(20), "}}".repeat(20));
    assert_eq!(refusal(&powers), "the answer nests too deeply");
}

#[test]
fn a_brace_body_carries_its_own_refusal() {
    assert_eq!(refusal("\\frac{§}{1}"), "a character outside the grammar");
    assert_eq!(refusal("\\frac{1}{§}"), "a character outside the grammar");
    assert_eq!(refusal("\\sqrt{§}"), "a character outside the grammar");
    assert_eq!(refusal("2^{§}"), "a character outside the grammar");
    assert_eq!(
        refusal("\\frac{1}{+}"),
        "the answer ends where a value belongs"
    );
}

#[test]
fn groups_nested_past_the_parser_bound_are_refused() {
    assert_eq!(
        parse(&nested_groups(21)),
        Ok(Ast::Fraction {
            numerator: BigInt::from(1),
            denominator: BigInt::from(2)
        })
    );
    // Level 22 refuses inside the brace body; level 23 refuses at the body itself.
    assert_eq!(refusal(&nested_groups(22)), "the answer nests too deeply");
    assert_eq!(refusal(&nested_groups(23)), "the answer nests too deeply");
}

#[test]
fn every_superscript_digit_lexes() {
    assert_eq!(parse("x⁵⁶⁷"), Ok(Ast::Pow(Box::new(var("x")), 567)));
    assert_eq!(parse("2⁰"), Ok(Ast::Pow(Box::new(int(2)), 0)));
    assert_eq!(parse("3⁸⁹"), Ok(Ast::Pow(Box::new(int(3)), 89)));
    assert_eq!(
        parse("y¹²³⁴"),
        Err(Undecidable::new("an exponent outside the evaluation bound"))
    );
    assert_eq!(parse("y⁴"), Ok(Ast::Pow(Box::new(var("y")), 4)));
}

#[test]
fn the_sharp_s_casefolds_to_a_double_s() {
    assert_eq!(normalize("Straße").string_key, "strasse");
    assert_eq!(normalize("ẞ").string_key, "ss");
}

#[test]
fn a_fraction_body_that_is_no_plain_digit_run_makes_no_mixed_number() {
    assert_eq!(
        refusal("2\\frac{x}{2}"),
        "a mixed number whose fraction is not proper"
    );
    assert_eq!(
        refusal("2\\frac{01}{2}"),
        "a mixed number whose fraction is not proper"
    );
    // A three-digit numerator after a space is a thousands group, and the
    // digit-run spelling hands the answer back: two numbers then stand side by side.
    assert_eq!(refusal("1 200/300"), "two numbers stand side by side");
}

#[test]
fn an_operator_with_no_operand_after_it_is_refused() {
    assert_eq!(refusal("1*"), "the answer ends where a value belongs");
    assert_eq!(refusal("1/"), "the answer ends where a value belongs");
    assert_eq!(refusal("2 1/2%%"), "two percent signs on one number");
    assert_eq!(refusal("3xy%%"), "two percent signs on one number");
}

#[test]
fn an_exponent_too_long_for_a_machine_word_is_refused() {
    assert_eq!(
        refusal("2^99999999999999999999"),
        "an exponent outside the evaluation bound"
    );
}

#[test]
fn a_letter_run_splits_under_a_root_glyph_and_takes_a_percent() {
    assert_eq!(parse("√xy^2"), parse("sqrt(x*y)^2"));
    assert_eq!(
        parse("3xy%"),
        Ok(Ast::Mul(vec![
            int(3),
            Ast::Mul(vec![
                var("x"),
                Ast::Div(Box::new(var("y")), Box::new(int(100)))
            ])
        ]))
    );
}

#[test]
fn the_reader_refuses_a_group_with_three_ends() {
    assert_eq!(refusal("(1, 2, 3]"), "an interval that has no two ends");
    assert_eq!(refusal("[1, 2, 3)"), "an interval that has no two ends");
    assert_eq!(
        parse("(1, 2, 3)"),
        Ok(Ast::Tuple(vec![int(1), int(2), int(3)]))
    );
}

#[test]
fn the_refusal_type_carries_its_reason() {
    assert_eq!(
        Undecidable::new("a reason").to_string(),
        "undecidable answer: a reason"
    );
}

#[test]
fn a_refusal_inside_a_juxtaposed_argument_and_a_root_glyph_reaches_the_top() {
    assert_eq!(
        refusal("6 x 10^y"),
        "an exponent that is not a whole number"
    );
    assert_eq!(
        refusal("sin 2 x 3^y"),
        "an exponent that is not a whole number"
    );
    assert_eq!(
        refusal("sin 2\\frac{3}{2}"),
        "a mixed number whose fraction is not proper"
    );
    assert_eq!(
        refusal("√\\frac{1}{+}"),
        "the answer ends where a value belongs"
    );
}
