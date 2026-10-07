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

const ANGLE: &str =
    "A ladder makes an angle with the ground. Find the angle, to one decimal place, in degrees.";
const METRES: &str = "A ramp rises 3 metres over a run of 4 metres. How long is the ramp, to one decimal place, in metres?";
const TEMPERATURE: &str = "At night the temperature falls from 5 to a lower value. What is the new temperature in degrees Celsius?";
const RADIANS: &str = "A wheel turns through 150 degrees. Write the angle in radians.";
const YEARS: &str =
    "Sam saves 12 euros each year. After how many years has Sam saved 36 euros? Answer in years.";
const PENCILS: &str = "Maria puts 8 pencils in each of 3 boxes. Find the total number of pencils.";
const SIDES: &str =
    "A field has two sides of 27 metres and 25 metres. List the two lengths in metres.";
const MILLIONS: &str =
    "A firm earned 13 million dollars. Write the earnings in millions of dollars.";
const CONNECTED: &str = "Is the graph connected?";
const TWO_PARTS: &str = "Find the two numbers.";
const MONEY: &str = "How much does the bike cost, in dollars?";

const DOLLAR: &str = r#"{"kind":"unit","quantity":"dollar","unit":"dollar","allow_omitted":true}"#;
const KM: &str = r#"{"kind":"unit","quantity":"length","unit":"km","allow_omitted":true}"#;
const FUNCTION_X: &str = r#"{"kind":"function","vars":["x"]}"#;
const TWO_DECIMALS: &str = r#"{"kind":"approx","decimals":2}"#;
const ONE_DECIMAL: &str = r#"{"kind":"approx","decimals":1}"#;
const SET: &str = r#"{"kind":"set"}"#;
const CONNECTED_LABEL: &str = r#"{"kind":"label","options":[["yes","connected","it is connected"],["no","not connected","disconnected"]]}"#;
const PAIR: &str = r#"{"kind":"multipart","parts":[{"name":"a","contract":{"kind":"exact"}},{"name":"b","contract":{"kind":"exact"}}]}"#;
const SETUP: &str = r#"{"kind":"relation_setup"}"#;
const DIVISION: &str = r#"{"kind":"polynomial_division","divisor":"x+1"}"#;

