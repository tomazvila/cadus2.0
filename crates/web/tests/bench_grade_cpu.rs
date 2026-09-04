//! Benchmark A, the grade half of the request tier: check, tier, and hash.
//!
//! Requirements: L2 (grade a verifiable answer, p95 < 300 ms), T1 (the grade
//! path spends no model token), A3 (the deterministic verdict), C4 (`correct`
//! comes from `cadus_core::answer::check` alone), D6 (no float in a value or an
//! equality decision).
//!
//! `docs/reference/l1-budget.md` gives the CPU half of the grade path 5 ms of
//! the 300 ms of L2. `crates/core/tests/bench_l1.rs` measures `check` alone.
//! This file measures the two pieces the HANDLER runs on one submission, in
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
//! `cargo test --workspace` stays a test run (spec section 10.5). The helpers
//! of the run have their own tests below, which run either way.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::Instant;

use cadus_core::answer::normalize;
use cadus_core::curriculum::AnswerKind;
use cadus_core::event::WorkQuality;
use cadus_core::learner::problem_text_hash;
use cadus_web::grade::{Grade, deterministic_grade};
use common::{
    BENCH_VAR, CORPUS_ROWS, CorpusRow, Percentiles, benchmarks_are_on, budget, corpus, kind_of,
    profile, write_artifact,
};

/// The p95 budget of one grade, in nanoseconds: 5 ms of the 300 ms of L2.
const GRADE_P95_BUDGET_NS: u128 = 5_000_000;

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

/// The width of a `problem_text_hash` digest, in hex characters.
const DIGEST_WIDTH: usize = 12;

// --------------------------------------------------------------------------- //
// The inputs
// --------------------------------------------------------------------------- //

/// One measured submission: the served statement, the authored answer, the
/// learner's answer, and the grammar.
struct Case {
    statement: String,
    expected: String,
    learner: String,
    kind: AnswerKind,
}

