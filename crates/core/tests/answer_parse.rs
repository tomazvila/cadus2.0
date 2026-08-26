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

/// Build a square-root node.
///
/// Review round 3 makes every root one node, so `sqrt(2)`, `\sqrt{2}`,
/// `\sqrt 2` and `√2` all build `Ast::Sqrt`.
fn root(argument: Ast) -> Ast {
    Ast::Sqrt(Box::new(argument))
}

/// Build a literal fraction node.
fn frac(numerator: i64, denominator: i64) -> Ast {
    Ast::Fraction {
        numerator: BigInt::from(numerator),
        denominator: BigInt::from(denominator),
    }
}

/// Build a mixed-number node with a non-negative whole part.
fn mixed(whole: i64, numerator: i64, denominator: i64) -> Ast {
    Ast::Mixed {
        whole: BigInt::from(whole),
        numerator: BigInt::from(numerator),
        denominator: BigInt::from(denominator),
    }
}

/// The refusal reason of an answer the grammar does not read.
fn refusal(text: &str) -> &'static str {
    match parse(&normalize(text).source) {
        Ok(ast) => panic!("{text:?} parsed to {ast:?}"),
        Err(refused) => refused.reason,
    }
}

/// Canonicalize one answer, and fail the test when the grammar refuses it.
fn value(text: &str) -> Canon {
    canonical_form(text).unwrap_or_else(|e| panic!("{text:?}: {}", e.reason))
}

// ---------------------------------------------------------------------------
// V4 — normalization
// ---------------------------------------------------------------------------

#[test]
fn the_string_steps_of_the_v4_table_give_the_literal_source() {
    // Restated in the M2 fix wave of review round 3. `normalize` now writes the
    // whole-string steps of the V4 table and nothing else: the outer `$…$`, the
    // trailing period, the whitespace collapse, the thousands group on a full
    // match, and the one-character Unicode table. Every construct — `\frac`,
    // `\sqrt`, `^{n}`, `%`, `√`, a vulgar glyph, a superscript run, `°` — is a
    // token of the lexer, so the source carries it unchanged and the tree below
    // is what the reader makes of it (the structural ruling of round 3).
    let pins: [(&str, &str); 8] = [
        ("7,329.", "7329"),
        ("$7,329$", "7329"),
        ("7\u{00a0}329", "7329"),
        ("15√3", "15√3"),
        ("x²+1", "x²+1"),
        ("30°", "30°"),
        ("½", "½"),
        ("1 + 2x.", "1 + 2x"),
    ];
    for (input, want) in pins {
        assert_eq!(normalize(input).source, want, "source of {input:?}");
    }
    // The value of every pinned pair of spec section 6.1 is unchanged.
    assert_eq!(ast("7,329."), int(7329));
    assert_eq!(ast("$7,329$"), int(7329));
    assert_eq!(ast("7\u{00a0}329"), int(7329));
    assert_eq!(ast("15√3"), Ast::Mul(vec![int(15), root(int(3))]));
    assert_eq!(
        ast("x²+1"),
        Ast::Add(vec![Ast::Pow(Box::new(var("x")), 2), int(1)])
    );
    assert_eq!(ast("30°"), int(30));
    assert_eq!(ast("½"), frac(1, 2));
    assert_eq!(
        ast("1 + 2x."),
        Ast::Add(vec![int(1), Ast::Mul(vec![int(2), var("x")])])
    );
}

