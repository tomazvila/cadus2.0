//! U2 acceptance: canonical forms (V1, D6) and the checker (A3, C4, V4).
//!
//! Every pair in this file is a literal of
//! `docs/reference/checker-1.0-spec.md`, and every expected verdict is the 1.0
//! verdict that the spec records. No expected value is read back from the code
//! under test.
//!
//! The pairs where 2.0 deliberately leaves 1.0 are NOT here. They live in
//! `answer_divergence.rs`, one test each, with the recorded 1.0 verdict and the
//! reason for the change.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use cadus_core::answer::{Atom, Basis, Canon, Monomial, Outcome, Verdict, canonical_form, check};
use cadus_core::curriculum::AnswerKind;
use num_bigint::BigInt;
use num_rational::BigRational;

/// The `numeric` answer kind.
const N: AnswerKind = AnswerKind::Numeric;
/// The `expression` answer kind.
const E: AnswerKind = AnswerKind::Expression;

/// The outcome of a decided check.
fn decided(correct: bool, notation: bool) -> Outcome {
    Outcome::Decided(Verdict { correct, notation })
}

/// Run one table of `(expected, learner, kind, correct)` rows.
fn run_table(rows: &[(&str, &str, AnswerKind, bool)]) {
    for (expected, learner, kind, correct) in rows {
        assert_eq!(
            check(expected, learner, *kind),
            decided(*correct, false),
            "{expected:?} against {learner:?} on {kind}"
        );
    }
}

/// Build an exact rational literal.
fn ratio(numerator: i64, denominator: i64) -> BigRational {
    BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
}

/// Build a whole-number rational literal.
fn whole(value: i64) -> BigRational {
    BigRational::from_integer(BigInt::from(value))
}

/// Build a monomial literal from atoms and exponents.
fn monomial(atoms: &[(Atom, i64)]) -> Monomial {
    atoms.iter().cloned().collect()
}

/// Build a variable atom literal.
fn var(name: &str) -> Atom {
    Atom::Var(name.to_string())
}

/// Canonicalize one answer, and fail the test when the grammar refuses it.
fn form(text: &str) -> Canon {
    canonical_form(text).unwrap_or_else(|e| panic!("{text:?}: {}", e.reason))
}

// ---------------------------------------------------------------------------
// Spec section 6.1 — tests/test_sympy_check.py
// ---------------------------------------------------------------------------

#[test]
fn spec_6_1_numeric_equivalence() {
    run_table(&[
        ("2/3", "0.667", N, false),
        ("2*x+1", "1 + 2*x", E, true),
        ("x**2 - 1", "(x-1)*(x+1)", E, true),
        ("3", "4", N, false),
        ("7", "49/7", N, true),
        ("1/2", "0.5", N, true),
    ]);
}

#[test]
fn spec_6_1_unicode_maths_is_parseable() {
    run_table(&[
        ("15√3", "15*sqrt(3)", E, true),
        ("1/(2√x)", "1/(2*sqrt(x))", E, true),
        ("x/√(x^2 + 9)", "x/sqrt(x**2+9)", E, true),
        (
            "1/(4√x · √(1 + √x))",
            "1/(4*sqrt(x)*sqrt(1+sqrt(x)))",
            E,
            true,
        ),
        ("2√3/3", "2*sqrt(3)/3", E, true),
        ("π/6", "pi/6", N, true),
        ("2π", "2*pi", N, true),
        ("x²+1", "x**2+1", E, true),
        ("½", "1/2", N, true),
        ("30°", "30", N, true),
        ("15*sqrt(3)", "15√3", E, true),
        ("pi/6", "π/6", N, true),
        ("15√3", "15*sqrt(2)", E, false),
        ("π/6", "pi/3", N, false),
        ("x²+1", "x**3+1", E, false),
        ("√2", "2", N, false),
    ]);
}

#[test]
fn spec_6_1_non_numeric_pairs_compare_structurally() {
    run_table(&[
        ("(4, 17)", "(4,17)", N, true),
        ("(4, 17)", "(4, 18)", N, false),
        ("(-6, 2)", "(-6,2)", N, true),
        ("{1, 2}", "{2,1}", E, true),
        ("{1, 2}", "{1,3}", E, false),
        ("1 + I", "1+I", E, true),
        ("1 + I", "1 - I", E, false),
        // Spec section 7.6: 1.0 reads a bare comma list and a parenthesized tuple
        // as one answer, and so does 2.0.
        ("4, 17", "(4, 17)", N, true),
    ]);
}

