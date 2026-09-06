//! The helpers of the `answer_parse` tests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(unused_imports)]

pub use super::fixtures::*;
pub use super::fuzz::*;
pub use cadus_core::answer::{
    Ast, Canon, Const, IneqOp, MAX_ANSWER_CHARS, canonical_form, normalize, parse,
};
pub use num_bigint::BigInt;
pub use std::collections::BTreeSet;

/// One corpus row of `crates/core/tests/fixtures/answers/corpus_1_0.jsonl`.
#[derive(serde::Deserialize)]
pub struct CorpusRow {
    pub answer: String,
    pub shape: String,
    pub topic_id: String,
    pub kp_id: String,
    pub exemplar_index: i64,
}

/// One row of the committed undecidable fixture.
#[derive(serde::Deserialize)]
pub struct ResidueRow {
    pub answer: String,
    pub topic_id: String,
    pub kp_id: String,
    pub exemplar_index: i64,
}

/// The identity of one answer in the corpus.
pub type Key = (String, String, i64, String);

/// Read the corpus.
pub fn corpus() -> Vec<CorpusRow> {
    read_jsonl("corpus_1_0.jsonl")
}

/// Read the committed set of answers the grammar refuses.
pub fn committed_residue() -> BTreeSet<Key> {
    read_jsonl::<ResidueRow>("undecidable_1_0.jsonl")
        .into_iter()
        .map(|row| (row.topic_id, row.kp_id, row.exemplar_index, row.answer))
        .collect()
}

/// Normalize and parse one answer, and fail the test when the grammar refuses it.
pub fn ast(text: &str) -> Ast {
    let source = normalize(text).source;
    parse(&source).unwrap_or_else(|e| panic!("{text:?} -> {source:?}: {}", e.reason))
}

/// Build an integer node.
pub fn int(value: i64) -> Ast {
    Ast::Integer(BigInt::from(value))
}

/// Build a variable node.
pub fn var(name: &str) -> Ast {
    Ast::Var(name.to_string())
}

/// Build a function call node with one argument.
pub fn call(name: &str, argument: Ast) -> Ast {
    Ast::Func(name.to_string(), vec![argument])
}

/// Build a square-root node.
///
/// Review round 3 makes every root one node, so `sqrt(2)`, `\sqrt{2}`,
/// `\sqrt 2` and `√2` all build `Ast::Sqrt`.
pub fn root(argument: Ast) -> Ast {
    Ast::Sqrt(Box::new(argument))
}

/// Build a literal fraction node.
pub fn frac(numerator: i64, denominator: i64) -> Ast {
    Ast::Fraction {
        numerator: BigInt::from(numerator),
        denominator: BigInt::from(denominator),
    }
}

/// Build a mixed-number node with a non-negative whole part.
pub fn mixed(whole: i64, numerator: i64, denominator: i64) -> Ast {
    Ast::Mixed {
        whole: BigInt::from(whole),
        numerator: BigInt::from(numerator),
        denominator: BigInt::from(denominator),
    }
}

/// The refusal reason of an answer the grammar does not read.
pub fn refusal(text: &str) -> &'static str {
    match parse(&normalize(text).source) {
        Ok(ast) => panic!("{text:?} parsed to {ast:?}"),
        Err(refused) => refused.reason,
    }
}

/// Canonicalize one answer, and fail the test when the grammar refuses it.
pub fn value(text: &str) -> Canon {
    canonical_form(text).unwrap_or_else(|e| panic!("{text:?}: {}", e.reason))
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
/// - The rational-exponent production of D-F3 (unit f2-grammar) then reads 14
///   `expression_symbolic` rows (`x^(1/2)`, `(5/2)x^(3/2)`) and one
///   `expression_numeric` row (`3 + 3*2^(1/3)`), so the two buckets hold 646
///   and 229. The quotient-and-remainder production reads all 16
///   `quotient_remainder` rows (`9 R2`, `x + 2 remainder 3`).
pub const SHAPE_COUNTS: [(&str, usize, usize); 15] = [
    ("comma_list", 28, 15),
    ("decimal", 128, 0),
    ("equation", 1, 0),
    ("expression_numeric", 229, 4),
    ("expression_symbolic", 646, 40),
    ("fraction", 350, 2),
    ("integer", 1622, 0),
    ("interval_ineq", 29, 5),
    ("mixed_number", 8, 0),
    ("ordered_tuple", 178, 0),
    ("other", 7, 0),
    ("prose_or_words", 0, 167),
    ("quotient_remainder", 16, 0),
    ("set_or_list", 5, 0),
    ("value_with_unit", 11, 1),
];

/// The tree of `2 cos 2t + (5/2) sin 2t`, which two spellings of the argument chain share.
pub fn two_cos_2t_plus_five_halves_sin_2t() -> Ast {
    Ast::Add(vec![
        Ast::Mul(vec![int(2), call("cos", Ast::Mul(vec![int(2), var("t")]))]),
        Ast::Mul(vec![
            frac(5, 2),
            call("sin", Ast::Mul(vec![int(2), var("t")])),
        ]),
    ])
}
