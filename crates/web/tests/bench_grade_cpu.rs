//! Benchmark A, the grade half of the request tier: check, tier, and hash.
//!
//! Requirements: L2 (grade a verifiable answer, p95 < 300 ms), T1 (the grade
//! path spends no model token), A3 (the deterministic verdict), C4 (`correct`
//! comes from `cadus_core::answer::check` alone), D6 (no float in a value or an
//! equality decision).
//!
//! `docs/reference/l1-budget.md` gives the CPU half of the grade path 5 ms of
//! the 300 ms of L2. `crates/core/tests/bench_l1.rs` measures `check` alone.
//! This file measures the three pieces the HANDLER runs on one submission, in
//! the order it runs them:
//!
//! 1. `cadus_core::learner::problem_text_hash` of the served statement — the D5
//!    digest;
//! 2. `cadus_web::grade::deterministic_grade` — `check` and the D-M5-2 work
//!    quality tier and the error tags, one call.
//!
//! The tier is the addition M5 U12 makes: M4 measured the checker, and no
//! benchmark read the function the grade route calls. The measured function is
//! the PRODUCTION function, so a tier rule that grows a `format!` in its hot
//! path moves this p95 and not a copy of it.
//!
//! # The corpus
//!
//! The fixture is `crates/core/tests/fixtures/answers/corpus_1_0.jsonl`, every
//! authored answer of the 1.0 curriculum. The learner side is built from it, so
//! every one of the three tiers is measured on real answers:
//!
//! - a re-spelling of the authored answer, which grades `nearly_perfect`;
//! - a value that is not the authored answer, which grades `nearly_passable`;
//! - a blank, which grades `poor` with the `blank-answer` tag.
//!
//! `crates/core/tests/bench_l1.rs` states why a re-spelling and not the
//! authored answer itself: `check` returns at rung 2 when the two normalized
//! string keys agree, so a self-check measures a string compare and never the
//! parser (M4 review 1, finding 20).
//!
//! # How to run it
//!
//! `scripts/bench.sh` runs this file in the release profile. Without
//! `CADUS_BENCH` in the environment it prints one skip line and returns, so
//! `cargo test --workspace` stays a test run (spec section 10.5).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use cadus_core::answer::normalize;
use cadus_core::curriculum::AnswerKind;
use cadus_core::event::WorkQuality;
use cadus_core::learner::problem_text_hash;
use cadus_web::grade::deterministic_grade;

/// The count of rows the 1.0 answer corpus holds.
const CORPUS_ANSWERS: usize = 3_492;

/// The p95 budget of one grade, in nanoseconds: 5 ms of the 300 ms of L2.
const GRADE_P95_BUDGET_NS: u128 = 5_000_000;

/// The debug-profile multiplier of the budget.
///
/// The exact arithmetic runs about ten times slower without optimization, which
/// is the rule `crates/core/tests/bench_l1.rs` and
/// `crates/core/tests/answer_check.rs` already carry.
const DEBUG_SLOWDOWN: u128 = 10;

/// Every `BLANK_STRIDE`-th row is graded blank.
const BLANK_STRIDE: usize = 10;

/// The remainder that marks a row graded against a wrong answer.
const WRONG_REMAINDER: usize = 1;

/// The count of rows the run grades blank.
const BLANK_ROWS: usize = 350;

/// The count of rows the run grades against a wrong value.
const WRONG_ROWS: usize = 350;

/// The count of rows the run grades against a re-spelling.
const RESPELLED_ROWS: usize = 2_792;

/// The count of grades that come back `correct`.
const CORRECT_GRADES: usize = 2_393;

/// The count of grades that come back `poor`.
///
/// A blank is the only input that reaches the `poor` tier of D-M5-2, so this
/// count and [`BLANK_ROWS`] are the same number by the rule, not by luck.
const POOR_GRADES: usize = 350;

/// The count of grades that come back `nearly_passable`.
const NEARLY_PASSABLE_GRADES: usize = 749;

/// The count of grades that carry the `blank-answer` tag.
const BLANK_TAGS: usize = 350;

/// The p50 floor of one grade, in nanoseconds.
///
/// The backstop of `crates/core/tests/bench_l1.rs`: a p50 under this floor says
/// the loop short-circuited before the canonicalizer and measured a string
/// compare.
const GRADE_P50_FLOOR_NS: u128 = 1_000;

/// The environment variable that turns the benchmarks on.
const BENCH_VAR: &str = "CADUS_BENCH";

