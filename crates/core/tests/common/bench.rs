//! Helpers benchmark A shares across its three binaries: the template
//! fixtures, the pinned sequence, and its digest. The percentiles, the time
//! budget, and the artifact file come from `cadus_testkit::bench`.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use cadus_core::learner::problem_text_hash;
use cadus_core::template::{Compiled, TemplateDoc, from_body, rng_from_seed};

/// The count of committed fixture templates (spec section 10.1).
pub const TEMPLATE_COUNT: usize = 20;

/// The count of measured instantiations (spec section 10.1).
pub const ITERATIONS: usize = 2_000;

/// The seed of every draw of the run. The benchmark is a function of this
/// number and the committed fixtures alone.
pub const BENCH_SEED: u64 = 20_260_827;

/// The last digest of the ring after the measured loop.
///
/// The literal is the instance hash of the 2,000th draw alone. It pins the last
/// iteration and nothing else, and iteration 1,999 reads fixture 20, because
/// `1999 % TEMPLATE_COUNT == 19`. [`SEQUENCE_DIGEST`] is the guard over all
/// 2,000 iterations and all 20 fixtures (M4 review 2, finding 6).
pub const RING_TAIL_AFTER_THE_RUN: &str = "c027d35bab27";

/// The count of iterations whose digest the anti-repeat view already held.
///
/// The 20 fixtures include small spaces (12 distinct instances for the perfect
/// squares and for the square roots), so a repeat inside the 20-digest ring and
/// the 12-digest task memory is expected, and the count is a fixed number of
/// this fixed sequence.
pub const BLOCKED_IN_THE_RUN: usize = 47;

/// One pinned iteration of the measured sequence.
///
/// The four fields are the whole record of one draw: the fixture the loop read,
/// the statement the learner reads, the answer the pool row carries, and the
/// digest the D5 anti-repeat view holds.
pub struct PinnedInstance {
    /// The index of the iteration inside the 2,000-iteration loop.
    pub iteration: usize,
    /// The file name of the fixture the iteration draws from.
    pub fixture: &'static str,
    /// The rendered statement.
    pub text: &'static str,
    /// The expected answer, inside the M2 grammar.
    pub answer: &'static str,
    /// `problem_text_hash` of `text`.
    pub hash: &'static str,
}

/// Three pinned iterations: the first draw of fixture 1, of fixture 10, and of
/// fixture 20 (M4 review 2, ruling on findings 6, 7 and 11).
///
/// `index % TEMPLATE_COUNT` picks the fixture, so iteration 0 draws the first
/// fixture in file-name order, iteration 9 draws the tenth, and iteration 19
/// draws the twentieth. These three literals are the readable half of the
/// sequence guard: a reviewer reads the fixture, the statement, and the answer
/// and compares them by eye. [`SEQUENCE_DIGEST`] is the half that covers the
/// other 17 fixtures.
///
/// [`the_measured_sequence_is_pinned`] prints the current values before it
/// asserts, so a deliberate change re-pins these literals from the printed
/// lines.
pub const PINNED_INSTANCES: [PinnedInstance; 3] = [
    PinnedInstance {
        iteration: 0,
        fixture: "01-perfect-squares.json",
        text: "Compute $3^{2}$.",
        answer: "9",
        hash: "e61280d48db9",
    },
    PinnedInstance {
        iteration: 9,
        fixture: "10-distribute-over-a-sum.json",
        text: "Expand $5(x + 3)$.",
        answer: "15 + 5*x",
        hash: "54a94abb69bd",
    },
    PinnedInstance {
        iteration: 19,
        fixture: "20-two-digit-difference.json",
        text: "Compute $48 - 34$.",
        answer: "14",
        hash: "1287954c19c6",
    },
];

