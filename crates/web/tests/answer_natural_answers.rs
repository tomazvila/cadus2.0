//! The served grading path accepts the natural spellings of a correct answer
//! and still refuses a wrong or unsimplified one. Each row is one served
//! question, its key and contract, one learner answer, and the verdict.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::AnswerKind;
use cadus_core::event::AttemptOutcome;
use cadus_web::grade::grade_served_item;
use common::lesson_problem;

const EXACT: &str = r#"{"kind":"exact"}"#;
const WIDTH: &str = "A garden bed is $x + 3$ metres wide on one side. The width is $x$ metres and the length is 5 metres more. Write an expression for the length in metres.";
const LADDER: &str = "A ladder leans against a wall. Its foot is 2 metres from the wall and its top is 6.8 metres up. How long is the ladder, to one decimal place, in metres?";
const AREA: &str = "A rug has area $x^2 + 9x + 20$ square metres and width $x + 4$ metres. Write the area as a product of factors, in square metres.";
const SECTIONS: &str =
    "A ribbon of 36 metres is cut into pieces of 3 metres. How many sections does she get?";
const PACKS: &str =
    "A collection of $2^9$ stamps is split into packs of $2^6$ stamps. How many packs are there?";
const SCIENTIFIC: &str = r#"{"kind":"required_normalized_scientific_notation"}"#;
const PLAIN: &str = "Simplify the expression.";

const RADICAL_UNIT: &str = r#"{"kind":"unit","quantity":"length","unit":"m","allow_omitted":true,"form":"simplest_radical"}"#;
const VERDICT: &str = r#"{"kind":"label","options":[["no solution","none","no real solution"],["all real numbers","every real number"]]}"#;
const ESTIMATE: &str = r#"{"kind":"multipart","parts":[{"name":"estimate","contract":{"kind":"label","options":[["too high","high","an overestimate"],["too low","low","an underestimate"]]}},{"name":"by","contract":{"kind":"exact"}}]}"#;
const ABC: &str = r#"{"kind":"multipart","parts":[{"name":"a","contract":{"kind":"exact"}},{"name":"b","contract":{"kind":"exact"}},{"name":"c","contract":{"kind":"exact"}}]}"#;
const SQUARE: &str = r#"{"kind":"multipart","parts":[{"name":"trinomial","contract":{"kind":"required_form","form":"expanded_polynomial"}},{"name":"square","contract":{"kind":"required_form","form":"factored_polynomial"}}]}"#;

fn form(name: &str) -> String {
    format!(r#"{{"kind":"required_form","form":"{name}"}}"#)
}

fn grade(question: &str, key: &str, contract: &str, learner: &str) -> AttemptOutcome {
    let contract: AnswerContract = serde_json::from_str(contract).unwrap();
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.answer_kind = Some("expression".to_owned());
    live.text = question.to_owned();
    live.expected.answer = key.to_owned();
    live.expected.answer_contract = Some(contract);
    grade_served_item(&live, learner, AnswerKind::Expression).outcome
}

struct Case {
    question: &'static str,
    key: &'static str,
    contract: String,
    right: &'static [&'static str],
    wrong: &'static [&'static str],
}

fn case(
    question: &'static str,
    key: &'static str,
    contract: &str,
    right: &'static [&'static str],
    wrong: &'static [&'static str],
) -> Case {
    Case {
        question,
        key,
        contract: contract.to_owned(),
        right,
        wrong,
    }
}