/// The environment variable that moves the artifact directory.
const ARTIFACT_DIR_VAR: &str = "CADUS_BENCH_DIR";

// --------------------------------------------------------------------------- //
// Percentiles and the artifact
// --------------------------------------------------------------------------- //

/// The `percent` percentile of a sorted sample, by the nearest-rank rule.
///
/// The rank is `ceil(percent * n / 100)`, counted from one. The arithmetic is
/// integer arithmetic, so no float enters a reported number (D6).
fn percentile(sorted: &[u128], percent: u128) -> u128 {
    assert!(!sorted.is_empty(), "a percentile needs a sample");
    let count = sorted.len() as u128;
    let rank = (percent * count).div_ceil(100).max(1);
    let index = usize::try_from(rank - 1).unwrap_or(0);
    sorted[index.min(sorted.len() - 1)]
}

/// The p50, p95, p99, and maximum of a sample of nanosecond durations.
struct Percentiles {
    p50: u128,
    p95: u128,
    p99: u128,
    max: u128,
}

impl Percentiles {
    /// Read the percentiles of one sample. The function sorts its own copy.
    fn of(samples: &[u128]) -> Self {
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        Self {
            p50: percentile(&sorted, 50),
            p95: percentile(&sorted, 95),
            p99: percentile(&sorted, 99),
            max: *sorted.last().unwrap(),
        }
    }
}

/// The name of the build profile, for the artifact.
fn profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

/// The time budget of this build profile.
fn budget(release_ns: u128) -> u128 {
    if cfg!(debug_assertions) {
        release_ns * DEBUG_SLOWDOWN
    } else {
        release_ns
    }
}

/// Write the benchmark artifact and print its path.
fn write_artifact(body: &str) {
    let dir = match std::env::var_os(ARTIFACT_DIR_VAR) {
        Some(value) => PathBuf::from(value),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench"),
    };
    std::fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let path = dir.join("benchmark-a-grade.json");
    std::fs::write(&path, body).unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
    println!("artifact: {}", path.display());
}

// --------------------------------------------------------------------------- //
// The corpus
// --------------------------------------------------------------------------- //

/// One row of the 1.0 answer corpus.
#[derive(serde::Deserialize)]
struct CorpusRow {
    answer: String,
    answer_kind: String,
}

/// Read the 1.0 answer corpus. It lives with the core tests that first read it.
fn corpus() -> Vec<CorpusRow> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures/answers/corpus_1_0.jsonl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|err| panic!("row {line}: {err}")))
        .collect()
}

/// The answer kind of one corpus row.
fn kind_of(row: &CorpusRow) -> AnswerKind {
    match row.answer_kind.as_str() {
        "numeric" => AnswerKind::Numeric,
        "expression" => AnswerKind::Expression,
        other => panic!("the corpus holds only verifiable kinds, and this row is {other}"),
    }
}

/// The learner side of one row: a blank, a wrong value, or a re-spelling.
///
/// The rule is a stride over the row index, so the three shares are a function
/// of the committed corpus alone and the counts above are literals of it.
fn learner_answer(index: usize, row: &CorpusRow) -> String {
    if index.is_multiple_of(BLANK_STRIDE) {
        return String::new();
    }
    // Re-spell the reader source and not the authored text. The author wraps an
    // answer in `$...$`, and `normalize` strips that pair at the two ends only.
    let source = normalize(&row.answer).source;
    match (index % BLANK_STRIDE, kind_of(row)) {
        (WRONG_REMAINDER, AnswerKind::Numeric) => format!("{source}+1"),
        (WRONG_REMAINDER, _) => format!("({source})*2"),
        (_, AnswerKind::Numeric) => format!("{source}+0"),
        (_, _) => format!("({source})*1"),
    }
}

// --------------------------------------------------------------------------- //
// The benchmark
// --------------------------------------------------------------------------- //

