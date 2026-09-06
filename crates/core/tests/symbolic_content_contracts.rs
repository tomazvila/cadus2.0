//! Independent grading-evidence matrix for the KPs this correction pass
//! closed: it calls the REAL production checker (`check_contract` for a
//! `label`/`inequality_union` contract, `check` for the base grammar) on
//! each row's own authored answer AND on an adversarial near-miss the row's
//! own requirement must reject. A row that only restated its generated
//! value, or that always returns `true`, would pass every other test in
//! this lane and still grade wrong; this file is the check that catches
//! that class of defect directly against the checker, not against a
//! hand-picked substring of the problem or answer text.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::path::Path;

use cadus_core::answer::{Outcome, check, check_contract};
use cadus_core::curriculum::{AnswerKind, Curriculum, load_curriculum};

fn curriculum() -> Curriculum {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
    load_curriculum(&root).unwrap().0
}

fn expected_answer<'a>(
    curriculum: &'a Curriculum,
    topic_id: &str,
    kp_id: &str,
    index: usize,
) -> &'a str {
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
    item.answer.as_str()
}

fn grade(
    curriculum: &Curriculum,
    topic_id: &str,
    kp_id: &str,
    index: usize,
    learner: &str,
) -> Outcome {
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
    let item = &kp.exemplars[index];
    match &item.answer_contract {
        Some(contract) => check_contract(&item.answer, learner, contract.clone()),
        None => check(&item.answer, learner, AnswerKind::MultiStep),
    }
}

fn assert_correct(curriculum: &Curriculum, topic_id: &str, kp_id: &str, index: usize) {
    let expected = expected_answer(curriculum, topic_id, kp_id, index).to_owned();
    let outcome = grade(curriculum, topic_id, kp_id, index, &expected);
    assert!(
        matches!(outcome, Outcome::Decided(verdict) if verdict.correct),
        "{topic_id}/{kp_id}[{index}]: the KP's own authored answer {expected:?} was not \
         graded correct by the production checker ({outcome:?})"
    );
}

fn assert_rejects(
    curriculum: &Curriculum,
    topic_id: &str,
    kp_id: &str,
    index: usize,
    wrong: &str,
    reason: &str,
) {
    let outcome = grade(curriculum, topic_id, kp_id, index, wrong);
    assert!(
        matches!(outcome, Outcome::Decided(verdict) if !verdict.correct),
        "{topic_id}/{kp_id}[{index}]: {reason} ({wrong:?} graded {outcome:?}, expected \
         Decided{{correct:false}})"
    );
}

/// One row: the authored answer must grade correct, and `wrong` (a specific
/// adversarial near-miss, not a random string) must grade incorrect.
struct Row {
    topic_id: &'static str,
    kp_id: &'static str,
    index: usize,
    wrong: &'static str,
    reason: &'static str,
}

