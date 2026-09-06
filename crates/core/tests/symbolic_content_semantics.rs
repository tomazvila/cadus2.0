//! Semantic regression guard for the content-symbolic-a lane's held-out
//! exemplars: an independent review found rows that were *decidable*
//! (production-correct) but violated their own knowledge point's literal
//! authored requirement (term count, integer/feasible solution, matching
//! sign/distribution shape, self-contained wording, or an explicit
//! verification step the KP name promises). Passing `crates/core/src/bin/
//! lint_curriculum.rs` and `ReadinessIndex` alone cannot catch these; each
//! row below re-checks the SPECIFIC authored constraint against the REAL
//! curriculum text, so a future edit that reintroduces one of these defects
//! fails here even though it would still serve, render, and grade.
//!
//! `Check.verify` takes the problem, the answer, AND the solution_sketch, so
//! a row whose requirement lives in the sketch (e.g. "the sketch must name
//! both branches") is checked directly here instead of through a
//! `verify: |_, _| true` placeholder plus a second, disconnected test. See
//! `symbolic_content_contracts.rs` for the companion evidence matrix that
//! calls the REAL production checker (not a text heuristic) on the rows
//! this correction pass closed.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::path::Path;

use cadus_core::curriculum::{Curriculum, load_curriculum};

fn curriculum() -> Curriculum {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
    load_curriculum(&root).unwrap().0
}

fn exemplar<'a>(
    curriculum: &'a Curriculum,
    topic_id: &str,
    kp_id: &str,
    index: usize,
) -> (&'a str, &'a str, &'a str) {
    let topic = curriculum
        .topics()
        .iter()
        .find(|topic| topic.id.as_str() == topic_id)
        .unwrap_or_else(|| panic!("topic {topic_id} not found"));
    let kp = topic
        .knowledge_points
        .iter()
        .find(|kp| kp.id.as_str() == kp_id)
        .unwrap_or_else(|| panic!("{topic_id}/{kp_id} not found"));
    let item = kp
        .exemplars
        .get(index)
        .unwrap_or_else(|| panic!("{topic_id}/{kp_id}[{index}] not found"));
    (
        item.problem.as_str(),
        item.answer.as_str(),
        item.solution_sketch.as_deref().unwrap_or(""),
    )
}

/// A semantic check tied to one KP's own authored `constraints` line, not to
/// answer-grammar decidability.
struct Check {
    topic_id: &'static str,
    kp_id: &'static str,
    index: usize,
    /// The literal authored requirement this row exists to protect.
    requirement: &'static str,
    verify: fn(problem: &str, answer: &str, sketch: &str) -> bool,
}

