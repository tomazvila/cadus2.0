//! Part 4 of the `template` tests. The header of `template_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::template::*;

/// M4 review 1, finding 17: the answer writer brackets a power under a power.
///
/// `**` groups to the right, so `x**2**3` reads as `x**(2**3)`. The M2 parser
/// refuses that string as a tower of powers, so the unbracketed form left the
/// decidable grammar and every instance of such a template was refused (V2).
#[test]
fn the_answer_writer_brackets_a_power_that_is_the_base_of_a_power() {
    for (source, wanted) in [
        ("(x**2)**3", "(x**2)**3"),
        ("2*(x**2)**3", "2*(x**2)**3"),
        ("((x + 1)**2)**2", "((1 + x)**2)**2"),
        ("(-x)**2", "(-x)**2"),
        ("(x/2)**3", "(x/2)**3"),
        ("(x*y)**2", "(x*y)**2"),
        ("sqrt(x)**2", "sqrt(x)**2"),
        ("x**2*y**3", "x**2*y**3"),
    ] {
        let ast = cadus_core::template::parse_answer_expr(source)
            .unwrap_or_else(|error| panic!("{source} parses: {error}"));
        let value = cadus_core::template::evaluate(&ast, &Bindings::new())
            .unwrap_or_else(|error| panic!("{source} evaluates: {error}"));
        let written = cadus_core::template::write(&value)
            .unwrap_or_else(|error| panic!("{source} writes: {error}"));
        assert_eq!(written, wanted, "{source}");
        cadus_core::answer::canonical_form(&written)
            .unwrap_or_else(|reason| panic!("{written} does not canonicalize: {reason}"));
    }
}

/// The candidate stream walks past a draw that spends its budget.
///
/// The constraint holds for about one tuple in 2,048, so about six draws in ten
/// spend the 1,000-draw budget. The stream that stopped at the first such draw
/// was empty three runs in five, and the refill then had no instance to insert
/// for a template the gate accepts (M4 review 1, findings 7 and 12).
#[test]
fn a_sparse_candidate_stream_skips_the_draws_that_spend_their_budget() {
    let doc = doc_from(
        r#"{"v": 1, "topic_id": "sparse", "answer_kind": "numeric",
            "statement": "Compute ${a} + {b}$.",
            "params": {"a": {"kind": "int", "low": 4096, "high": 8192},
                       "b": {"kind": "int", "low": 1, "high": 10}},
            "constraints": [{"op": "eq", "left": {"mod": ["a", {"lit": 4096}]},
                             "right": {"lit": 0}}],
            "answer_expr": "a + b", "hints": ["Which column do you add first?"],
            "samples": [{"params": {"a": 4096, "b": 1}, "expected": "4097"}]}"#,
    );
    let compiled = Compiled::new(&doc).expect("the template compiles");
    // The counts are the ones the three seeds produce, and a seeded draw is the
    // same on every machine.
    for (seed, count) in [(1_u64, 11_usize), (5, 10), (9, 6)] {
        let mut rng = rng_from_seed(seed);
        let stream = compiled.candidates(&mut rng).expect("the stream builds");
        assert_eq!(stream.len(), count, "seed {seed}");
        for bindings in &stream {
            assert!(
                cadus_core::template::all_hold(&doc.constraints, bindings).expect("it decides"),
                "the stream never yields a tuple the constraints refuse"
            );
        }
    }
}