/// The re-spelling of one reader source: a wrong value, or the same value in
/// another string.
///
/// The rewrite starts from the reader source and not the authored text: the
/// author wraps an answer in `$...$`, and `normalize` strips that pair at the
/// two ends only.
fn respell(source: &str, wrong: bool, kind: AnswerKind) -> String {
    match (wrong, kind) {
        (true, AnswerKind::Numeric) => format!("{source}+1"),
        (true, _) => format!("({source})*2"),
        (false, AnswerKind::Numeric) => format!("{source}+0"),
        (false, _) => format!("({source})*1"),
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
    let source = normalize(&row.answer).source;
    respell(
        &source,
        index % BLANK_STRIDE == WRONG_REMAINDER,
        kind_of(row),
    )
}

/// Every input of the run, built before the loop. A `format!` of the fixture
/// is fixture work, and fixture work never enters a measured sample.
fn cases(corpus: &[CorpusRow]) -> Vec<Case> {
    corpus
        .iter()
        .enumerate()
        .map(|(index, row)| Case {
            statement: format!("Problem {index}. State the value."),
            expected: row.answer.clone(),
            learner: learner_answer(index, row),
            kind: kind_of(row),
        })
        .collect()
}

/// The three shares of the inputs.
struct Shares {
    blanks: usize,
    wrong: usize,
    respelled: usize,
}

/// Count the three shares of `cases`.
fn shares(cases: &[Case]) -> Shares {
    let blanks = cases.iter().filter(|case| case.learner.is_empty()).count();
    let respelled = cases
        .iter()
        .enumerate()
        .filter(|(index, _)| index % BLANK_STRIDE > WRONG_REMAINDER)
        .count();
    Shares {
        blanks,
        wrong: cases.len() - blanks - respelled,
        respelled,
    }
}

// --------------------------------------------------------------------------- //
// The verdict tally
// --------------------------------------------------------------------------- //

/// How many grades of the run came back at each tier.
#[derive(Default)]
struct Tally {
    correct: usize,
    poor: usize,
    nearly_passable: usize,
    blank_tags: usize,
}

impl Tally {
    /// Count one grade.
    fn add(&mut self, grade: &Grade) {
        if grade.correct {
            self.correct += 1;
        }
        match grade.work_quality {
            WorkQuality::Poor => self.poor += 1,
            WorkQuality::NearlyPassable => self.nearly_passable += 1,
            _ => {}
        }
        if grade.error_tags.iter().any(|tag| tag == "blank-answer") {
            self.blank_tags += 1;
        }
    }
}

/// What one measured run produced.
struct Run {
    samples: Vec<u128>,
    digests: Vec<String>,
    tally: Tally,
}

/// Time the hash and the grade of every case, in the order the handler runs
/// them.
fn measure(cases: &[Case]) -> Run {
    let mut run = Run {
        samples: Vec::with_capacity(cases.len()),
        digests: Vec::with_capacity(cases.len()),
        tally: Tally::default(),
    };
    for case in cases {
        let start = Instant::now();
        let digest = problem_text_hash(&case.statement);
        let grade = deterministic_grade(&case.expected, &case.learner, case.kind);
        run.samples.push(start.elapsed().as_nanos());
        run.digests.push(digest);
        run.tally.add(&grade);
    }
    run
}

// --------------------------------------------------------------------------- //
// The report and the assertions
// --------------------------------------------------------------------------- //

/// The JSON body of the artifact.
fn artifact_body(rows: usize, shares: &Shares, tally: &Tally, times: &Percentiles) -> String {
    format!(
        "{{\n  \"benchmark\": \"A-grade\",\n  \"profile\": {:?},\n  \"rows\": {rows},\n  \
         \"inputs\": {{\"blank\": {}, \"wrong\": {}, \"respelled\": {}}},\n  \
         \"verdicts\": {{\"correct\": {}, \"poor\": {}, \"nearly_passable\": {}}},\n  \
         \"grade_ns\": {},\n  \"p95_budget_ns\": {}\n}}\n",
        profile(),
        shares.blanks,
        shares.wrong,
        shares.respelled,
        tally.correct,
        tally.poor,
        tally.nearly_passable,
        times.json(),
        budget(GRADE_P95_BUDGET_NS),
    )
}

/// The three shares are the literals of the committed corpus.
fn assert_shares(shares: &Shares) {
    assert_eq!(
        shares.blanks, BLANK_ROWS,
        "the blank share of the corpus changed"
    );
    assert_eq!(
        shares.wrong, WRONG_ROWS,
        "the wrong share of the corpus changed"
    );
    assert_eq!(
        shares.respelled, RESPELLED_ROWS,
        "the re-spelled share of the corpus changed"
    );
}

/// Every row is measured, every iteration produced a digest of the D5 width,
/// and the verdict counts are the literals of the committed corpus.
fn assert_verdicts(run: &Run) {
    assert_eq!(run.samples.len(), CORPUS_ROWS, "every row is measured");
    assert_eq!(
        run.digests.len(),
        CORPUS_ROWS,
        "a measured iteration produced no digest"
    );
    assert!(
        run.digests
            .iter()
            .all(|digest| digest.len() == DIGEST_WIDTH),
        "problem_text_hash returned a digest of another width"
    );
    assert_eq!(
        run.tally.correct, CORRECT_GRADES,
        "the count of accepted re-spellings changed"
    );
    assert_eq!(
        run.tally.poor, POOR_GRADES,
        "the count of poor grades changed"
    );
    assert_eq!(
        run.tally.nearly_passable, NEARLY_PASSABLE_GRADES,
        "the count of nearly_passable grades changed"
    );
    assert_eq!(
        run.tally.blank_tags, BLANK_TAGS,
        "the count of blank-answer tags changed"
    );
}

/// The p95 holds the budget, and the p50 stands above the short-circuit floor.
fn assert_times(times: &Percentiles) {
    let budget = budget(GRADE_P95_BUDGET_NS);
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

/// The whole run: the corpus, the loop, the report, then the assertions.
///
/// The report comes first, so the review cycle reads the failing run too
/// (spec section 10.2).
fn run_benchmark() {
    let corpus = corpus();
    assert_eq!(corpus.len(), CORPUS_ROWS, "the corpus is 3,492 answers");
    let cases = cases(&corpus);
    let shares = shares(&cases);
    let run = measure(&cases);
    let times = Percentiles::of(&run.samples);
    println!(
        "benchmark A grade ({}): {} over {} rows ({} blank, {} wrong, {} respelled; \
         {} correct, {} poor, {} nearly_passable)",
        profile(),
        times.phrase(),
        cases.len(),
        shares.blanks,
        shares.wrong,
        shares.respelled,
        run.tally.correct,
        run.tally.poor,
        run.tally.nearly_passable,
    );
    write_artifact(
        "benchmark-a-grade.json",
        &artifact_body(cases.len(), &shares, &run.tally, &times),
    );
    assert_shares(&shares);
    assert_verdicts(&run);
    assert_times(&times);
}

/// Benchmark A: the hash and the deterministic grade hold the 5 ms of L2.
#[test]
fn benchmark_a_grade_cpu_holds_the_l2_segment() {
    if !benchmarks_are_on() {
        println!("SKIPPED benchmark A (grade CPU): {BENCH_VAR} is not set");
        return;
    }
    run_benchmark();
}

// --------------------------------------------------------------------------- //
// The helpers, tested without the benchmark switch
// --------------------------------------------------------------------------- //

/// One corpus row of `kind` with the authored answer `answer`.
fn row(answer: &str, kind: &str) -> CorpusRow {
    CorpusRow {
        answer: answer.to_string(),
        answer_kind: kind.to_string(),
    }
}

/// The stride rule: index 0 is blank, index 1 is wrong, every other index is
/// a re-spelling, and each rewrite follows the grammar of the row.
#[test]
fn the_learner_answer_follows_the_stride_rule() {
    let numeric = row("$12$", "numeric");
    let expression = row("2*x+1", "expression");
    assert_eq!(learner_answer(0, &numeric), "");
    assert_eq!(learner_answer(1, &numeric), "12+1");
    assert_eq!(learner_answer(2, &numeric), "12+0");
    assert_eq!(learner_answer(11, &expression), "(2*x+1)*2");
    assert_eq!(learner_answer(12, &expression), "(2*x+1)*1");
    assert_eq!(learner_answer(20, &expression), "");
}

/// The tally reads the three tiers and the blank tag of a grade.
#[test]
fn the_tally_counts_every_tier_once() {
    let mut tally = Tally::default();
    tally.add(&deterministic_grade("12", "12+0", AnswerKind::Numeric));
    tally.add(&deterministic_grade("12", "12+1", AnswerKind::Numeric));
    tally.add(&deterministic_grade("12", "", AnswerKind::Numeric));
    assert_eq!(
        (
            tally.correct,
            tally.poor,
            tally.nearly_passable,
            tally.blank_tags
        ),
        (1, 1, 1, 1)
    );
}

/// The shares of a ten-row slice are one blank, one wrong, eight re-spelled.
#[test]
fn the_shares_follow_the_stride() {
    let rows: Vec<CorpusRow> = (0..10).map(|_| row("7", "numeric")).collect();
    let counted = shares(&cases(&rows));
    assert_eq!(
        (counted.blanks, counted.wrong, counted.respelled),
        (1, 1, 8)
    );
}

/// The percentiles follow the nearest-rank rule, and the artifact writer
/// puts its file where it says it does.
#[test]
fn the_percentiles_and_the_artifact_are_readable() {
    let times = Percentiles::of(&[5, 1, 4, 2, 3]);
    assert_eq!((times.p50, times.p95, times.p99, times.max), (3, 5, 5, 5));
    assert_eq!(
        times.json(),
        "{\"p50_ns\": 3, \"p95_ns\": 5, \"p99_ns\": 5, \"max_ns\": 5}"
    );
    assert_eq!(times.phrase(), "p50 3 ns, p95 5 ns, p99 5 ns, max 5 ns");
    let empty = Percentiles::of(&[]);
    assert_eq!((empty.p50, empty.max), (0, 0));

    let body = artifact_body(
        10,
        &Shares {
            blanks: 1,
            wrong: 1,
            respelled: 8,
        },
        &Tally::default(),
        &times,
    );
    let path = write_artifact("benchmark-a-grade-probe.json", &body);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), body);
    assert!(body.contains("\"benchmark\": \"A-grade\""));
    assert!(body.contains(&format!("\"profile\": {:?}", profile())));
}