const ROWS: &[Row] = &[
    // point-slope-standard-form/kp1: the contract requires the canonical
    // A > 0 standard form; a sign-reversed but algebraically equivalent
    // representation must still be rejected (CORRECTION4 item 1).
    Row {
        topic_id: "point-slope-standard-form",
        kp_id: "kp1",
        index: 0,
        wrong: "-2x + y = 3",
        reason: "a sign-reversed (A < 0) but algebraically equivalent standard form must \
                 be rejected: the contract requires the canonical A > 0 orientation",
    },
    Row {
        topic_id: "point-slope-standard-form",
        kp_id: "kp1",
        index: 2,
        wrong: "-3x + y = 2",
        reason: "a sign-reversed (A < 0) but algebraically equivalent standard form must \
                 be rejected: the contract requires the canonical A > 0 orientation",
    },
    // interval-notation/kp2: unbounded intervals; the endpoint bracket
    // (round vs. square) and the direction of the ray must both be exact.
    Row {
        topic_id: "interval-notation",
        kp_id: "kp2",
        index: 0,
        wrong: "[2, ∞)",
        reason: "a square bracket next to infinity is never correct (infinity is not a \
                 number, so it never closes)",
    },
    Row {
        topic_id: "interval-notation",
        kp_id: "kp2",
        index: 1,
        wrong: "(-∞, -1)",
        reason: "the finite endpoint -1 is included (<=), so a round bracket there is wrong",
    },
    Row {
        topic_id: "interval-notation",
        kp_id: "kp2",
        index: 3,
        wrong: "x < 3",
        reason: "the source interval's finite endpoint 3 is included (square bracket), so \
                 strict < is wrong",
    },
    // interval-notation/kp3: unions; a swapped strict/inclusive bracket on
    // either piece must be rejected, and the pure-inequality row is graded
    // by real algebra (`inequality_union`), not a literal contract.
    Row {
        topic_id: "interval-notation",
        kp_id: "kp3",
        index: 0,
        wrong: "(-∞, -2] ∪ (4, ∞)",
        reason: "the source uses strict < at -2, so an inclusive bracket there is wrong",
    },
    Row {
        topic_id: "interval-notation",
        kp_id: "kp3",
        index: 1,
        wrong: "(-∞, -2) ∪ (3, ∞)",
        reason: "$2x + 1 \\le -3$ solves to the inclusive $x \\le -2$, so a round bracket \
                 there is wrong",
    },
    Row {
        topic_id: "interval-notation",
        kp_id: "kp3",
        index: 2,
        wrong: "x <= 1 or x >= 5",
        reason: "the source interval $(-\\infty, 1)$ is open at 1, so <= there is wrong; \
                 `inequality_union` decides this by real interval algebra, not a literal \
                 string match",
    },
    Row {
        topic_id: "interval-notation",
        kp_id: "kp3",
        index: 3,
        wrong: "(-∞, -5) ∪ (4, ∞)",
        reason: "$x + 1 \\le -4$ solves to the inclusive $x \\le -5$, so a round bracket \
                 there is wrong",
    },
    // interpreting-graphs-qualitatively: a swapped classification label
    // (the kind of mistake a label contract exists to catch) must fail.
    Row {
        topic_id: "interpreting-graphs-qualitatively",
        kp_id: "kp1",
        index: 1,
        wrong: "increasing",
        reason: "the graph goes downward, so the opposite classification must be rejected",
    },
    Row {
        topic_id: "interpreting-graphs-qualitatively",
        kp_id: "kp3",
        index: 0,
        wrong: "runner B",
        reason: "runner A's line is steeper, so naming the other runner must be rejected",
    },
    // interpreting-linear-models: the classic slope/intercept mixup — the
    // OTHER row's own correct answer, offered as a wrong answer here.
    Row {
        topic_id: "interpreting-linear-models",
        kp_id: "kp1",
        index: 0,
        wrong: "the fixed fee charged regardless of hours (€40)",
        reason: "naming the model's intercept when asked for the slope's meaning (a real \
                 slope/intercept mixup) must be rejected",
    },
    Row {
        topic_id: "interpreting-linear-models",
        kp_id: "kp2",
        index: 1,
        wrong: "the cost per hour of work (€12 per hour)",
        reason: "naming the model's slope when asked for the intercept's meaning (a real \
                 slope/intercept mixup) must be rejected",
    },
];

#[test]
fn every_repaired_row_grades_its_own_answer_correct() {
    let curriculum = curriculum();
    for row in ROWS {
        assert_correct(&curriculum, row.topic_id, row.kp_id, row.index);
    }
}

#[test]
fn every_repaired_row_rejects_its_adversarial_near_miss() {
    let curriculum = curriculum();
    for row in ROWS {
        assert_rejects(
            &curriculum,
            row.topic_id,
            row.kp_id,
            row.index,
            row.wrong,
            row.reason,
        );
    }
}