const CHECKS: &[Check] = &[
    Check {
        topic_id: "evaluating-expressions",
        kp_id: "kp2",
        index: 3,
        requirement: "watch sign of squared negatives: the held-out item must square a negative input",
        verify: |problem, _, _| problem.contains('^') && problem.contains("x = -"),
    },
    Check {
        topic_id: "evaluating-expressions",
        kp_id: "kp3",
        index: 2,
        requirement: "three terms",
        verify: |problem, _, _| {
            // The expression sits between "$" delimiters right after "Evaluate ".
            let expr = problem.split('$').nth(1).unwrap_or("");
            expr.matches(['+', '-']).count() >= 2
        },
    },
    Check {
        topic_id: "inequality-word-problems",
        kp_id: "kp2",
        index: 2,
        requirement: "the required average is achievable on a 0-10 scale (feasible threshold)",
        verify: |_, answer, _| answer == "at least 10",
    },
    Check {
        topic_id: "absolute-value-equations",
        kp_id: "kp1",
        index: 2,
        requirement: "split into two linear equations; integer solutions",
        verify: |_, answer, _| !answer.contains('/'),
    },
    Check {
        topic_id: "distribute-then-solve",
        kp_id: "kp2",
        index: 3,
        requirement: "negative multipliers and extra constant terms: the item must carry both",
        verify: |problem, _, _| {
            problem.contains("-2(") || problem.contains("-3(") || problem.contains("-4(")
        },
    },
    Check {
        topic_id: "absolute-value-equations",
        kp_id: "kp2",
        index: 3,
        requirement: "the sketch names both split branches and their final values",
        verify: |_, answer, sketch| {
            answer == "x = 7 or x = -7" && sketch.contains("x = 7") && sketch.contains("x = -7")
        },
    },
    Check {
        topic_id: "graphing-proportional-relationships",
        kp_id: "kp1",
        index: 3,
        requirement: "always include the origin plus one more LATTICE point (both coordinates integers)",
        verify: |_, answer, _| !answer.contains("k)"),
    },
    Check {
        topic_id: "graphing-from-a-table",
        kp_id: "kp2",
        index: 3,
        requirement: "the item must show an actual table of rows, not a bare equation",
        verify: |problem, _, _| problem.contains("The table gives"),
    },
    Check {
        topic_id: "linear-word-problems",
        kp_id: "kp1",
        index: 3,
        requirement: "self-contained: exemplars are served independently, so the model must be restated",
        verify: |problem, _, _| {
            problem.contains("y = 3p + 5") || !problem.starts_with("Using that model")
        },
    },
    Check {
        topic_id: "slope-as-rate-of-change",
        kp_id: "kp1",
        index: 2,
        requirement: "the prompt's requested representation must match what the bare-number contract can grade",
        verify: |problem, _, _| problem.contains("as a signed number"),
    },
    Check {
        topic_id: "substitution-with-isolated-variable",
        kp_id: "kp3",
        index: 3,
        requirement: "back-substitute and check: the sketch must verify in an original equation",
        verify: |problem, _, _| problem.contains("check"),
    },
    Check {
        topic_id: "elimination-with-addition",
        kp_id: "kp3",
        index: 3,
        requirement: "verify in both equations",
        verify: |problem, _, _| problem.contains("check"),
    },
    Check {
        topic_id: "systems-of-linear-inequalities",
        kp_id: "kp2",
        index: 3,
        requirement: "one strict and one inclusive inequality per problem (not a generic definition)",
        verify: |problem, _, _| problem.contains('$') && problem.contains("system"),
    },
    Check {
        topic_id: "systems-money-problems",
        kp_id: "kp3",
        index: 3,
        requirement: "verify counts are nonnegative integers and totals match, for the held-out item too",
        verify: |_, answer, sketch| {
            answer == "6" && sketch.contains("nonnegative") && sketch.contains("check")
        },
    },
    // Round 2 (independent re-review) corrections.
    Check {
        topic_id: "basic-absolute-value-inequalities",
        kp_id: "kp1",
        index: 0,
        requirement: "|x| < a is one bounded interval, not two rays joined by \"and\"",
        verify: |_, answer, sketch| {
            answer == "-5 < x < 5"
                && sketch.contains("bounded interval")
                && !sketch.contains(" and ")
        },
    },
    Check {
        topic_id: "basic-absolute-value-inequalities",
        kp_id: "kp1",
        index: 2,
        requirement: "|x| < a is one bounded interval, not two rays joined by \"and\"",
        verify: |_, answer, sketch| {
            answer == "-8 < x < 8"
                && sketch.contains("bounded interval")
                && !sketch.contains(" and ")
        },
    },
    Check {
        topic_id: "systems-mixture-problems",
        kp_id: "kp1",
        index: 0,
        requirement: "set up BOTH the amount and the concentration equation, not one arithmetic step",
        verify: |problem, answer, _| {
            problem.contains("give both equations") && answer.contains("and")
        },
    },
    Check {
        topic_id: "systems-mixture-problems",
        kp_id: "kp1",
        index: 2,
        requirement: "set up BOTH the amount and the concentration equation, not one arithmetic step",
        verify: |problem, answer, _| {
            problem.contains("give both equations") && answer.contains("and")
        },
    },
    Check {
        topic_id: "systems-word-problems",
        kp_id: "kp3",
        index: 1,
        requirement: "one SCALED relationship plus one sum (not an additive age offset)",
        verify: |problem, _, _| problem.contains("times as old"),
    },
    Check {
        topic_id: "systems-word-problems",
        kp_id: "kp3",
        index: 3,
        requirement: "one SCALED relationship plus one sum (not an additive age offset)",
        verify: |problem, _, _| problem.contains("times as old"),
    },
    Check {
        topic_id: "systems-special-cases",
        kp_id: "kp3",
        index: 3,
        requirement: "the four outcomes of this KP must include a no-solution case",
        verify: |_, answer, _| answer == "no solution",
    },
];