#[test]
fn spec_6_1_grouped_integers_are_accepted() {
    for learner in [
        "7,329",
        "7329",
        "7,329.",
        "7329.",
        "7 329",
        "7\u{00a0}329",
        "7\u{202f}329",
        "$7,329$",
        " 7,329 ",
    ] {
        assert_eq!(
            check("7329", learner, N),
            decided(true, false),
            "grouped {learner:?}"
        );
    }
}

#[test]
fn spec_6_1_a_wrong_value_stays_wrong_however_it_is_grouped() {
    for learner in ["7330", "7,330", "7.32", "73,29", "7329x"] {
        assert_eq!(
            check("7329", learner, N),
            decided(false, false),
            "wrong {learner:?}"
        );
    }
}

#[test]
fn spec_6_1_a_period_grouped_integer_is_a_notation_variant() {
    assert_eq!(check("7329", "7.329", N), decided(true, true));
    assert_eq!(check("2500", "2.500", N), decided(true, true));
}

#[test]
fn spec_6_1_the_american_reading_always_wins_first() {
    assert_eq!(check("0.5", "0.5", N), decided(true, false));
    assert_eq!(check("1", "1.000", N), decided(true, false));
}

#[test]
fn spec_6_1_the_notation_reading_requires_an_exact_value_match() {
    assert_eq!(check("7239", "7.329", N), decided(false, false));
}

#[test]
fn spec_6_1_a_trailing_period_does_not_break_an_expression() {
    assert_eq!(check("2*x+1", "1 + 2x.", E), decided(true, false));
}

#[test]
fn spec_6_1_a_decimal_answer_survives_a_trailing_period() {
    assert_eq!(check("0.5", "0.5.", N), decided(true, false));
    assert_eq!(check("0.5", "0.50", N), decided(true, false));
}

// ---------------------------------------------------------------------------
// Spec section 6.2 — tests/test_deterministic_grade.py, mapped to `check`
// ---------------------------------------------------------------------------

#[test]
fn spec_6_2_a_verified_correct_answer_needs_no_engine() {
    run_table(&[
        ("12", "12", N, true),
        ("12", "12.0", N, true),
        ("12", "sqrt(144)", N, true),
        ("7329", "7,329", N, true),
        ("7400", "7400", N, true),
        ("2*x+1", "1 + 2x", E, true),
    ]);
}

#[test]
fn spec_6_2_a_blank_answer_is_wrong() {
    for learner in ["", "   ", "\t\n"] {
        assert_eq!(
            check("12", learner, N),
            decided(false, false),
            "blank {learner:?}"
        );
    }
    // The blank rung runs before the kind gate, exactly as 1.0
    // `deterministic_grade.py:120-125` orders the two.
    assert_eq!(
        check("anything", "   ", AnswerKind::Proof),
        decided(false, false)
    );
}

#[test]
fn spec_6_2_an_unverifiable_kind_has_no_deterministic_verdict() {
    for kind in [AnswerKind::MultiStep, AnswerKind::Proof] {
        match check("anything", "some answer", kind) {
            Outcome::Undecidable(reason) => {
                assert_eq!(reason.reason, "the answer kind is not decidable");
            }
            other => panic!("{kind} gave {other:?}"),
        }
    }
}

#[test]
fn spec_6_2_a_correct_answer_carries_no_notation_note() {
    assert_eq!(check("0.5", "0.5", N), decided(true, false));
    assert_eq!(check("12", "12", N), decided(true, false));
}

// ---------------------------------------------------------------------------
// Spec section 6.3 — tests/test_sympy_security.py
// ---------------------------------------------------------------------------

/// The payloads of 1.0 `tests/test_sympy_security.py:24-58`.
const RCE_PAYLOADS: [&str; 4] = [
    "__import__('os').system('touch _rce_marker_should_not_exist')",
    "exec(\"open('_rce_marker_should_not_exist','w').write('x')\")",
    "eval(\"__import__('os').getenv('ANTHROPIC_API_KEY')\")",
    "print(open('.env.example').read())",
];

#[test]
fn spec_6_3_security_payloads_are_undecidable_and_run_nothing() {
    for payload in RCE_PAYLOADS {
        assert!(
            matches!(check("4", payload, N), Outcome::Undecidable(_)),
            "payload as the learner answer: {payload:?}"
        );
        assert!(
            matches!(check(payload, "4", N), Outcome::Undecidable(_)),
            "payload as the authored answer: {payload:?}"
        );
    }
    assert!(
        !std::path::Path::new("_rce_marker_should_not_exist").exists(),
        "the checker must never evaluate a learner answer"
    );
}

