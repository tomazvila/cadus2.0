//! Regression guard for three restoration classes: lost contextual practice,
//! tautological multiplication decomposition, removed finite_objective_domain.
//! Loads the public curriculum tree (common::paths::tree) in each test.

#![allow(clippy::unwrap_used)]

mod common;

use cadus_core::curriculum::{Curriculum, FiniteCaseRole, KnowledgePoint};

// -- Helpers --

fn kp_problems(curriculum: &Curriculum, topic: &str, kp: &str) -> Vec<String> {
    curriculum
        .knowledge_points(curriculum.idx_of(topic).unwrap())
        .iter()
        .find(|p| p.id.as_str() == kp)
        .unwrap()
        .exemplars
        .iter()
        .map(|e| e.problem.clone())
        .collect()
}

fn kp_sketches(curriculum: &Curriculum, topic: &str, kp: &str) -> Vec<String> {
    curriculum
        .knowledge_points(curriculum.idx_of(topic).unwrap())
        .iter()
        .find(|p| p.id.as_str() == kp)
        .unwrap()
        .exemplars
        .iter()
        .filter_map(|e| e.solution_sketch.clone())
        .collect()
}

fn is_tautology(sketch: &str) -> bool {
    sketch.split(';').any(|part| {
        let trimmed = part.trim().trim_start_matches('$').trim_end_matches('$');
        if let Some(eq_pos) = trimmed.find(" = ") {
            let lhs = trimmed[..eq_pos]
                .trim()
                .trim_start_matches('$')
                .trim_end_matches('$')
                .trim();
            let rhs = trimmed[eq_pos + 3..]
                .trim()
                .trim_start_matches('$')
                .trim_end_matches('$')
                .trim();
            lhs == rhs && !lhs.contains("\\times") && !lhs.contains('+') && !lhs.contains('-')
        } else {
            false
        }
    })
}

fn assert_context_and_variety(problems: &[String], word_prefix: &str) {
    assert!(
        problems.iter().any(|p| p.starts_with(word_prefix)),
        "expected word problem starting with {word_prefix:?}, got: {problems:?}"
    );
    assert!(
        problems.iter().any(|p| p.contains("Find")),
        "expected Find-variant, got: {problems:?}"
    );
}

fn assert_finite_domain(kp: &KnowledgePoint, case_ids: &[&str], expo_roles: &[FiniteCaseRole]) {
    let d = kp.finite_objective_domain.as_ref().unwrap();
    assert_eq!(d.schema_version, 1);
    assert!(!d.review_ref.is_empty());
    let ids: Vec<&str> = d.cases.iter().map(|c| c.id.as_str()).collect();
    for want in case_ids {
        assert!(ids.contains(want), "missing case {want} in {ids:?}");
    }
    for role in expo_roles {
        assert!(
            d.cases.iter().any(|c| c.role == *role),
            "missing role {role:?}"
        );
    }
    kp.validate_finite_objective_domain().unwrap();
}

// -- Test 1: contextual practice + Find/Evaluate variety across arithmetic core --

#[test]
fn contextual_mixed_practice_across_arithmetic_core() {
    let c = common::paths::tree();

    // single-digit-addition kp1/kp2
    let p = kp_problems(&c, "single-digit-addition", "kp1");
    assert_context_and_variety(&p, "A box has");
    let p = kp_problems(&c, "single-digit-addition", "kp2");
    assert_context_and_variety(&p, "Liam has");

    // subtraction-facts kp1/kp2
    let p = kp_problems(&c, "subtraction-facts", "kp1");
    assert!(p.iter().any(|p| p.starts_with("A basket has")));
    assert!(p.iter().any(|p| p.starts_with("Evaluate")));
    assert!(p.iter().any(|p| p.contains("Find")));
    let p = kp_problems(&c, "subtraction-facts", "kp2");
    assert_context_and_variety(&p, "A parking lot");

    // multiplication-tables kp1/kp2
    let p = kp_problems(&c, "multiplication-tables", "kp1");
    assert_context_and_variety(&p, "A classroom has");
    let p = kp_problems(&c, "multiplication-tables", "kp2");
    assert_context_and_variety(&p, "A theater has");

    // division-facts kp1
    let p = kp_problems(&c, "division-facts", "kp1");
    assert!(
        p.iter()
            .any(|p| p.contains("cookies") && p.contains("shared"))
    );
    assert!(p.iter().any(|p| p.contains("Find")));

    // perfect-squares kp1
    let p = kp_problems(&c, "perfect-squares", "kp1");
    assert_context_and_variety(&p, "A square garden");
}

// -- Test 2: pedagogical decomposition + no tautologies in multiplication kp1 --

#[test]
fn multiplication_sketches_are_pedagogical_not_tautological() {
    let c = common::paths::tree();
    let sketches = kp_sketches(&c, "multiplication-tables", "kp1");

    assert!(
        sketches
            .iter()
            .any(|s| s.contains("6 \\times 7 = 6 \\times 5 + 6 \\times 2"))
    );
    assert!(
        sketches
            .iter()
            .any(|s| s.contains("8 \\times 4 = 8 \\times 2 \\times 2"))
    );
    assert!(
        sketches
            .iter()
            .any(|s| s.contains("9 \\times 6 = 10 \\times 6 - 6"))
    );
    assert!(sketches.iter().any(|s| s.contains("appends one zero")));

    for s in &sketches {
        assert!(!is_tautology(s), "tautology found: {s:?}");
    }
}

// -- Test 3: finite_objective_domain for both logarithm KPs --

#[test]
fn both_log_kps_have_valid_finite_objective_domain() {
    let c = common::paths::tree();
    let kps = c.knowledge_points(c.idx_of("common-natural-logarithms").unwrap());
    assert_eq!(
        kps.iter()
            .filter(|kp| kp.finite_objective_domain.is_some())
            .count(),
        2
    );
    let kp1 = kps.iter().find(|kp| kp.id.as_str() == "kp1").unwrap();
    let kp2 = kps.iter().find(|kp| kp.id.as_str() == "kp2").unwrap();

    assert_finite_domain(
        kp1,
        &["common-neg4", "common-0", "common-6"],
        &[
            FiniteCaseRole::PracticeFresh,
            FiniteCaseRole::ReservedAssessment,
        ],
    );
    assert_finite_domain(
        kp2,
        &["natural-0", "natural-1", "natural-5"],
        &[FiniteCaseRole::TeachOnly, FiniteCaseRole::TaughtRehearsal],
    );
}