/// Benchmark A: the hash and the deterministic grade hold the 5 ms of L2.
#[test]
fn benchmark_a_grade_cpu_holds_the_l2_segment() {
    if std::env::var_os(BENCH_VAR).is_none() {
        println!("SKIPPED benchmark A (grade CPU): {BENCH_VAR} is not set");
        return;
    }
    let corpus = corpus();
    assert_eq!(corpus.len(), CORPUS_ANSWERS, "the corpus is 3,492 answers");

    // Build every input before the loop. A `format!` of the fixture is fixture
    // work, and fixture work never enters a measured sample.
    let cases: Vec<(String, String, String, AnswerKind)> = corpus
        .iter()
        .enumerate()
        .map(|(index, row)| {
            (
                format!("Problem {index}. State the value."),
                row.answer.clone(),
                learner_answer(index, row),
                kind_of(row),
            )
        })
        .collect();

    let blanks = cases.iter().filter(|case| case.2.is_empty()).count();
    let respelled = cases
        .iter()
        .enumerate()
        .filter(|(index, _)| index % BLANK_STRIDE > WRONG_REMAINDER)
        .count();
    let wrong = cases.len() - blanks - respelled;

    let mut samples: Vec<u128> = Vec::with_capacity(cases.len());
    let mut digests: Vec<String> = Vec::with_capacity(cases.len());
    let mut correct = 0_usize;
    let mut poor = 0_usize;
    let mut nearly_passable = 0_usize;
    let mut blank_tags = 0_usize;
    for (statement, expected, learner, kind) in &cases {
        let start = Instant::now();
        let digest = problem_text_hash(statement);
        let grade = deterministic_grade(expected, learner, *kind);
        samples.push(start.elapsed().as_nanos());
        digests.push(digest);
        if grade.correct {
            correct += 1;
        }
        match grade.work_quality {
            WorkQuality::Poor => poor += 1,
            WorkQuality::NearlyPassable => nearly_passable += 1,
            _ => {}
        }
        if grade.error_tags.iter().any(|tag| tag == "blank-answer") {
            blank_tags += 1;
        }
    }

    let times = Percentiles::of(&samples);
    let budget = budget(GRADE_P95_BUDGET_NS);
    // Report first, then assert: the review cycle reads the failing run too
    // (spec section 10.2).
    println!(
        "benchmark A grade ({}): p50 {} ns, p95 {} ns, p99 {} ns, max {} ns over {} rows \
         ({} blank, {} wrong, {} respelled; {} correct, {} poor, {} nearly_passable)",
        profile(),
        times.p50,
        times.p95,
        times.p99,
        times.max,
        cases.len(),
        blanks,
        wrong,
        respelled,
        correct,
        poor,
        nearly_passable,
    );
    write_artifact(&format!(
        "{{\n  \"benchmark\": \"A-grade\",\n  \"profile\": {:?},\n  \"rows\": {},\n  \
         \"inputs\": {{\"blank\": {}, \"wrong\": {}, \"respelled\": {}}},\n  \
         \"verdicts\": {{\"correct\": {}, \"poor\": {}, \"nearly_passable\": {}}},\n  \
         \"grade_ns\": {{\"p50_ns\": {}, \"p95_ns\": {}, \"p99_ns\": {}, \"max_ns\": {}}},\n  \
         \"p95_budget_ns\": {}\n}}\n",
        profile(),
        cases.len(),
        blanks,
        wrong,
        respelled,
        correct,
        poor,
        nearly_passable,
        times.p50,
        times.p95,
        times.p99,
        times.max,
        budget,
    ));

    assert_eq!(samples.len(), CORPUS_ANSWERS, "every row is measured");
    assert_eq!(blanks, BLANK_ROWS, "the blank share of the corpus changed");
    assert_eq!(wrong, WRONG_ROWS, "the wrong share of the corpus changed");
    assert_eq!(
        respelled, RESPELLED_ROWS,
        "the re-spelled share of the corpus changed"
    );
    assert_eq!(
        digests.len(),
        CORPUS_ANSWERS,
        "a measured iteration produced no digest"
    );
    assert!(
        digests.iter().all(|digest| digest.len() == 12),
        "problem_text_hash returned a digest of another width"
    );
    assert_eq!(
        correct, CORRECT_GRADES,
        "the count of accepted re-spellings changed"
    );
    assert_eq!(poor, POOR_GRADES, "the count of poor grades changed");
    assert_eq!(
        nearly_passable, NEARLY_PASSABLE_GRADES,
        "the count of nearly_passable grades changed"
    );
    assert_eq!(
        blank_tags, BLANK_TAGS,
        "the count of blank-answer tags changed"
    );
    assert!(
        times.p95 < budget,
        "the p95 grade took {} ns, and the budget is {budget} ns",
        times.p95
    );
    assert!(
        times.p50 > GRADE_P50_FLOOR_NS,
        "the p50 grade took {} ns, under the {GRADE_P50_FLOOR_NS} ns floor, so the loop \
         short-circuited before the canonicalizer",
        times.p50
    );
}