#[test]
fn spec_6_3_exponent_bombs_are_undecidable_inside_the_budget() {
    let bombs = [
        "9**9**9",
        "9^9^9",
        "9**9**9**9",
        "2**10000000",
        "(2)**(9999999)",
    ];
    let start = Instant::now();
    for bomb in bombs {
        assert!(
            matches!(check("4", bomb, N), Outcome::Undecidable(_)),
            "bomb as the learner answer: {bomb:?}"
        );
    }
    for bomb in ["9**9**9", "2**10000000"] {
        assert!(
            matches!(check(bomb, "4", N), Outcome::Undecidable(_)),
            "bomb as the authored answer: {bomb:?}"
        );
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(50),
        "the exponent bombs took {elapsed:?}, and the budget is 50 ms"
    );
}

#[test]
fn spec_6_3_the_guard_does_not_reject_legitimate_powers() {
    run_table(&[
        ("8", "2**3", E, true),
        ("1024", "2**10", E, true),
        ("x**2+1", "x^2+1", E, true),
        ("x**2*y**3", "x^2*y^3", E, true),
        ("15√3", "15*sqrt(3)", E, true),
    ]);
}

// ---------------------------------------------------------------------------
// Canonical forms (V1, D6) — the literal shapes of spec section 8.1
// ---------------------------------------------------------------------------

#[test]
fn a_decimal_becomes_an_exact_rational() {
    assert_eq!(form("0.7"), Canon::Rational(ratio(7, 10)));
    assert_eq!(form("0.50"), Canon::Rational(ratio(1, 2)));
    assert_eq!(form("12.0"), Canon::Rational(whole(12)));
    assert_eq!(form("3 1/2"), Canon::Rational(ratio(7, 2)));
    assert_eq!(form("-2 1/4"), Canon::Rational(ratio(-9, 4)));
}

#[test]
fn a_radical_is_reduced_to_a_squarefree_radicand() {
    let two_root_two = Canon::Radical(BTreeMap::from([(
        Basis {
            radicand: BigInt::from(2),
            pi: 0,
            e: 0,
        },
        whole(2),
    )]));
    assert_eq!(form("sqrt(8)"), two_root_two);
    assert_eq!(form("sqrt(4)"), Canon::Rational(whole(2)));
    assert_eq!(form("sqrt(144)"), Canon::Rational(whole(12)));
    assert_eq!(form("sqrt(0)"), Canon::Rational(whole(0)));
    // sqrt(2)*sqrt(3) is sqrt(6), and 1/sqrt(2) is sqrt(2)/2.
    let root_six = Canon::Radical(BTreeMap::from([(
        Basis {
            radicand: BigInt::from(6),
            pi: 0,
            e: 0,
        },
        whole(1),
    )]));
    assert_eq!(form("sqrt(2)*sqrt(3)"), root_six);
    let half_root_two = Canon::Radical(BTreeMap::from([(
        Basis {
            radicand: BigInt::from(2),
            pi: 0,
            e: 0,
        },
        ratio(1, 2),
    )]));
    assert_eq!(form("1/sqrt(2)"), half_root_two);
}

#[test]
fn a_radicand_that_is_not_a_whole_number_stays_a_function() {
    let x = Canon::Poly(BTreeMap::from([(monomial(&[(var("x"), 1)]), whole(1))]));
    assert_eq!(form("sqrt(x)"), Canon::Func("sqrt".to_string(), vec![x]));
    assert_eq!(
        form("sqrt(1/2)"),
        Canon::Func("sqrt".to_string(), vec![Canon::Rational(ratio(1, 2))])
    );
}

#[test]
fn a_polynomial_is_a_sparse_map_of_monomials() {
    let expected = Canon::Poly(BTreeMap::from([
        (monomial(&[]), whole(1)),
        (monomial(&[(var("x"), 1)]), whole(2)),
        (monomial(&[(var("x"), 2)]), whole(1)),
    ]));
    assert_eq!(form("(x+1)**2"), expected);
    assert_eq!(form("x**2+2*x+1"), expected);
    assert_eq!(form("1 + 2x + x^2"), expected);
}