#[test]
fn the_2_0_additions_of_the_v4_table_are_tokens_with_a_literal_tree() {
    // Restated in the M2 fix wave of review round 3. Round 2 pinned the source
    // string of each addition, and four C4 false positives came out of those
    // string rewrites (findings #1, #2, #3, #4, #6, #8). The addition is a token
    // now, so this test pins the tree, which is what the checker compares on.
    assert_eq!(ast("\\frac{1}{2}"), frac(1, 2));
    assert_eq!(ast("2\\frac{1}{2}"), mixed(2, 1, 2));
    assert_eq!(ast("2 \\frac{1}{2}"), mixed(2, 1, 2));
    // A `\frac` whose braces hold an expression is the quotient of the two.
    assert_eq!(
        ast("\\frac{x+1}{2}"),
        Ast::Div(Box::new(Ast::Add(vec![var("x"), int(1)])), Box::new(int(2)))
    );
    assert_eq!(ast("\\sqrt{2}"), root(int(2)));
    assert_eq!(ast("\\sqrt 2"), root(int(2)));
    assert_eq!(
        ast("5x\\sqrt{2}"),
        Ast::Mul(vec![int(5), var("x"), root(int(2))])
    );
    assert_eq!(ast("x^{2}"), Ast::Pow(Box::new(var("x")), 2));
    assert_eq!(ast("50%"), frac(50, 100));
    // The label stays in the source. The parser reads it, and `check`
    // compares the two labels (review findings #2, #10, #16).
    assert_eq!(normalize("x = 5").source, "x = 5");
    assert_eq!(ast("2\\cdot 3"), Ast::Mul(vec![int(2), int(3)]));
    assert_eq!(ast("6\\times 7"), Ast::Mul(vec![int(6), int(7)]));
    assert_eq!(ast("8÷2"), frac(8, 2));
    assert_eq!(ast("\\left(x\\right)"), var("x"));
    assert_eq!(normalize("-1 ≤ x ≤ 3").source, "-1 <= x <= 3");
    assert_eq!(
        ast("2√3/3"),
        Ast::Div(
            Box::new(Ast::Mul(vec![int(2), root(int(3))])),
            Box::new(int(3))
        )
    );
    assert_eq!(
        ast("x/√(x^2 + 9)"),
        Ast::Div(
            Box::new(var("x")),
            Box::new(root(Ast::Add(vec![
                Ast::Pow(Box::new(var("x")), 2),
                int(9)
            ])))
        )
    );
}

