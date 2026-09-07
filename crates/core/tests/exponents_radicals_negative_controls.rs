//! Adversarial negative controls for `06-exponents-radicals.yaml`: domain,
//! sign, and simplification-invariant checks that a wrong or malformed
//! answer must actually fail, not merely that a right one passes.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::path::Path;

use cadus_core::answer::{AnswerContract, canonical_form, same_answer};
use cadus_core::curriculum::load_curriculum;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn clean_curriculum() -> cadus_core::curriculum::Curriculum {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    curriculum
}

/// An even-index root of a negative number is not a real number, so the
/// grammar must never let it collapse to a signed real value: this is the
/// domain fact `cube-roots/kp2` relies on being false for square roots and
/// true for its own odd-index radicands.
#[test]
fn an_even_root_of_a_negative_number_never_matches_a_signed_real() {
    let even_root = canonical_form("sqrt(-9)").unwrap();
    assert!(!same_answer(&even_root, &canonical_form("3").unwrap()));
    assert!(!same_answer(&even_root, &canonical_form("-3").unwrap()));
}

/// `cube-roots/kp2` grades its odd-root-of-a-negative-radicand exemplars
/// (`-8`, `-125`, `-64`, `-27`) as the plain signed integer, so the sign the
/// authored answer carries is exactly the fact under test: the grammar must
/// tell `-4` apart from `4`. (`foundations_compute.py`'s
/// `RadicalEvaluatorSafetyTest` independently proves the arithmetic itself —
/// `(-4)**3 == -64` — since the answer grammar does not evaluate a negative
/// base raised to a fractional power at all, matching its treatment of
/// `sqrt(-4)` above.)
#[test]
fn cube_roots_of_negative_radicands_keep_a_distinguishable_sign() {
    assert!(!same_answer(
        &canonical_form("-4").unwrap(),
        &canonical_form("4").unwrap()
    ));
}

/// A sign-flipped, off-by-one, or swapped-label adversarial answer must be
/// rejected for a representative exemplar of every family in this unit —
/// proof the checker actually discriminates rather than accepting anything.
#[test]
fn a_representative_exemplar_of_every_family_rejects_a_wrong_neighbor() {
    let cases: &[(&str, &str, &str)] = &[
        ("x^7", "x^8", "off-by-one exponent"),
        ("a^7", "a^6", "off-by-one exponent"),
        ("y^4", "y^5", "off-by-one exponent"),
        ("1/y^4", "y^4", "missing reciprocal"),
        ("1", "0", "wrong zero-power value"),
        ("1/9", "9", "missing reciprocal"),
        ("13", "-13", "wrong sign on a real root"),
        ("-4", "4", "wrong sign on an odd real root"),
        ("3*sqrt(3)", "9*sqrt(3)", "wrong extracted coefficient"),
        ("x^3*sqrt(x)", "x^4*sqrt(x)", "wrong leftover power"),
        ("8*sqrt(2)", "8*sqrt(3)", "wrong radicand"),
        (
            "sqrt(3)",
            "3",
            "left a radical unevaluated versus rationalized",
        ),
        ("4", "5", "arithmetic slip"),
        ("x^(3/5)", "x^(5/3)", "swapped numerator and denominator"),
        ("x = 36", "x = 6", "forgot to square the isolated root"),
    ];
    for (authored, adversarial, why) in cases {
        let expected = canonical_form(authored)
            .unwrap_or_else(|_| panic!("{authored} must itself be decidable"));
        let Ok(wrong) = canonical_form(adversarial) else {
            continue; // an undecidable adversarial neighbor is refused for free
        };
        assert!(
            !same_answer(&expected, &wrong),
            "{why}: {authored:?} must not accept {adversarial:?}"
        );
    }
}

/// The `label`-contract families (`pythagorean-converse`'s yes/no and
/// acute/right/obtuse answers, `radical-equations-basic/kp3`'s "no
/// solution") grade by exact alias, not free-form text: a swapped or
/// numeric-looking neighbor must be refused under the very options each
/// exemplar authors.
#[test]
fn label_contract_families_reject_a_swapped_or_numeric_neighbor() {
    let right_triangle = AnswerContract::Label {
        options: vec![vec!["yes".to_owned()], vec!["no".to_owned()]],
    };
    let yes = right_triangle.validate_expected("yes").unwrap();
    let no = right_triangle.validate_expected("no").unwrap();
    assert!(!same_answer(&yes, &no));

    let classification = AnswerContract::Label {
        options: vec![
            vec!["acute".to_owned()],
            vec!["right".to_owned()],
            vec!["obtuse".to_owned()],
        ],
    };
    let obtuse = classification.validate_expected("obtuse").unwrap();
    let acute = classification.validate_expected("acute").unwrap();
    assert!(!same_answer(&obtuse, &acute));

    let no_solution = AnswerContract::Label {
        options: vec![vec!["no solution".to_owned()]],
    };
    assert!(no_solution.validate_expected("no solution").is_ok());
    assert!(
        no_solution.validate_expected("0").is_err(),
        "a domain violation must not validate as the numeric value 0"
    );
}