#[test]
fn every_corrected_row_still_meets_its_own_kps_literal_requirement() {
    let curriculum = curriculum();
    let mut failures = Vec::new();
    for check in CHECKS {
        let (problem, answer, sketch) =
            exemplar(&curriculum, check.topic_id, check.kp_id, check.index);
        if !(check.verify)(problem, answer, sketch) {
            failures.push(format!(
                "{}/{}[{}]: fails \"{}\" (problem: {problem:?}, answer: {answer:?})",
                check.topic_id, check.kp_id, check.index, check.requirement
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_sketches_carrying_verification_evidence_still_carry_it() {
    // The four rows whose whole requirement lives in the sketch (both
    // absolute-value-inequalities rows and the absolute-value-equations
    // held-out) are checked directly by `CHECKS` above now that `Check.verify`
    // takes the sketch too; this table covers only the rows `CHECKS` does not
    // reach: systems-money-problems/kp3's PRACTICE row (index 2, not the
    // held-out index 3 `CHECKS` covers) and both systems-mixture-problems
    // rows, whose `CHECKS` entry checks the problem/answer text only.
    let curriculum = curriculum();
    for (topic_id, kp_id, index, must_contain) in [
        ("systems-money-problems", "kp3", 2, "nonnegative"),
        ("systems-mixture-problems", "kp1", 0, "Amounts sum"),
        ("systems-mixture-problems", "kp1", 2, "Amounts sum"),
    ] {
        let topic = curriculum
            .topics()
            .iter()
            .find(|topic| topic.id.as_str() == topic_id)
            .unwrap();
        let kp = topic
            .knowledge_points
            .iter()
            .find(|kp| kp.id.as_str() == kp_id)
            .unwrap();
        let sketch = kp.exemplars[index].solution_sketch.as_deref().unwrap_or("");
        assert!(
            sketch.contains(must_contain),
            "{topic_id}/{kp_id}[{index}] sketch {sketch:?} is missing {must_contain:?}"
        );
    }
}

fn constant_first_differences(rows: &[(i32, i32)]) -> bool {
    let deltas: Vec<_> = rows
        .windows(2)
        .map(|pair| (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1))
        .collect();
    deltas.windows(2).all(|pair| pair[0] == pair[1])
}

#[test]
fn graph_table_rows_have_constant_first_differences() {
    assert!(constant_first_differences(&[(1, -2), (2, -1), (3, 0)]));
    assert!(!constant_first_differences(&[(0, -3), (1, -2), (3, 0)]));
    let curriculum = curriculum();
    let (problem, answer, sketch) = exemplar(&curriculum, "graphing-from-a-table", "kp2", 1);
    assert!(problem.contains("$(1, -2)$, $(2, -1)$, $(3, 0)$"));
    assert_eq!(answer, "(3, 0)");
    assert!(sketch.contains("$\\Delta x = 1$") && sketch.contains("$\\Delta y = 1$"));
}

#[test]
fn substitution_check_and_elimination_opposites_are_explicit() {
    let curriculum = curriculum();
    let (problem, _, sketch) =
        exemplar(&curriculum, "substitution-with-isolated-variable", "kp3", 1);
    assert!(problem.contains("check your answer"));
    assert!(sketch.contains("check $3 + 12 = 15$"));
    for index in 0..4 {
        let (_, _, sketch) = exemplar(&curriculum, "systems-elimination", "kp1", index);
        assert!(sketch.contains("by $-2$") || sketch.contains("by $-3$"));
        assert!(sketch.contains("add"));
        assert!(!sketch.contains("subtract"));
    }
}
