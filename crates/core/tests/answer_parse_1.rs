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

mod common;

use common::parse::*;

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
    // `30°` is a quantity since the value-with-unit production of D-F3 (unit
    // f2-grammar); the lexer deleted the degree sign before.
    assert_eq!(
        ast("30°"),
        Ast::Quantity {
            value: Box::new(int(30)),
            unit: "°",
        }
    );
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