/// The topics whose own `constraints` line requires a fully reduced
/// radical (`estimating-square-roots` deliberately compares an
/// UNreduced `sqrt(n)` against integers, so it is excluded on purpose).
const SIMPLIFY_RADICAL_TOPICS: &[&str] = &[
    "simplifying-radicals",
    "simplifying-radicals-variables",
    "adding-subtracting-radicals",
    "radical-operations",
    "dividing-radicals",
    "rationalizing-denominators",
    "pythagorean-theorem",
];

/// Every "a*sqrt(b)" or bare "sqrt(b)" answer authored in this unit must
/// leave a squarefree radicand: an unreduced `sqrt(8)` where `2*sqrt(2)` was
/// intended is a real authoring bug this test independently catches.
#[test]
fn every_simplified_radical_answer_has_a_squarefree_radicand() {
    let curriculum = clean_curriculum();
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for topic in curriculum.topics() {
        if !SIMPLIFY_RADICAL_TOPICS.contains(&topic.id.as_str()) {
            continue;
        }
        for kp in &topic.knowledge_points {
            for exemplar in &kp.exemplars {
                for radicand in radicands_of(&exemplar.answer) {
                    checked += 1;
                    if !is_squarefree(radicand.abs()) {
                        failures.push(format!(
                            "{}/{}: {:?} has a non-squarefree radicand {radicand}",
                            topic.id.as_str(),
                            kp.id.as_str(),
                            exemplar.answer
                        ));
                    }
                }
            }
        }
    }
    assert!(
        checked >= 30,
        "expected the radical-heavy topics to contribute cases: {checked}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every `N x 10^E`-shaped answer in this unit keeps its coefficient in the
/// authored range `1 <= |N| < 10`, the scientific-notation domain rule
/// `scientific-notation-conversion/kp1`'s own constraints line states.
#[test]
fn every_scientific_notation_answer_keeps_its_coefficient_normalized() {
    let curriculum = clean_curriculum();
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for topic in curriculum.topics() {
        if !topic.id.as_str().starts_with("scientific-notation") {
            continue;
        }
        for kp in &topic.knowledge_points {
            for exemplar in &kp.exemplars {
                let Some(mantissa) = mantissa_of(&exemplar.answer) else {
                    continue;
                };
                checked += 1;
                if !(1.0..10.0).contains(&mantissa.abs()) {
                    failures.push(format!("{:?} -> mantissa {mantissa}", exemplar.answer));
                }
            }
        }
    }
    assert!(
        checked >= 10,
        "expected several scientific-notation answers: {checked}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every `radical-equations-basic/kp3` "no solution" exemplar names a
/// negative right-hand side: the domain fact that makes it genuinely
/// unsolvable over the reals, not an arbitrary label.
#[test]
fn no_solution_radical_equations_name_a_negative_right_hand_side() {
    let curriculum = clean_curriculum();
    let mut checked = 0usize;
    for topic in curriculum.topics() {
        if topic.id.as_str() != "radical-equations-basic" {
            continue;
        }
        for kp in &topic.knowledge_points {
            for exemplar in &kp.exemplars {
                if exemplar.answer != "no solution" {
                    continue;
                }
                checked += 1;
                let after_equals = exemplar.problem.rsplit('=').next().unwrap();
                let rhs: String = after_equals
                    .chars()
                    .filter(|c| *c != '$' && *c != '.')
                    .collect();
                let rhs = rhs.trim();
                assert!(
                    rhs.starts_with('-'),
                    "{:?} should isolate sqrt(...) equal to a negative value",
                    exemplar.problem
                );
            }
        }
    }
    assert_eq!(
        checked, 2,
        "expected exactly the two authored no-solution exemplars"
    );
}

fn is_squarefree(value: i64) -> bool {
    let mut factor = 2i64;
    while factor * factor <= value {
        if value % (factor * factor) == 0 {
            return false;
        }
        factor += 1;
    }
    true
}

/// Every radicand appearing in a `sqrt(n)` call of an answer string.
fn radicands_of(answer: &str) -> Vec<i64> {
    let mut out = Vec::new();
    let mut rest = answer;
    while let Some(at) = rest.find("sqrt(") {
        rest = &rest[at + "sqrt(".len()..];
        let Some(close) = rest.find(')') else { break };
        let inner = &rest[..close];
        if let Ok(value) = inner.parse::<i64>() {
            out.push(value);
        }
        rest = &rest[close..];
    }
    out
}

/// The leading mantissa of an `N x 10^E` or `N \times 10^{E}` answer string.
fn mantissa_of(answer: &str) -> Option<f64> {
    if !answer.contains("10^") {
        return None; // not an "N x 10^E" shaped answer at all
    }
    let head = answer.split(['x']).next()?.trim();
    head.parse::<f64>().ok()
}