/// The digest of the whole measured sequence.
///
/// [`sequence_digest`] folds the fixture name, the rendered text, the answer,
/// and the instance hash of all 2,000 iterations into one `problem_text_hash`.
/// The literal therefore moves on a change to the draw order, to the renderer,
/// to the evaluator, or to the digest of ANY of the 20 fixtures.
///
/// This literal is the answer to M4 review 2 finding 6. Before it, the file
/// pinned the last draw and a count of set hits alone, and the FIXM4a bracket
/// rule changed 183 of the 2,000 statements (fixtures 07 and 08) while both
/// literals held byte for byte.
pub const SEQUENCE_DIGEST: &str = "c767c554c277";

/// The directory of the committed template fixtures.
pub fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/templates")
}

/// Read the committed templates in file-name order.
pub fn fixtures() -> Vec<(String, TemplateDoc)> {
    let dir = fixture_dir();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let body = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
            let doc = from_body(&body).unwrap_or_else(|err| panic!("{name} does not read: {err}"));
            (name, doc)
        })
        .collect()
}

/// Compile every fixture, in file-name order.
#[must_use]
pub fn compile_all(fixtures: &[(String, TemplateDoc)]) -> Vec<Compiled<'_>> {
    fixtures
        .iter()
        .map(|(name, doc)| {
            Compiled::new(doc).unwrap_or_else(|err| panic!("{name} does not compile: {err}"))
        })
        .collect()
}

/// One replayed iteration of the measured sequence.
pub struct Replayed {
    /// The file name of the fixture the iteration drew from.
    pub fixture: String,
    /// The rendered statement.
    pub text: String,
    /// The expected answer.
    pub answer: String,
    /// `problem_text_hash` of `text`.
    pub hash: String,
}

/// Replay the measured sequence outside the allocation counter.
///
/// The replay draws the same 2,000 instances as
/// [`benchmark_a_instantiation_holds_the_l1_segment`]: the same seed, the same
/// fixtures, the same order, the same production `draw`. The measured loop keeps
/// no text and no digest of its own, because a kept `String` is an allocation
/// [`ALLOCATION_BOUND`] pays for; this function keeps all four fields of every
/// iteration and costs the measured numbers nothing.
///
/// The ring and the task memory of the measured loop read the digests and never
/// feed the draw, so the two loops walk one sequence. The benchmark asserts that
/// tie: its ring tail equals the last digest of this replay.
pub fn replay() -> Vec<Replayed> {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), TEMPLATE_COUNT);
    let compiled = compile_all(&fixtures);

    let mut rng = rng_from_seed(BENCH_SEED);
    let mut run: Vec<Replayed> = Vec::with_capacity(ITERATIONS);
    for index in 0..ITERATIONS {
        let slot = index % TEMPLATE_COUNT;
        let instance = match compiled[slot].draw(&mut rng) {
            Ok(instance) => instance,
            Err(err) => panic!("iteration {index} did not instantiate: {err}"),
        };
        run.push(Replayed {
            fixture: fixtures[slot].0.clone(),
            text: instance.text,
            answer: instance.answer,
            hash: instance.instance_hash,
        });
    }
    run
}

/// The digest of a whole replayed run.
///
/// The preimage holds one line per iteration. One line is the fixture name, the
/// rendered text, the answer, and the instance hash, in that order, separated by
/// the unit separator `U+001F`. No field of the four carries that character or a
/// newline, so one preimage reads back as one sequence.
///
/// The function hashes the preimage with `problem_text_hash`, the production
/// digest of a problem text, so the fold needs no second hash definition
/// (trap T17).
pub fn sequence_digest(run: &[Replayed]) -> String {
    let mut preimage = String::new();
    for row in run {
        preimage.push_str(&row.fixture);
        preimage.push('\u{1f}');
        preimage.push_str(&row.text);
        preimage.push('\u{1f}');
        preimage.push_str(&row.answer);
        preimage.push('\u{1f}');
        preimage.push_str(&row.hash);
        preimage.push('\n');
    }
    problem_text_hash(&preimage)
}