#[test]
fn the_unicode_table_of_1_0_gives_the_literal_source() {
    // The one-character operator and constant glyphs stay in `normalize`. The
    // glyphs that carry an argument left the table for the lexer in review
    // round 3, so their tree is pinned below the table.
    let pins: [(&str, &str); 7] = [
        ("π", "pi"),
        ("τ", "(2*pi)"),
        ("∞", "oo"),
        ("2·3", "2*3"),
        ("−5", "-5"),
        ("–5", "-5"),
        ("θ", "theta"),
    ];
    for (input, want) in pins {
        assert_eq!(normalize(input).source, want, "source of {input:?}");
    }
    assert_eq!(ast("⅓"), frac(1, 3));
    assert_eq!(ast("¾"), frac(3, 4));
    assert_eq!(ast("2³"), Ast::Pow(Box::new(int(2)), 3));
    assert_eq!(ast("2·3"), Ast::Mul(vec![int(2), int(3)]));
    assert_eq!(ast("−5"), Ast::Neg(Box::new(int(5))));
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
fn a_percent_binds_to_the_primary_in_front_of_it_and_to_nothing_else() {
    // Review round 3, findings #3, #4 and #6, restating the round 1 finding #17
    // pins. Round 2 spliced the text `(n)/100` into the source, so the `/100`
    // bound to the operator beside it: `15/30%` became `(15/30)/100` = 1/200,
    // which refused the right answer 50 on the topic that teaches that division
    // and graded the learner who wrote 50 correct against an authored 1/200
    // (C4). The percent is a postfix token now, and the node it builds holds one
    // primary.
    //
    // The value under every operator that can steal the divisor.
    assert_eq!(value("50%"), value("1/2"));
    assert_eq!(value("3 + 4%"), value("3.04"));
    assert_eq!(value("3 - 4%"), value("2.96"));
    assert_eq!(value("1 + 49%"), value("1.49"));
    assert_eq!(value("2*4%"), value("0.08"));
    assert_eq!(value("0.5%"), value("1/200"));
    // The divisor case is the one the round 2 rewrite broke.
    assert_eq!(value("15/30%"), value("50"));
    assert_eq!(value("36/45%"), value("80"));
    assert_eq!(value("8/5%"), value("160"));
    assert_eq!(value("1/4%"), value("25"));
    assert_eq!(value("3 / 4%"), value("75"));
    assert_eq!(value("1/(50%)"), value("2"));
    // The power case. The percent binds tighter than `^`, so the base carries it.
    assert_eq!(value("4%^2"), value("0.0016"));
    assert_eq!(value("4%**2"), value("0.0016"));
    // C4: every wrong value the round 2 reading invented stays wrong.
    assert_ne!(value("15/30%"), value("1/200"));
    assert_ne!(value("1/4%"), value("0.0025"));
    assert_ne!(value("3 / 4%"), value("3/400"));
    assert_ne!(value("4%^2"), value("0.0004"));
    assert_ne!(value("3 + 4%"), value("7/100"));
    assert_ne!(value("1 + 49%"), value("1/2"));
    assert_ne!(value("2*4%"), value("8"));
    // The neighbors. A percent takes the primary in front of it, whatever that
    // primary is, and it takes exactly one primary.
    assert_eq!(value("(2+2)%"), value("0.04"));
    assert_eq!(value("sqrt(4)%"), value("0.02"));
    assert_eq!(value("50% + 50%"), value("1"));
    assert_eq!(value("sin 50%"), value("sin(1/2)"));
    assert_eq!(value("x%"), value("x/100"));
    assert_eq!(value("pi%"), value("pi/100"));
    assert_ne!(value("(2+2)%"), value("2 + 2/100"));
    assert_ne!(value("sqrt(4)%"), value("sqrt(4/100)"));
    // An exponent of `50%` is a half, and the grammar holds a whole exponent
    // only, so the answer gets no verdict instead of a second reading.
    assert_eq!(
        refusal("2^50%"),
        "an exponent that is not a whole number",
        "2^50% has no whole exponent"
    );
    assert_eq!(refusal("50%%"), "two percent signs on one number");
    assert_eq!(refusal("%"), "a symbol where a value belongs");
    assert_eq!(refusal("%50"), "a symbol where a value belongs");
}

#[test]
fn a_thousands_group_in_front_of_a_percent_is_undecidable() {
    // Review round 3, finding #6. Round 2 pulled the trailing `%` off before the
    // thousands step, so `1 500%` read the group and became 15 while
    // `3 + 1 500%` split it and became 8: one string, two readings, and the
    // split one graded a learner who wrote 18 correct against an authored 8
    // (C4). The V4 table reads a group on a full match of the whole answer, and
    // an answer that carries a `%` is not that group.
    for text in ["1 500%", "3 + 1 500%", "2 + 1 000%", "x/1 500%"] {
        let reason = refusal(text);
        assert!(
            reason == "two numbers stand side by side"
                || reason == "a space-grouped number stands after a factor",
            "{text:?} gave {reason:?}"
        );
    }
    // The comma separator gets the same answer, so the two separators of the V4
    // table read alike. Round 2 gave `1,500%` the value 15 and `1 500%` the same
    // 15, while `3 + 1,500` was the tuple `(4, 500)`.
    for text in ["1,500%", "3 + 1,500", "1,500 + 3"] {
        assert_eq!(
            refusal(text),
            "a comma-grouped number stands in a longer answer",
            "{text:?} takes no reading"
        );
    }
    // The full-match rule still reads the whole answer as one grouped value.
    assert_eq!(value("1 500"), value("1500"));
    assert_eq!(value("1,500"), value("1500"));
    assert_eq!(value("1500%"), value("15"));
    assert_eq!(value("1500 %"), value("15"));
    // A bracket makes a pair explicit, and a two-digit group is no group at all,
    // so the tuple readings of round 1 stand.
    assert_eq!(ast("(1,500)"), Ast::Tuple(vec![int(1), int(500)]));
    assert_eq!(ast("4,17"), Ast::Tuple(vec![int(4), int(17)]));
    assert_eq!(ast("4, 17"), Ast::Tuple(vec![int(4), int(17)]));
    assert_eq!(ast("x,500"), Ast::Tuple(vec![var("x"), int(500)]));
}

#[test]
fn a_vulgar_fraction_after_a_digit_run_is_a_mixed_number() {
    // Review findings #1 and #9. The old reading made `3½` the product `3*(1/2)`,
    // so a learner who wrote three and a half was correct against `1.5`.
    // Restated in review round 3: the glyph is a lexer token, so the source
    // keeps it and this test pins the tree it builds.
    assert_eq!(ast("½"), frac(1, 2));
    assert_eq!(ast("3½"), mixed(3, 1, 2));
    assert_eq!(ast("2⅓"), mixed(2, 1, 3));
    assert_eq!(ast("5¾"), mixed(5, 3, 4));
    assert_eq!(ast("x½"), Ast::Mul(vec![var("x"), frac(1, 2)]));
    assert_eq!(ast("(2)½"), Ast::Mul(vec![int(2), frac(1, 2)]));
    assert_eq!(value("3½"), value("7/2"));
    assert_eq!(value("2⅓"), value("7/3"));
    assert_ne!(value("2⅓"), value("2/3"));
    assert_ne!(value("3½"), value("1.5"));
    assert_eq!(value("3½"), value("3 1/2"));
}

#[test]
fn every_spelling_of_a_mixed_number_has_one_value() {
    // Review round 2, findings #1, #2, #3, #5, #6, #7. The round 1 reading fired
    // on the glued glyph alone: `2½` was 5/2 while `2 ½` and `2\frac{1}{2}` were
    // the product 1. One value in five spellings therefore got two verdicts, and
    // the wrong learner answer two and a half was correct against the authored 1
    // on the topic that writes mixed numbers (C4).
    let spellings: [&str; 7] = [
        "2 1/2",
        "2½",
        "2 ½",
        "2\u{a0}½",
        "2\u{2009}½",
        "2\\frac{1}{2}",
        "2 \\frac{1}{2}",
    ];
    for text in spellings {
        assert_eq!(
            ast(text),
            Ast::Mixed {
                whole: BigInt::from(2),
                numerator: BigInt::from(1),
                denominator: BigInt::from(2)
            },
            "{text:?} is one mixed number"
        );
        assert_eq!(value(text), value("5/2"), "{text:?} is five halves");
        // C4: the product reading is the wrong value, and no spelling admits it.
        assert_ne!(value(text), value("1"), "{text:?} is not the product 1");
    }
    // The sign of the whole part carries over the whole value.
    for text in ["-2 1/2", "-2½", "-2 ½", "-2\\frac{1}{2}", "-2 \\frac{1}{2}"] {
        assert_eq!(value(text), value("-5/2"), "{text:?} is minus five halves");
        assert_ne!(value(text), value("-1"), "{text:?} is not minus one");
        assert_ne!(value(text), value("5/2"), "{text:?} keeps its sign");
    }
    // The neighbors of the rule. A mixed number is one operand of the term it
    // stands in, and it takes no second fraction.
    assert_eq!(value("2 ½ + 1"), value("7/2"));
    assert_eq!(value("2\\frac{1}{2} + 1"), value("7/2"));
    assert_eq!(value("2 ½*2"), value("5"));
    assert_eq!(value("1 - 2 ½"), value("-3/2"));
    assert_eq!(value("3 ⅓"), value("10/3"));
    assert_eq!(value("5 ¾"), value("23/4"));
    assert_eq!(value("2\\frac{7}{12}"), value("31/12"));
    // A learner who writes the product still gets the product. An explicit `*`,
    // a bracket, and a bracketed whole part are three spellings of one half of
    // two, and every one of them keeps the value 1.
    assert_eq!(value("2*½"), value("1"));
    assert_eq!(value("2(1/2)"), value("1"));
    assert_eq!(value("(2)½"), value("1"));
    assert_eq!(value("2*\\frac{1}{2}"), value("1"));
    assert_eq!(value("x½"), value("x/2"));
    assert_eq!(value("x\\frac{1}{2}"), value("x/2"));
    // The lone fraction keeps its own value in every position.
    assert_eq!(value("½"), value("1/2"));
    assert_eq!(value("\\frac{1}{2}"), value("1/2"));
    assert_eq!(value("1/½"), value("2"));
    assert_eq!(value("½ + ½"), value("1"));
    assert_eq!(value("sqrt ½"), value("sqrt(1/2)"));
}

#[test]
fn a_frac_is_one_token_whatever_whitespace_its_braces_hold() {
    // Review round 3, findings #1 and #2. Round 2 tested the brace TEXT for two
    // digit runs, and `collapse_whitespace` keeps a space inside a brace, so
    // `2\frac{ 1}{2}` fell out of the fraction spelling and read as the product
    // 1: a learner who wrote two and a half was graded correct against the
    // authored `1` of `mixed-numbers` kp3, and the right answer `4\frac{ 1}{2}`
    // against an authored `4 1/2` was graded wrong. The lexer lexes the brace
    // bodies now, so whitespace changes no token.
    for text in [
        "2\\frac{1}{2}",
        "2\\frac{ 1}{2}",
        "2\\frac{1}{ 2}",
        "2\\frac{ 1 }{ 2 }",
        "2 \\frac{ 1}{2}",
        "2\\frac{1 }{2 }",
        "2\u{00a0}\\frac{ 1 }{2}",
    ] {
        assert_eq!(ast(text), mixed(2, 1, 2), "{text:?} is one mixed number");
        assert_eq!(value(text), value("5/2"), "{text:?} is five halves");
        // C4: the product reading is the wrong value, and no spelling admits it.
        assert_ne!(value(text), value("1"), "{text:?} is not the product 1");
    }
    // The right answer in the same notation is accepted, which is the mirror of
    // the same defect.
    assert_eq!(value("4\\frac{ 1}{2}"), value("4 1/2"));
    assert_eq!(value("3\\frac{ 3 }{ 4 }"), value("15/4"));
    assert_eq!(value("2\\frac{ 7 }{ 12 }"), value("31/12"));
    assert_ne!(value("3\\frac{ 3 }{ 4 }"), value("9/4"));
    // A second `\frac` beside the first is a product of two fractions, not a
    // mixed number, because no number token stands in front of it.
    assert_eq!(
        ast("\\frac{1}{2}\\frac{1}{3}"),
        Ast::Mul(vec![frac(1, 2), frac(1, 3)])
    );
    assert_eq!(value("\\frac{1}{2}\\frac{1}{3}"), value("1/6"));
    assert_eq!(value("\\frac{ 1 }{ 2 }\\frac{1}{3}"), value("1/6"));
    assert_ne!(value("\\frac{1}{2}\\frac{1}{3}"), value("1/2"));
    // A brace body that is no digit run is a quotient of two expressions, and
    // whitespace changes nothing there either.
    assert_eq!(
        ast("\\frac{x+1}{2}"),
        Ast::Div(Box::new(Ast::Add(vec![var("x"), int(1)])), Box::new(int(2)))
    );
    assert_eq!(value("\\frac{ x + 1 }{ 2 }"), value("(x+1)/2"));
    assert_eq!(value("\\frac{2x}{4}"), value("x/2"));
    assert_ne!(value("\\frac{x+1}{2}"), value("x + 1/2"));
    // A number in front of a `\frac` whose braces hold an expression is neither
    // a mixed number nor a product, so the answer gets no verdict (C4).
    for text in ["2\\frac{x+1}{2}", "2\\frac{+1}{2}", "2\\frac{1 2}{3}"] {
        assert_eq!(
            refusal(text),
            "a mixed number whose fraction is not proper",
            "{text:?} takes no reading"
        );
    }
    // A `\frac` nests, and the nesting is one node at every level.
    assert_eq!(value("\\frac{\\frac{1}{2}}{3}"), value("1/6"));
    assert_eq!(value("\\frac{1}{\\frac{1}{2}}"), value("2"));
    // An empty brace body carries no value.
    assert_eq!(
        refusal("\\frac{}{2}"),
        "a fraction with a body the reader cannot read"
    );
    assert_eq!(
        refusal("\\frac{1}{0}"),
        "a fraction with a zero denominator"
    );
}

#[test]
fn a_bracket_free_function_argument_stops_at_a_function() {
    // Review round 3, finding #5. The chain ran over every operand, so the
    // authored `sec x tan x` of `derivatives-trig` kp1 exemplar 1 meant
    // `sec(x*tan(x))`: the correct learner answer `sec(x)tan(x)` was graded
    // wrong and the meaningless `sec(x tan x)` was graded correct (C4).
    //
    // The three answers are authored, and the 1.0 reading of each one is its
    // `canonical` field in `crates/core/tests/fixtures/answers/corpus_1_0.jsonl`:
    //   derivatives-trig kp1 1 `sec x tan x`    1.0 source `sec x tan x`
    //                                           1.0 canonical `sec(x*tan(x))`
    //   derivatives-trig kp1 2 `-csc x cot x`   1.0 source `-csc x cot x`
    //                                           1.0 canonical `-csc(x*cot(x))`
    //   derivatives-trig kp2 2 `2 sin x cos x`  1.0 source `2 sin x cos x`
    //                                           1.0 canonical `2*sin(x*cos(x))`
    // 2.0 reads the product instead. `answer_divergence.rs` records the three
    // measured divergences with the 1.0 verdicts.
    assert_eq!(
        ast("sec x tan x"),
        Ast::Mul(vec![call("sec", var("x")), call("tan", var("x"))])
    );
    assert_eq!(
        ast("-csc x cot x"),
        Ast::Mul(vec![
            Ast::Neg(Box::new(call("csc", var("x")))),
            call("cot", var("x"))
        ])
    );
    assert_eq!(
        ast("2 sin x cos x"),
        Ast::Mul(vec![int(2), call("sin", var("x")), call("cos", var("x"))])
    );
    // The learner spellings of the same value are accepted.
    assert_eq!(value("sec x tan x"), value("sec(x)tan(x)"));
    assert_eq!(value("sec x tan x"), value("sec(x)*tan(x)"));
    assert_eq!(value("-csc x cot x"), value("-csc(x)*cot(x)"));
    assert_eq!(value("2 sin x cos x"), value("2*sin(x)*cos(x)"));
    assert_eq!(value("2 sin x cos x"), value("2 sin(x) cos(x)"));
    // The chain stops in the same place with an explicit `*` in front of the
    // function, so one value keeps one reading.
    assert_eq!(value("sec x * tan x"), value("sec(x)*tan(x)"));
    assert_eq!(value("sec x * tan x"), value("sec x tan x"));
    // C4: the nested reading is a different value and it is refused.
    assert_ne!(value("sec x tan x"), value("sec(x tan x)"));
    assert_ne!(value("2 sin x cos x"), value("2 sin(x cos x)"));
    assert_ne!(value("sec x tan x"), value("sec(x)*tan(y)"));
    assert_ne!(value("2 sin x cos x"), value("2*sin(x)*cos(y)"));
    // A root is the name `sqrt` in another spelling, so it stops the chain too.
    assert_eq!(value("cos 2\\sqrt{3}"), value("cos(2)*sqrt(3)"));
    assert_eq!(value("cos 2√3"), value("cos(2)*sqrt(3)"));
    assert_eq!(value("cos 2 sqrt 3"), value("cos(2)*sqrt(3)"));
    assert_ne!(value("cos 2√3"), value("cos(2*sqrt(3))"));
    // The stop takes the second function and never the first, so a bracket-free
    // argument still reaches its own function.
    assert_eq!(value("sqrt 2"), value("sqrt(2)"));
    assert_eq!(value("15 sqrt 3"), value("15*sqrt(3)"));
    assert_eq!(value("sin cos x"), value("sin(cos(x))"));
    assert_eq!(value("cos 2x"), value("cos(2*x)"));
    assert_eq!(value("sin 3t^2"), value("sin(3*t**2)"));
    // The four authored corpus answers that hold a function after a bracket-free
    // argument keep the 1.0 reading, because their arguments are bracketed or
    // the chain stops at an operator first.
    assert_eq!(value("tan x + x sec^2 x"), value("x*sec(x)**2 + tan(x)"));
    assert_eq!(value("sin x + x cos x"), value("x*cos(x) + sin(x)"));
    assert_eq!(
        value("sec(2x) + 2x sec(2x) tan(2x)"),
        value("2*x*tan(2*x)*sec(2*x) + sec(2*x)")
    );
    assert_eq!(value("e^(sin x) cos x"), value("exp(sin(x))*cos(x)"));
}

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
    assert_eq!(
        ast("-3 1/2"),
        Ast::Neg(Box::new(Ast::Mixed {
            whole: BigInt::from(3),
            numerator: BigInt::from(1),
            denominator: BigInt::from(2)
        }))
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
    assert_eq!(
        ast("-3½"),
        Ast::Neg(Box::new(Ast::Mixed {
            whole: BigInt::from(3),
            numerator: BigInt::from(1),
            denominator: BigInt::from(2)
        }))
    );
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