#[test]
fn a_chained_inequality_becomes_a_range_over_its_variable() {
    let expected = Canon::Interval {
        var: Some("x".to_string()),
        lo: Some(Box::new(Canon::Rational(whole(-1)))),
        lo_closed: true,
        hi: Some(Box::new(Canon::Rational(whole(3)))),
        hi_closed: true,
    };
    assert_eq!(form("-1 ≤ x ≤ 3"), expected);
    assert_eq!(form("-1 <= x <= 3"), expected);
    assert_eq!(form("3 >= x >= -1"), expected);
    let open_below = Canon::Interval {
        var: Some("x".to_string()),
        lo: None,
        lo_closed: false,
        hi: Some(Box::new(Canon::Rational(whole(-1)))),
        hi_closed: true,
    };
    assert_eq!(form("x <= -1"), open_below);
    assert_eq!(form("-1 >= x"), open_below);
}

#[test]
fn a_set_is_unordered_and_a_list_and_a_tuple_are_ordered() {
    assert_eq!(check("{1, 3, 5}", "{5, 3, 1}", E), decided(true, false));
    assert_eq!(check("[-3, 3]", "[3, -3]", E), decided(false, false));
    assert_eq!(check("(4, 17)", "(17, 4)", N), decided(false, false));
    // A tuple compares its members by value, which 1.0 cannot do (spec 7.7).
    assert_eq!(check("(4, 17)", "(4, 17.0)", N), decided(true, false));
}

#[test]
fn division_by_a_sum_normalizes_the_content() {
    assert_eq!(check("1/(x+1)", "2/(2*x+2)", E), decided(true, false));
    assert_eq!(check("1/(x+1)", "-1/(-x-1)", E), decided(true, false));
    assert_eq!(check("1/(x+1)", "1/(x+2)", E), decided(false, false));
}

#[test]
fn the_dot_thousands_hole_of_spec_7_2_stays_closed() {
    // `_DOT_GROUPS_RE` needs a non-zero leading group, so a 1000x slip is wrong.
    assert_eq!(check("8", "0.008", N), decided(false, false));
    assert_eq!(check("8", "8.000", N), decided(true, false));
    assert_eq!(check("0.5", "0,5", N), decided(false, false));
}

// ---------------------------------------------------------------------------
// The corpus: coverage and the L2 budget
// ---------------------------------------------------------------------------

/// One corpus row of `crates/core/tests/fixtures/answers/corpus_1_0.jsonl`.
#[derive(serde::Deserialize)]
struct CorpusRow {
    answer: String,
    answer_kind: String,
}

/// Read the answer corpus.
fn corpus() -> Vec<CorpusRow> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/answers/corpus_1_0.jsonl");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}")))
        .collect()
}

/// Read the answer kind of a corpus row.
fn kind_of(row: &CorpusRow) -> AnswerKind {
    match row.answer_kind.as_str() {
        "numeric" => AnswerKind::Numeric,
        "expression" => AnswerKind::Expression,
        other => panic!("the corpus holds only verifiable kinds, and this row is {other}"),
    }
}

