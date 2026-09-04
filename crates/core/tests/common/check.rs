//! The helpers of the `answer_check` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

pub use super::answer::*;
pub use super::fixtures::*;
pub use super::fuzz::*;
pub use cadus_core::answer::check::same_answer;
pub use cadus_core::answer::{
    Ast, Atom, Basis, Canon, Monomial, Outcome, Verdict, canon, canonical_form, check,
};
pub use cadus_core::curriculum::AnswerKind;
pub use num_bigint::BigInt;
pub use num_rational::BigRational;
pub use std::collections::BTreeMap;
pub use std::time::{Duration, Instant};

/// Run one table of `(expected, learner, kind, correct)` rows.
pub fn run_table(rows: &[(&str, &str, AnswerKind, bool)]) {
    for (expected, learner, kind, correct) in rows {
        assert_eq!(
            check(expected, learner, *kind),
            decided(*correct, false),
            "{expected:?} against {learner:?} on {kind}"
        );
    }
}

/// Build an exact rational literal.
pub fn ratio(numerator: i64, denominator: i64) -> BigRational {
    BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
}

/// Build a whole-number rational literal.
pub fn whole(value: i64) -> BigRational {
    BigRational::from_integer(BigInt::from(value))
}

/// Build a monomial literal from atoms and exponents.
pub fn monomial(atoms: &[(Atom, i64)]) -> Monomial {
    atoms.iter().cloned().collect()
}

/// Build a variable atom literal.
pub fn var(name: &str) -> Atom {
    Atom::Var(name.to_string())
}

/// Canonicalize one answer, and fail the test when the grammar refuses it.
pub fn form(text: &str) -> Canon {
    canonical_form(text).unwrap_or_else(|e| panic!("{text:?}: {}", e.reason))
}

/// Build a radical literal from one radicand and its coefficient.
pub fn radical(radicand: i64, coefficient: BigRational) -> Canon {
    Canon::Radical(BTreeMap::from([(
        Basis {
            radicand: BigInt::from(radicand),
            pi: 0,
            e: 0,
        },
        coefficient,
    )]))
}

/// Whether the run asks for the release budget of L2.
///
/// A debug build runs the exact arithmetic about ten times slower than a release
/// build, so the two builds carry two budgets. `CADUS_RELEASE_BENCH` selects the
/// release budget of 5 ms per check and 1 s per corpus pass. A plain
/// `cargo test` run keeps the debug budget of 50 ms and 5 s, which measures the
/// work and not the scheduler (`docs/reviews/M2-review-1.md`, finding 20).
pub fn release_bench() -> bool {
    std::env::var_os("CADUS_RELEASE_BENCH").is_some()
}

/// The wall-clock budget of one check.
pub fn one_check_budget() -> Duration {
    if release_bench() {
        Duration::from_millis(5)
    } else {
        Duration::from_millis(50)
    }
}

/// The wall-clock budget of one check of a crafted worst-case answer.
///
/// The round-4 ruling of finding #2 puts the reviewer's 4,000-character case at
/// 50 ms in a release build. A debug build runs the exact arithmetic about ten
/// times slower, so the debug budget is ten times the release one, the same
/// scale as [`one_check_budget`] carries.
pub fn bomb_budget() -> Duration {
    if release_bench() {
        Duration::from_millis(50)
    } else {
        Duration::from_millis(500)
    }
}

/// The wall-clock budget of one pass over the whole corpus.
pub fn corpus_budget() -> Duration {
    if release_bench() {
        Duration::from_secs(1)
    } else {
        Duration::from_secs(5)
    }
}

// ---------------------------------------------------------------------------
// Spec section 6.3 — tests/test_sympy_security.py
// ---------------------------------------------------------------------------
/// The payloads of 1.0 `tests/test_sympy_security.py:24-58`.
pub const RCE_PAYLOADS: [&str; 4] = [
    "__import__('os').system('touch _rce_marker_should_not_exist')",
    "exec(\"open('_rce_marker_should_not_exist','w').write('x')\")",
    "eval(\"__import__('os').getenv('ANTHROPIC_API_KEY')\")",
    "print(open('.env.example').read())",
];

/// Build the canonical form of one labeled whole number, `<var> = <value>`.
pub fn labeled(var: &str, value: i64) -> Canon {
    let ast = Ast::Assign {
        var: var.to_string(),
        value: Box::new(Ast::Integer(BigInt::from(value))),
    };
    canon(&ast).unwrap_or_else(|e| panic!("{var} = {value}: {}", e.reason))
}

// ---------------------------------------------------------------------------
// The corpus: coverage and the L2 budget
// ---------------------------------------------------------------------------
/// One corpus row of `crates/core/tests/fixtures/answers/corpus_1_0.jsonl`.
#[derive(serde::Deserialize)]
pub struct CorpusRow {
    pub answer: String,
    pub answer_kind: String,
}