fn cases() -> Vec<Case> {
    vec![
        case(
            WIDTH,
            "x + 5",
            EXACT,
            &["x + 5 m", "L = x + 5", "x+5 metres"],
            &["x + 5 kg", "x + 6 m"],
        ),
        case(
            LADDER,
            "7.1",
            r#"{"kind":"approx","decimals":1}"#,
            &["7.1 m", "≈7.1", "≈ 7.1 m", "about 7.1", "7.1"],
            &["7.2 m", "7.1 cm", "about 7.2", "7.1 kg"],
        ),
        case(
            AREA,
            "(x + 4)(x + 5)",
            &form("factored_polynomial"),
            &[
                "A = (x+4)(x+5)",
                "(x+4)(x+5) m^2",
                "(x + 5)(x + 4) square metres",
                "(x+4)(x+5) m²",
            ],
            &["x^2 + 9x + 20 m^2"],
        ),
        case(
            SECTIONS,
            "12",
            EXACT,
            &["12 sections", "12 sections.", "She gets 12 sections"],
            &[
                "13 sections",
                "She does not get 12 sections",
                "at least 12 sections",
            ],
        ),
        case(
            PACKS,
            "8",
            EXACT,
            &["8 packs", "there are 8 packs"],
            &["9 packs", "there are not 8 packs"],
        ),
        case(
            PLAIN,
            "2*sqrt(3)",
            RADICAL_UNIT,
            &["2√3 m", "2 sqrt(3) m", "2sqrt(3)", "2√3 metres"],
            &["sqrt(12) m", "3.46 m"],
        ),
        case(
            PLAIN,
            "no solution",
            VERDICT,
            &[
                "There are no real solutions.",
                "no real roots",
                "∅",
                "the equation has no solution",
            ],
            &["all real numbers"],
        ),
        case(
            PLAIN,
            "estimate = too high; by = 160",
            ESTIMATE,
            &["too high by 160", "Too high, by 160", "too high; 160"],
            &["too low by 160", "too high by 150", "too high, 1, by 160"],
        ),
        case(
            PLAIN,
            "3.2*10^5",
            SCIENTIFIC,
            &["3.2x10^5", "3.2e5", "3.2 × 10^5", "3.2·10^5"],
            &["32*10^4"],
        ),
        case(PLAIN, "7.2*10^-4", SCIENTIFIC, &["7.2×10⁻⁴"], &["72×10⁻⁵"]),
        case(
            PLAIN,
            "-8, 0, 8",
            EXACT,
            &["x = ±8 or x = 0", "0, ±8", "x = 0, x = 8, x = -8"],
            &["±8"],
        ),
        case(PLAIN, "3", EXACT, &["3, 3", "x = 3 or x = 3"], &["3, 4"]),
        case(
            PLAIN,
            "a = 2; b = -7; c = 3",
            ABC,
            &["a = 2, b = -7, c = 3", "2, -7, 3", "a=2; b=-7; c=3"],
            &["a = 2, b = 7, c = 3"],
        ),
        case(
            PLAIN,
            "trinomial = x^2 + 8x + 16; square = (x + 4)^2",
            SQUARE,
            &["x^2 + 8x + 16; (x+4)^2", "x^2+8x+16, (x+4)(x+4)"],
            &["(x+4)^2; (x+4)^2"],
        ),
        case(
            PLAIN,
            "1 + 2*sqrt(2)",
            &form("simplest_radical_sum"),
            &["1+2√2", "2√2 + 1"],
            &["1 + √8", "(1+√2)(3-√2)"],
        ),
        case(
            PLAIN,
            "-x^2 + 5x + 3",
            &form("standard_form_polynomial"),
            &["-x^2 + 5x + 3"],
            &["3 + 5x - x^2", "5x - x^2 + 3"],
        ),
        case(
            PLAIN,
            "x^(5/6)",
            &form("rational_exponent"),
            &["x^(5/6)", "x^{5/6}"],
            &["x^(1/2) * x^(1/3)", r"\sqrt[6]{x^5}"],
        ),
        case(
            PLAIN,
            "root(4, x^3)",
            &form("radical"),
            &[r"\sqrt[4]{x^3}", "root(4,x^3)"],
            &["x^(3/4)"],
        ),
    ]
}

#[test]
fn every_natural_spelling_grades_through_the_served_path() {
    let mut failures = Vec::new();
    let mut rows = 0;
    for case in cases() {
        for learner in case.right {
            rows += 1;
            if grade(case.question, case.key, &case.contract, learner) != AttemptOutcome::Correct {
                failures.push(format!("should be correct: {} | {learner}", case.key));
            }
        }
        for learner in case.wrong {
            rows += 1;
            if grade(case.question, case.key, &case.contract, learner) == AttemptOutcome::Correct {
                failures.push(format!("should be wrong: {} | {learner}", case.key));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {rows} rows:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
