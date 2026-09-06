//! Benchmark A, part 2: the `answer::check` segment of the L2 budget over the
//! whole 1.0 answer corpus.
//!
//! Requirement L2 (grade a verifiable answer, p95 < 300 ms), T1 (no model token
//! on the path), D6 (no float in a value or an equality decision). The 5 ms
//! segment is the row of `docs/reference/l1-budget.md` this file measures. See
//! `bench_l1.rs` for why the benchmark is a plain test, and for how to run it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;
use std::time::Instant;

use cadus_core::answer::{Outcome, canonical_form, check, normalize};
use cadus_core::curriculum::AnswerKind;
use cadus_testkit::bench::{
    BENCH_VAR, Percentiles, benchmarks_are_on, budget, profile, write_artifact,
};
use cadus_testkit::fixtures::read_jsonl;

/// The p95 budget of one `check`, in nanoseconds: 5 ms of the 300 ms of L2.
const CHECK_P95_BUDGET_NS: u128 = 5_000_000;

/// The count of answers in the 1.0 corpus (`docs/plans/M2.md`).
const CORPUS_ANSWERS: usize = 3_492;

/// The count of corpus pairs whose two sides both reach a canonical form.
///
/// The L2 half compares each authored answer against a re-spelled equivalent
/// (see [`respell`]). A pair of this count runs the parser and the exact
/// canonicalizer on BOTH sides, so this literal is the count of measured calls
/// that reach the arithmetic the 5 ms segment pays for. It is the counter that
/// proves the string rung did not answer the run: 2,994 of the 3,492 calls take
/// the parse-and-canonicalize path, where the self-check of M4 review 1
/// finding 20 took it zero times.
///
/// The other 498 pairs still run the parser but do not produce canonical forms
/// on both sides. The expanded grammar moved nine pairs into the exact
/// canonical path; their answer contract still refuses a decided verdict, so
/// the verdict totals below remain 2,985 correct and 507 undecidable.
const CANONICALIZED_PAIRS: usize = 2_994;

/// The count of re-spelled pairs the checker decides correct.
const RESPELLED_CORRECT: usize = 2_985;

/// The count of re-spelled pairs the checker decides wrong.
///
/// A wrong verdict here is a canonicalizer that does not see `(e)*1` as `e` or
/// `n+0` as `n`. The literal pins that count.
const RESPELLED_WRONG: usize = 0;

/// The count of re-spelled pairs the checker refuses (V2).
const RESPELLED_UNDECIDABLE: usize = 507;

/// The p50 floor of the L2 half, in nanoseconds.
///
/// The floor is the second guard against a silent return to the string rung.
/// Finding 20 of M4 review 1 measured the self-check at a p50 of 450 ns and the
/// re-spelled check at a p50 of 2,620 ns on this box. A p50 under this floor
/// means the loop stopped reaching the canonicalizer, so the reported number is
/// not the number of the grade path. The floor is deliberately far under the
/// measured p50: it catches a rung-2 short circuit, not a fast machine.
///
/// The count assertions above are the primary proof. This floor is the backstop
/// for a change that keeps the counts and drops the work.
const CHECK_P50_FLOOR_NS: u128 = 1_000;

// ---------------------------------------------------------------------------
// Benchmark A, part 2 — the M2 check on the corpus (L2)
// ---------------------------------------------------------------------------

/// One row of `crates/core/tests/fixtures/answers/corpus_1_0.jsonl`.
#[derive(serde::Deserialize)]
struct CorpusRow {
    answer: String,
    answer_kind: String,
}

/// Read the 1.0 answer corpus.
fn corpus() -> Vec<CorpusRow> {
    read_jsonl(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/answers/corpus_1_0.jsonl"),
    )
}

/// The answer kind of one corpus row.
fn kind_of(row: &CorpusRow) -> AnswerKind {
    match row.answer_kind.as_str() {
        "numeric" => AnswerKind::Numeric,
        "expression" => AnswerKind::Expression,
        other => panic!("the corpus holds only verifiable kinds, and this row is {other}"),
    }
}

/// The re-spelling of one corpus answer: the same value, a different string.
///
/// `check` returns at rung 2 when the two normalized string keys agree, so a
/// self-check measures `normalize` and a string compare and never the parser or
/// the canonicalizer (M4 review 1, finding 20). The learner side of the L2 half
/// is therefore a re-spelling:
///
/// - a `numeric` answer takes `+0`;
/// - an `expression` answer goes inside parentheses and takes `*1`.
///
/// Both rewrites keep the value and change the string key, so every measured
/// call walks past rung 2 into the parser and the exact arithmetic. A learner
/// types the same kind of variant, so the measured cost is the cost of the M5
/// grade path.
fn respell(row: &CorpusRow) -> String {
    // Re-spell the reader source, not the authored text. The author wraps an
    // answer in `$...$`, and `normalize` strips that pair at the two ends only;
    // a `$` in the middle of `($3\pi$)*1` leaves the grammar. The source string
    // is what a learner types, so the rewrite starts from it.
    let source = normalize(&row.answer).source;
    match kind_of(row) {
        AnswerKind::Numeric => format!("{source}+0"),
        AnswerKind::Expression => format!("({source})*1"),
        // `kind_of` panics on every other kind, so this arm is unreachable. It
        // keeps the match exhaustive without a wildcard that hides a new kind.
        other => panic!("the corpus holds only verifiable kinds, and this row is {other:?}"),
    }
}