/// Read the answer corpus.
pub fn corpus() -> Vec<CorpusRow> {
    read_jsonl("corpus_1_0.jsonl")
}

/// Read the answer kind of a corpus row.
pub fn kind_of(row: &CorpusRow) -> AnswerKind {
    match row.answer_kind.as_str() {
        "numeric" => AnswerKind::Numeric,
        "expression" => AnswerKind::Expression,
        other => panic!("the corpus holds only verifiable kinds, and this row is {other}"),
    }
}

/// Build the reciprocal bomb of M2 review 1, finding 5.
///
/// The answer is `1/(a/3**250 + b/5**250 + …)`: every term carries a coprime
/// denominator, so the least common multiple of the content normalization grows
/// by about 580 bits per term. The shape is inside the section 8.1 grammar and
/// inside the 4,000-character input cap.
pub fn reciprocal_bomb(terms: usize) -> String {
    const NAMES: [&str; 40] = [
        "a", "b", "c", "d", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
        "u", "v", "w", "x", "y", "z", "A", "B", "C", "D", "F", "G", "H", "I", "J", "K", "L", "M",
        "N", "O", "P", "Q",
    ];
    const PRIMES: [u32; 40] = [
        3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89,
        97, 101, 103, 107, 109, 113, 127, 131, 137, 139, 149, 151, 157, 163, 167, 173, 179,
    ];
    let mut parts: Vec<String> = Vec::new();
    for index in 0..terms {
        let name = NAMES[index % NAMES.len()];
        let prime = PRIMES[index % PRIMES.len()];
        let power = 1 + index / NAMES.len();
        let numerator = if power == 1 {
            name.to_string()
        } else {
            format!("{name}**{power}")
        };
        parts.push(format!("{numerator}/{prime}**250"));
    }
    // The last term brings the answer to the 2,072 characters of the review.
    parts.push("R/2**11".to_string());
    format!("1/({})", parts.join(" + "))
}

/// Build the sum-rebuild bomb of M2 review 4, finding 2.
///
/// The answer is one bracket that holds the 280-term sum `e**2 + … + e**281`,
/// and `factors` factors of `1` follow it. Every factor costs one node step, and
/// every factor rebuilt all 280 terms three times over: once in `frac_of`, once
/// in the `is_one` short cut of `poly_mul`, and once in the demotion that
/// `value` runs. `MAX_STEPS` bounded the COUNT of operations and not the COST of
/// one, so 846 factors cost 188 ms of canonicalization in a release build.
///
/// The shape is inside the section 8.1 grammar, holds no LaTeX and no glyph, and
/// stays inside the 4,000-character input cap.
pub fn sum_rebuild_bomb(factors: usize) -> String {
    let mut bomb = wide_sum(280);
    for _ in 0..factors {
        bomb.push_str("*1");
    }
    bomb
}

/// Build one bracketed sum of `terms` powers of `e`.
pub fn wide_sum(terms: usize) -> String {
    let parts: Vec<String> = (2..2 + terms).map(|power| format!("e^{power}")).collect();
    format!("({})", parts.join("+"))
}

/// The characters a hostile learner reaches for.
pub const FUZZ_ALPHABET: [&str; 40] = [
    "0", "1", "9", ".", ",", "+", "-", "*", "/", "**", "^", "(", ")", "[", "]", "{", "}", "<", "=",
    ">", "x", "y", "sqrt", "sin", "log", "exp", "pi", "e", "theta", "%", "$", "\\frac", "\\sqrt",
    "√", "π", "∞", "≤", "²", " ", "\u{202f}",
];

/// Build one random string from the alphabet.
pub fn fuzz_string(rng: &mut Rng, tokens: usize) -> String {
    let mut out = String::new();
    for _ in 0..tokens {
        let index = rng.below(FUZZ_ALPHABET.len());
        out.push_str(FUZZ_ALPHABET.get(index).copied().unwrap_or("0"));
    }
    out
}

/// Build one random answer: half raw bytes, half tokens of the alphabet.
pub fn fuzz_case(rng: &mut Rng) -> String {
    if rng.next().is_multiple_of(2) {
        let length = 1 + rng.below(64);
        fuzz_bytes(rng, length)
    } else {
        let tokens = 1 + rng.below(24);
        fuzz_string(rng, tokens)
    }
}

/// Build one random byte string, read as lossy UTF-8.
pub fn fuzz_bytes(rng: &mut Rng, length: usize) -> String {
    let mut bytes = Vec::with_capacity(length);
    for _ in 0..length {
        bytes.push(u8::try_from(rng.next() % 256).unwrap_or(0));
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The range of `x` from -1 to 3, with the given closedness of each end.
pub fn x_range(lo_closed: bool, hi_closed: bool) -> Canon {
    Canon::Interval {
        var: Some("x".to_string()),
        lo: Some(Box::new(Canon::Rational(whole(-1)))),
        lo_closed,
        hi: Some(Box::new(Canon::Rational(whole(3)))),
        hi_closed,
    }
}