#[test]
fn every_interval_row_uses_set_equivalence_and_rejects_an_endpoint_error() {
    use cadus_core::answer::AnswerContract;
    let curriculum = curriculum();
    let wrong = [
        ("kp1", 0, "(2, 7]"),
        ("kp1", 1, "[0, 5]"),
        ("kp1", 2, "[1, 6]"),
        ("kp1", 3, "(-3, 4)"),
        ("kp2", 0, "[2, ∞)"),
        ("kp2", 1, "(-∞, -1)"),
        ("kp2", 2, "x > 4"),
        ("kp2", 3, "x < 3"),
        ("kp3", 0, "(-∞, -2] ∪ (4, ∞)"),
        ("kp3", 1, "(-∞, -2) ∪ (3, ∞)"),
        ("kp3", 2, "x <= 1 or x >= 5"),
        ("kp3", 3, "(-∞, -5) ∪ (4, ∞)"),
    ];
    let topic = curriculum
        .topics()
        .iter()
        .find(|topic| topic.id.as_str() == "interval-notation")
        .unwrap();
    for (kp_id, index, near_miss) in wrong {
        let kp = topic
            .knowledge_points
            .iter()
            .find(|kp| kp.id.as_str() == kp_id)
            .unwrap();
        assert!(
            matches!(
                kp.exemplars[index].answer_contract,
                Some(AnswerContract::InequalityUnion)
            ),
            "interval-notation/{kp_id}[{index}] must use inequality_union"
        );
        assert_rejects(
            &curriculum,
            "interval-notation",
            kp_id,
            index,
            near_miss,
            "changed endpoint direction or openness must be incorrect",
        );
    }
}

#[test]
fn canonical_standard_form_is_visible_and_rejects_sign_reversal() {
    let curriculum = curriculum();
    let wrong = [
        ("kp1", 0, "-2x + y = 3"),
        ("kp1", 1, "-x + 2y = -2"),
        ("kp1", 2, "-3x + y = 2"),
        ("kp1", 3, "-x + 2y = 4"),
        ("kp3", 0, "-3x + y = -1"),
        ("kp3", 1, "-x + 2y = 8"),
        ("kp3", 2, "-3x + y = -3"),
        ("kp3", 3, "-x + 2y = 7"),
    ];
    for (kp_id, index, near_miss) in wrong {
        let topic = curriculum
            .topics()
            .iter()
            .find(|topic| topic.id.as_str() == "point-slope-standard-form")
            .unwrap();
        let kp = topic
            .knowledge_points
            .iter()
            .find(|kp| kp.id.as_str() == kp_id)
            .unwrap();
        assert!(
            kp.exemplars[index].problem.contains("A > 0"),
            "point-slope-standard-form/{kp_id}[{index}] hides the canonical sign rule"
        );
        assert_rejects(
            &curriculum,
            "point-slope-standard-form",
            kp_id,
            index,
            near_miss,
            "the explicitly excluded A < 0 form must be incorrect",
        );
    }
}

#[test]
fn interpretation_labels_are_bounded_by_the_visible_prompt() {
    use cadus_core::answer::AnswerContract;
    let curriculum = curriculum();
    for (topic_id, kp_ids) in [
        (
            "interpreting-graphs-qualitatively",
            &["kp1", "kp2", "kp3"][..],
        ),
        ("interpreting-linear-models", &["kp1", "kp2"][..]),
    ] {
        let topic = curriculum
            .topics()
            .iter()
            .find(|topic| topic.id.as_str() == topic_id)
            .unwrap();
        for kp_id in kp_ids {
            let kp = topic
                .knowledge_points
                .iter()
                .find(|kp| kp.id.as_str() == *kp_id)
                .unwrap();
            for (index, item) in kp.exemplars.iter().enumerate() {
                assert!(
                    item.problem.contains(" or "),
                    "{topic_id}/{kp_id}[{index}] must show its bounded choices"
                );
                let Some(AnswerContract::Label { options }) = &item.answer_contract else {
                    panic!("{topic_id}/{kp_id}[{index}] must use a label contract");
                };
                assert!(
                    options.len() >= 2,
                    "{topic_id}/{kp_id}[{index}] must offer at least two choices"
                );
                assert!(
                    options.iter().flatten().any(|alias| alias == &item.answer),
                    "{topic_id}/{kp_id}[{index}] answer is outside its choices"
                );
            }
        }
    }
}