fn cases_pass_3() -> Vec<Case> {
    vec![
        case(
            ANGLE,
            "36.9",
            ONE_DECIMAL,
            &["36.9°", "36.9 degrees", "36.9", "≈36.9°", "36.91"],
            &["37°", "36.8°", "36.8 degrees"],
        ),
        case(
            METRES,
            "5.0",
            ONE_DECIMAL,
            &["5 m", "5.0 m", "5.0"],
            &["5.1 m", "5 cm"],
        ),
        case(
            TEMPERATURE,
            "-15",
            EXACT,
            &["-15 degrees", "-15°C", "-15 °C", "-15"],
            &["-14 degrees", "15 degrees"],
        ),
        case(
            RADIANS,
            "5*pi/6",
            EXACT,
            &["5pi/6 radians", "5π/6 radians", "5π/6", "5pi/6 rad"],
            &["5pi/3 radians", "5 radians"],
        ),
        case(
            YEARS,
            "3",
            EXACT,
            &["3 years", "3 year", "3"],
            &["4 years", "3 months"],
        ),
        case(
            PENCILS,
            "24",
            EXACT,
            &["24 pencils", "24"],
            &["25 pencils", "23 pencils"],
        ),
        case(
            SIDES,
            "27, 25",
            EXACT,
            &[
                "27 m, 25 m",
                "27 metres and 25 metres",
                "27, 25",
                "25 m, 27 m",
            ],
            &["27 m, 26 m", "27 cm, 25 cm"],
        ),
        case(
            TWO_PARTS,
            "a = 18; b = 24",
            PAIR,
            &["18 and 24", "18, 24", "18; 24"],
            &["24 and 18", "18 and 25"],
        ),
        case(
            MILLIONS,
            "13",
            EXACT,
            &["13 million dollars", "13 million", "13"],
            &["14 million dollars", "130 million dollars"],
        ),
        case(
            MONEY,
            "1710",
            DOLLAR,
            &["$1,710", "1,710 dollars", "$1710", "1710 dollars"],
            &["$1,711", "$171"],
        ),
        case(
            MONEY,
            "-20",
            DOLLAR,
            &["-$20", "-20 dollars", "-$20.00"],
            &["$20", "-$21"],
        ),
        case(
            MONEY,
            "12700",
            DOLLAR,
            &["$12,700", "12,700 dollars"],
            &["$1,270"],
        ),
        case(MONEY, "-3.65", DOLLAR, &["-$3.65"], &["-$3.56"]),
        case(
            PLAIN,
            "13 km",
            KM,
            &["d = 13 km", "d = 13", "13 km"],
            &["d = 14 km"],
        ),
        case(
            PLAIN,
            "2x",
            FUNCTION_X,
            &["f'(x) = 2x", "dy/dx = 2x"],
            &["f'(x) = 3x"],
        ),
        case(
            PLAIN,
            "2.81",
            TWO_DECIMALS,
            &["2.81", "2.807", "2.8149"],
            &["2.9", "2.8", "2.8049"],
        ),
        case(
            CONNECTED,
            "yes",
            CONNECTED_LABEL,
            &["Yes, it is", "Yes, it must be connected"],
            &["No, it is not", "it is disconnected"],
        ),
        case(
            PLAIN,
            "{2, 5}",
            SET,
            &["2, 5", "5, 2", "{2,5}"],
            &["2, 2, 5", "2", "2, 6"],
        ),
        case(
            PLAIN,
            "m/2 + 3 >= 12",
            SETUP,
            &["0.5m+3>=12", "(1/2)m+3>=12", "12 <= m/2 + 3"],
            &["m/2 + 3 > 12", "m >= 18"],
        ),
        case(
            PLAIN,
            "|x| < 5",
            SETUP,
            &["|x| < 5", "5 > |x|"],
            &["|x| <= 5", "x < 5"],
        ),
        case(
            PLAIN,
            "x + 2 remainder 3",
            DIVISION,
            &["x+2+3/(x+1)", "x+2 r 3", "quotient x+2, remainder 3"],
            &["x+2+3/(x+2)", "x + 2 remainder 4"],
        ),
        case(
            PLAIN,
            "ln(8)",
            EXACT,
            &["3 ln 2", "3*ln(2)", "ln 8"],
            &["ln 9", "2 ln 3"],
        ),
        case(
            PLAIN,
            "log(45)",
            EXACT,
            &["log10(45)", "lg(45)", "ln(45)/ln(10)", "log(5)+log(9)"],
            &["ln(45)", "log(54)"],
        ),
        case(
            PLAIN,
            "ln(10)/ln(2)",
            EXACT,
            &["log_2(10)", "log2(10)"],
            &["log_3(10)", "ln(10)"],
        ),
        case(
            PLAIN,
            "x*x*x",
            &form("repeated_multiplication"),
            &["x*x*x"],
            &["x^3"],
        ),
        case(
            PLAIN,
            "4000 + 500 + 6",
            &form("expanded_place_value"),
            &["4000 + 500 + 6", "4*1000 + 5*100 + 6"],
            &["4506"],
        ),
        case(
            PLAIN,
            "3x/4",
            &form("simplified_rational"),
            &["3x/4"],
            &["6x/8"],
        ),
        case(
            PLAIN,
            "y = (x+3)^2 - 4",
            &form("vertex_form"),
            &["y = (x+3)^2 - 4"],
            &["y = x^2 + 6x + 5"],
        ),
    ]
}

#[test]
fn the_pass_three_spellings_grade_through_the_served_path() {
    let mut failures = Vec::new();
    let mut rows = 0;
    for case in cases_pass_3() {
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