/// Benchmark A: `check` over the whole 1.0 corpus holds the 5 ms segment of L2.
///
/// The grade path of M5 runs one `check` per attempt (A3, D-O2). The corpus is
/// every authored answer of the 1.0 curriculum, and the learner side of each
/// pair is the [`respell`] variant of it, so the p95 this test reports is the
/// p95 of a check that runs the parser and the canonicalizer.
#[test]
fn benchmark_a_check_holds_the_l2_segment() {
    if !benchmarks_are_on() {
        println!("SKIPPED benchmark A (check): {BENCH_VAR} is not set");
        return;
    }
    let corpus = corpus();
    assert_eq!(corpus.len(), CORPUS_ANSWERS, "the corpus is 3,492 answers");

    // Build both sides before the loop. The `format!` of the re-spelling is
    // fixture work, and fixture work never enters a measured sample.
    let pairs: Vec<(&str, String, AnswerKind)> = corpus
        .iter()
        .map(|row| (row.answer.as_str(), respell(row), kind_of(row)))
        .collect();

    // Guard 1: no measured call stops at the string rung. Rung 2 returns when
    // the two normalized string keys agree, so every pair of this run carries
    // two different keys.
    let different_keys = pairs
        .iter()
        .filter(|(expected, learner, _)| {
            normalize(expected).string_key != normalize(learner).string_key
        })
        .count();
    assert_eq!(
        different_keys, CORPUS_ANSWERS,
        "a pair with two equal string keys returns at rung 2 and measures no arithmetic"
    );

    // Guard 2: the count of pairs whose two sides both reach a canonical form.
    // Those are the calls that run the exact arithmetic of the L2 budget. The
    // count runs outside the measured loop, so it costs the reported numbers
    // nothing.
    let canonicalized = pairs
        .iter()
        .filter(|(expected, learner, _)| {
            canonical_form(expected).is_ok() && canonical_form(learner).is_ok()
        })
        .count();
    assert_eq!(
        canonicalized, CANONICALIZED_PAIRS,
        "the count of pairs that reach the canonicalizer changed"
    );

    let mut samples: Vec<u128> = Vec::with_capacity(pairs.len());
    let mut correct = 0_usize;
    let mut wrong = 0_usize;
    let mut undecidable = 0_usize;
    for (expected, learner, kind) in &pairs {
        let start = Instant::now();
        let outcome = check(expected, learner, *kind);
        samples.push(start.elapsed().as_nanos());
        match outcome {
            Outcome::Decided(verdict) if verdict.correct => correct += 1,
            Outcome::Decided(_) => wrong += 1,
            Outcome::Undecidable(_) => undecidable += 1,
        }
    }

    let times = Percentiles::of(&samples);
    let budget = budget(CHECK_P95_BUDGET_NS);
    // Report first, then assert, for the same reason the instantiation half
    // reports first: the review cycle reads a failing run as well.
    println!(
        "benchmark A check ({}): p50 {} ns, p95 {} ns, p99 {} ns, max {} ns over {} answers \
         ({} correct, {} wrong, {} undecidable, {} canonicalized)",
        profile(),
        times.p50,
        times.p95,
        times.p99,
        times.max,
        pairs.len(),
        correct,
        wrong,
        undecidable,
        canonicalized,
    );
    write_artifact(
        "benchmark-a-check.json",
        &format!(
            "{{\n  \"benchmark\": \"A-check\",\n  \"profile\": {:?},\n  \"answers\": {},\n  \
             \"learner_side\": \"respelled\",\n  \"canonicalized_pairs\": {},\n  \
             \"verdicts\": {{\"correct\": {}, \"wrong\": {}, \"undecidable\": {}}},\n  \
             \"check_ns\": {},\n  \"p95_budget_ns\": {},\n  \"p50_floor_ns\": {}\n}}\n",
            profile(),
            pairs.len(),
            canonicalized,
            correct,
            wrong,
            undecidable,
            times.json(),
            budget,
            CHECK_P50_FLOOR_NS,
        ),
    );

    assert_eq!(
        correct, RESPELLED_CORRECT,
        "the count of re-spellings the checker accepts changed"
    );
    assert_eq!(
        wrong, RESPELLED_WRONG,
        "the checker called a re-spelling of an answer a different answer"
    );
    assert_eq!(
        undecidable, RESPELLED_UNDECIDABLE,
        "the count of pairs outside the decidable grammar changed"
    );
    assert!(
        times.p95 < budget,
        "the p95 check took {} ns, and the budget is {budget} ns",
        times.p95
    );
    assert!(
        times.p50 > CHECK_P50_FLOOR_NS,
        "the p50 check took {} ns, under the {CHECK_P50_FLOOR_NS} ns floor, so the loop \
         short-circuited before the canonicalizer",
        times.p50
    );
}