#[test]
fn every_answer_the_grammar_accepts_also_canonicalizes() {
    let corpus = corpus();
    assert_eq!(corpus.len(), 3_492, "the corpus is 3,492 answers");
    let mut canonical = 0_usize;
    let mut refused: Vec<(&str, &'static str)> = Vec::new();
    for row in &corpus {
        match canonical_form(&row.answer) {
            Ok(_) => canonical += 1,
            Err(reason) => refused.push((&row.answer, reason.reason)),
        }
    }
    // FIXM2a pins 265 answers as outside the grammar (`undecidable_1_0.jsonl`),
    // so 3,492 - 265 = 3,227 answers parse. Every one of them canonicalizes.
    assert_eq!(
        canonical,
        3_227,
        "the first refusals are {:?}",
        refused.iter().take(5).collect::<Vec<_>>()
    );
}

#[test]
fn the_corpus_self_check_holds_the_l2_budget() {
    let corpus = corpus();
    let mut worst = Duration::ZERO;
    let mut worst_answer = "";
    let start = Instant::now();
    for row in &corpus {
        let one = Instant::now();
        let outcome = check(&row.answer, &row.answer, kind_of(row));
        let elapsed = one.elapsed();
        if elapsed > worst {
            worst = elapsed;
            worst_answer = &row.answer;
        }
        assert_eq!(
            outcome,
            decided(true, false),
            "an answer must equal itself: {:?}",
            row.answer
        );
    }
    let total = start.elapsed();
    assert!(
        total < Duration::from_secs(1),
        "the 3,492 self-checks took {total:?}, and the budget is 1 s"
    );
    assert!(
        worst < Duration::from_millis(5),
        "the longest single check took {worst:?} on {worst_answer:?}, and the budget is 5 ms"
    );
}

#[test]
fn the_corpus_canonicalization_holds_the_l2_budget() {
    // The self-check above stops on the string rung, so it never reaches the
    // arithmetic. This test drives the whole path and asserts the same budget.
    let corpus = corpus();
    let mut worst = Duration::ZERO;
    let mut worst_answer = "";
    let start = Instant::now();
    for row in &corpus {
        let one = Instant::now();
        let _ = canonical_form(&row.answer);
        let elapsed = one.elapsed();
        if elapsed > worst {
            worst = elapsed;
            worst_answer = &row.answer;
        }
    }
    let total = start.elapsed();
    assert!(
        total < Duration::from_secs(1),
        "the 3,492 canonicalizations took {total:?}, and the budget is 1 s"
    );
    assert!(
        worst < Duration::from_millis(5),
        "the longest canonicalization took {worst:?} on {worst_answer:?}, and the budget is 5 ms"
    );
}

// ---------------------------------------------------------------------------
// The checker never panics
// ---------------------------------------------------------------------------

/// A deterministic xorshift generator. The fuzz needs no crate for this.
struct Rng(u64);

impl Rng {
    /// Return the next pseudo-random word.
    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state
    }

    /// Return a value below `bound`.
    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        let bound = u64::try_from(bound).unwrap_or(u64::MAX);
        usize::try_from(self.next() % bound).unwrap_or(0)
    }
}

/// The characters a hostile learner reaches for.
const FUZZ_ALPHABET: [&str; 40] = [
    "0", "1", "9", ".", ",", "+", "-", "*", "/", "**", "^", "(", ")", "[", "]", "{", "}", "<", "=",
    ">", "x", "y", "sqrt", "sin", "log", "exp", "pi", "e", "theta", "%", "$", "\\frac", "\\sqrt",
    "√", "π", "∞", "≤", "²", " ", "\u{202f}",
];

/// Build one random string from the alphabet.
fn fuzz_string(rng: &mut Rng, tokens: usize) -> String {
    let mut out = String::new();
    for _ in 0..tokens {
        let index = rng.below(FUZZ_ALPHABET.len());
        out.push_str(FUZZ_ALPHABET.get(index).copied().unwrap_or("0"));
    }
    out
}

/// Build one random answer: half raw bytes, half tokens of the alphabet.
fn fuzz_case(rng: &mut Rng) -> String {
    if rng.next().is_multiple_of(2) {
        let length = 1 + rng.below(64);
        fuzz_bytes(rng, length)
    } else {
        let tokens = 1 + rng.below(24);
        fuzz_string(rng, tokens)
    }
}

/// Build one random byte string, read as lossy UTF-8.
fn fuzz_bytes(rng: &mut Rng, length: usize) -> String {
    let mut bytes = Vec::with_capacity(length);
    for _ in 0..length {
        bytes.push(u8::try_from(rng.next() % 256).unwrap_or(0));
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn the_checker_never_panics_on_random_input() {
    let mut rng = Rng(0x2026_0826_u64 | 1);
    let start = Instant::now();
    let mut cases = 0_u64;
    while start.elapsed() < Duration::from_secs(10) {
        for _ in 0..64 {
            let expected = fuzz_case(&mut rng);
            let learner = fuzz_case(&mut rng);
            let kind = if rng.next().is_multiple_of(2) { N } else { E };
            let probe = (expected.clone(), learner.clone(), kind);
            let result = std::panic::catch_unwind(move || check(&probe.0, &probe.1, probe.2));
            assert!(
                result.is_ok(),
                "the checker panicked on {expected:?} against {learner:?} on {kind}"
            );
            cases += 1;
        }
    }
    assert!(cases > 1_000, "the fuzz ran only {cases} cases");
}

#[test]
fn an_answer_past_the_input_cap_is_undecidable() {
    let long = "1".repeat(4_001);
    assert!(matches!(check("1", &long, N), Outcome::Undecidable(_)));
    assert!(matches!(check(&long, "1", N), Outcome::Undecidable(_)));
    let at_cap = "1".repeat(4_000);
    assert_eq!(check(&at_cap, &at_cap, N), decided(true, false));
}
