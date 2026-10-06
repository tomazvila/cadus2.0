//! Natural learner spellings (spec section 8.7): measured decoration, spoken
//! verdicts, a verdict and its value in one sentence, scientific notation,
//! solution lists, unit phrases and the new required forms. Every reading has
//! a false-positive probe beside it.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{
    AnswerContract, NumericForm, Outcome, check_contract, measured_answer, scientific, unit_phrases,
};

fn contract(json: &str) -> AnswerContract {
    serde_json::from_str(json).unwrap()
}

fn verdict(key: &str, json: &str, learner: &str) -> Option<bool> {
    match check_contract(key, learner, contract(json)) {
        Outcome::Decided(verdict) => Some(verdict.correct),
        Outcome::Undecidable(_) => None,
    }
}

/// Grade a learner answer the way the served route does: strip the measured
/// decoration the question allows, then grade under the item's contract.
fn served(problem: &str, key: &str, json: &str, learner: &str) -> Option<bool> {
    let text = measured_answer(problem, key, learner).unwrap_or_else(|| learner.to_owned());
    verdict(key, json, &text)
}

const AREA: &str = "A rectangle has area $x^2 + 9x + 20$ square metres and width $x + 4$ metres. Find its area in factored form and its length.";
const FACTORED: &str = r#"{"kind":"required_form","form":"factored_polynomial"}"#;
const EXPANDED: &str = r#"{"kind":"required_form","form":"expanded_polynomial"}"#;
const EXACT: &str = r#"{"kind":"exact"}"#;
const APPROX: &str = r#"{"kind":"approx","decimals":1}"#;

#[test]
fn a_named_and_measured_answer_grades_its_value() {
    assert_eq!(
        served(AREA, "(x + 4)(x + 5)", FACTORED, "A = (x+4)(x+5)"),
        Some(true)
    );
    assert_eq!(
        served(AREA, "(x + 4)(x + 5)", FACTORED, "(x+4)(x+5) m^2"),
        Some(true)
    );
    assert_eq!(
        served(
            AREA,
            "(x + 4)(x + 5)",
            FACTORED,
            "Area = (x+4)(x+5) square metres"
        ),
        Some(true)
    );
    assert_eq!(served(AREA, "x + 5", EXACT, "x + 5 m"), Some(true));
    assert_eq!(served(AREA, "x + 5", EXACT, "L = x + 5 m"), Some(true));
    let frame = "A frame has area $2x^2 - 2x - 4$ cm$^2$. Expand the product.";
    assert_eq!(
        served(frame, "2x^2 - 2x - 4", EXPANDED, "2x^2-2x-4 cm^2"),
        Some(true)
    );
    let ladder = "How long is the ladder, in metres, to one decimal place?";
    assert_eq!(served(ladder, "7.1", APPROX, "7.1 m"), Some(true));
    assert_eq!(served(ladder, "7.1", APPROX, "≈7.1"), Some(true));
    assert_eq!(served(ladder, "7.1", APPROX, "≈ 7.1 m"), Some(true));
}

#[test]
fn a_stripped_token_never_changes_the_mathematics() {
    // The decoration still has to be right: a wrong value stays wrong.
    assert_eq!(
        served(AREA, "(x + 4)(x + 5)", FACTORED, "A = x^2 + 9x + 20"),
        Some(false)
    );
    assert_eq!(served(AREA, "x + 5", EXACT, "x + 6 m"), Some(false));
    // A letter that is a variable of the key is never a unit.
    assert_eq!(
        measured_answer("Simplify $5m$ in metres.", "5m", "5 m"),
        None
    );
    // x, e, i and pi are not units; they stay in the value.
    for learner in ["x + 5 x", "x + 5 e", "x + 5 i", "x + 5 pi"] {
        assert_eq!(measured_answer(AREA, "x + 5", learner), None, "{learner}");
    }
    // A unit the question never names is not stripped.
    assert_eq!(measured_answer(AREA, "x + 5", "x + 5 kg"), None);
    assert_eq!(
        measured_answer("Simplify $(x+1)^2$.", "x^2 + 2x + 1", "x^2 + 2x + 1 m"),
        None
    );
    // A key that names its value keeps the learner's name.
    assert_eq!(measured_answer(AREA, "x = 5", "A = 5"), None);
    // The approximately sign is only read for a numeric key.
    assert_eq!(measured_answer(AREA, "x + 5", "≈ x + 5"), None);
    // Only quantity names fall away.
    assert_eq!(measured_answer(AREA, "x + 5", "y = x + 5"), None);
}

const VERDICT: &str = r#"{"kind":"label","options":[["no solution","none","no real solution"],["all real numbers","every real number"]]}"#;

#[test]
fn a_spoken_verdict_names_its_option() {
    for learner in [
        "There are no real solutions.",
        "no real roots",
        "∅",
        "{}",
        "no (real) solution",
        "No solution!",
        "the equation has no real roots",
    ] {
        assert_eq!(
            verdict("no solution", VERDICT, learner),
            Some(true),
            "{learner}"
        );
    }
    let longer = r#"{"kind":"label","options":[["longer","the bacterium","the bacterium is longer"],["shorter","the virus"]]}"#;
    assert_eq!(
        verdict("longer", longer, "The bacterium is longer."),
        Some(true)
    );
    assert_eq!(verdict("longer", longer, "it is longer"), Some(true));
}

#[test]
fn a_spoken_verdict_never_names_the_other_option() {
    assert_eq!(
        verdict("no solution", VERDICT, "every real number."),
        Some(false)
    );
    assert_eq!(
        verdict("no solution", VERDICT, "two real solutions"),
        Some(false)
    );
    let longer =
        r#"{"kind":"label","options":[["longer","the bacterium"],["shorter","the virus"]]}"#;
    assert_eq!(verdict("longer", longer, "it is not longer"), Some(false));
    assert_eq!(
        verdict("longer", longer, "the virus is shorter"),
        Some(false)
    );
    // An answer that names both options names neither.
    let both = r#"{"kind":"label","options":[["high"],["low"]]}"#;
    assert_eq!(verdict("high", both, "high and low"), Some(false));
}

const ESTIMATE: &str = r#"{"kind":"multipart","parts":[{"name":"estimate","contract":{"kind":"label","options":[["too high","high","an overestimate"],["too low","low","an underestimate"]]}},{"name":"by","contract":{"kind":"exact"}}]}"#;

#[test]
fn a_verdict_and_its_value_read_from_one_sentence() {
    let key = "estimate = too high; by = 160";
    assert_eq!(verdict(key, ESTIMATE, "too high by 160"), Some(true));
    assert_eq!(verdict(key, ESTIMATE, "Too high by 160"), Some(true));
    assert_eq!(verdict(key, ESTIMATE, "too high 160"), Some(true));
    assert_eq!(verdict(key, ESTIMATE, "too high, 160"), Some(true));
    assert_eq!(verdict(key, ESTIMATE, "too low by 160"), Some(false));
    assert_eq!(verdict(key, ESTIMATE, "too high by 150"), Some(false));
    assert_ne!(verdict(key, ESTIMATE, "too high"), Some(true));
}

#[test]
fn named_parts_may_be_separated_by_commas() {
    let abc = r#"{"kind":"multipart","parts":[{"name":"a","contract":{"kind":"exact"}},{"name":"b","contract":{"kind":"exact"}},{"name":"c","contract":{"kind":"exact"}}]}"#;
    let key = "a = 1; b = -3; c = 2";
    assert_eq!(verdict(key, abc, "a = 1, b = -3, c = 2"), Some(true));
    assert_eq!(verdict(key, abc, "1, -3, 2"), Some(true));
    assert_eq!(verdict(key, abc, "b = -3, a = 1, c = 2"), Some(true));
    assert_eq!(verdict(key, abc, "a = 1, b = 3, c = 2"), Some(false));
    assert_eq!(verdict(key, abc, "-3, 1, 2"), Some(false));
}

#[test]
fn scientific_spellings_read_as_the_number() {
    let sci = r#"{"kind":"required_normalized_scientific_notation"}"#;
    for learner in [
        "3.2x10^5",
        "3.2×10^5",
        "3.2*10^5",
        "3.2e5",
        "3.2 X 10^5",
        "3.2 x 10^{5}",
    ] {
        assert_eq!(verdict("3.2*10^5", sci, learner), Some(true), "{learner}");
    }
    for learner in ["7.2e-4", "7.2×10⁻⁴", "7.2x10^-4", "7.2 × 10^(-4)"] {
        assert_eq!(verdict("7.2*10^-4", sci, learner), Some(true), "{learner}");
    }
    assert_eq!(verdict("3.2*10^5", EXACT, "3.2e5"), Some(true));
    // The notation is still required, and the value still has to agree.
    assert_eq!(verdict("3.2*10^5", sci, "32x10^4"), Some(false));
    assert_eq!(verdict("3.2*10^5", sci, "3.2e6"), Some(false));
    // Only a whole number-times-power-of-ten answer is read this way.
    assert_eq!(scientific("3.2x"), None);
    assert_eq!(scientific("2e"), None);
    assert_eq!(scientific("3x10"), None);
    assert_eq!(scientific("x10^5"), None);
    // A variable key keeps x as a variable.
    assert_ne!(verdict("3.2x", EXACT, "3.2x10^5"), Some(true));
}

#[test]
fn solution_lists_read_plus_minus_and_double_roots() {
    assert_eq!(verdict("-8, 0, 8", EXACT, "x = ±8 or x = 0"), Some(true));
    assert_eq!(verdict("-8, 0, 8", EXACT, "x = 0, ±8"), Some(true));
    assert_eq!(verdict("-4, 4", EXACT, "x = +-4"), Some(true));
    assert_eq!(verdict("-4, 4", EXACT, "+-4"), Some(true));
    assert_eq!(verdict("3", EXACT, "3, 3"), Some(true));
    assert_eq!(verdict("3", EXACT, "x=3 or x=3"), Some(true));
    // A sign written inside the value keeps its plain reading.
    assert_eq!(verdict("-1", EXACT, "3 +- 4"), Some(true));
    // A wrong member stays wrong.
    assert_eq!(verdict("-8, 0, 8", EXACT, "x = ±8"), Some(false));
    assert_eq!(verdict("3", EXACT, "3, -3"), Some(false));
    assert_ne!(verdict("3", EXACT, "x = ±3"), Some(true));
    assert_ne!(verdict("3", EXACT, "(3, 3)"), Some(true));
}

#[test]
fn a_list_after_one_name_reads_as_its_members() {
    let list = r#"{"kind":"list","ordered":false,"member":{"kind":"exact"}}"#;
    assert_eq!(verdict("7, 8, 13", list, "b = 7, 8, 13"), Some(true));
    assert_eq!(verdict("7, 8, 13", list, "b = 7, 8, 14"), Some(false));
    assert_ne!(verdict("7, 8, 13", list, "b = 7, b = 8, 13"), Some(true));
}

#[test]
fn unit_phrases_read_in_table_spelling() {
    assert_eq!(unit_phrases("25 square metres"), "25 m^2");
    assert_eq!(unit_phrases("4 sec"), "4 s");
    assert_eq!(unit_phrases("10 m long"), "10 m");
    assert_eq!(unit_phrases("x long"), "x long");
    assert_eq!(unit_phrases("10 long"), "10 long");
    let area = r#"{"kind":"unit","quantity":"area","unit":"m^2","allow_omitted":true}"#;
    let time = r#"{"kind":"unit","quantity":"time","unit":"s","allow_omitted":true}"#;
    let length = r#"{"kind":"unit","quantity":"length","unit":"m","allow_omitted":true}"#;
    assert_eq!(verdict("25", area, "25 square metres"), Some(true));
    assert_eq!(verdict("4", time, "4 sec"), Some(true));
    assert_eq!(verdict("10", length, "10 m long"), Some(true));
    assert_eq!(verdict("10", length, "10 cm long"), Some(false));
    assert_eq!(verdict("25", area, "25 square centimetres"), Some(false));
}

#[test]
fn a_unit_contract_can_require_the_simplest_radical() {
    let radical = r#"{"kind":"unit","quantity":"length","unit":"m","allow_omitted":true,"form":"simplest_radical"}"#;
    for learner in ["2√3 m", "2sqrt(3) m", "2√3", "200√3 cm", "2 sqrt 3 m"] {
        assert_eq!(
            verdict("2sqrt(3)", radical, learner),
            Some(true),
            "{learner}"
        );
    }
    for learner in ["√12 m", "sqrt(12)", "6/√3 m", "2√3 cm", "3√2 m"] {
        assert_eq!(
            verdict("2sqrt(3)", radical, learner),
            Some(false),
            "{learner}"
        );
    }
    // Without the form an equal unsimplified length is correct.
    let plain = r#"{"kind":"unit","quantity":"length","unit":"m","allow_omitted":true}"#;
    assert_eq!(verdict("2sqrt(3)", plain, "√12 m"), Some(true));
    // The key itself must be in the form.
    assert_eq!(verdict("sqrt(12)", radical, "2√3 m"), None);
}

fn form(form: NumericForm) -> String {
    serde_json::to_string(&AnswerContract::RequiredForm { form }).unwrap()
}

#[test]
fn a_radical_sum_must_be_simplified_and_combined() {
    let sum = form(NumericForm::SimplestRadicalSum);
    for learner in ["1 + 2√2", "2√2 + 1", "1 + 2sqrt(2)"] {
        assert_eq!(
            verdict("1 + 2sqrt(2)", &sum, learner),
            Some(true),
            "{learner}"
        );
    }
    for learner in [
        "(1 + √2)(3 - √2) - 2",
        "1 + √8",
        "√2 + √2 + 1",
        "3 - 2 + 2√2",
        "1 + 4/√2",
    ] {
        assert_eq!(
            verdict("1 + 2sqrt(2)", &sum, learner),
            Some(false),
            "{learner}"
        );
    }
    assert_eq!(verdict("2sqrt(3) + sqrt(6)", &sum, "√12 + √6"), Some(false));
    assert_eq!(verdict("2sqrt(3) + sqrt(6)", &sum, "√6 + 2√3"), Some(true));
    assert_eq!(verdict("-3 - 3sqrt(5)", &sum, "-3 - 3√5"), Some(true));
}

#[test]
fn standard_form_writes_the_degrees_in_descending_order() {
    let standard = form(NumericForm::StandardFormPolynomial);
    assert_eq!(
        verdict("-x^2 + 5x + 3", &standard, "-x^2 + 5x + 3"),
        Some(true)
    );
    assert_eq!(
        verdict("-x^2 + 5x + 3", &standard, "3 - x^2 + 5x"),
        Some(false)
    );
    assert_eq!(
        verdict("-x^2 + 5x + 3", &standard, "5x - x^2 + 3"),
        Some(false)
    );
    assert_eq!(verdict("4x^3 - 2x", &standard, "4x^3-2x"), Some(true));
    assert_eq!(
        verdict("4x^3 - 2x", &standard, "4x^3 - 2x + 0x^2"),
        Some(false)
    );
}

#[test]
fn a_rational_exponent_is_one_power_and_a_radical_has_no_fraction_exponent() {
    let power = form(NumericForm::RationalExponent);
    for learner in ["x^(5/6)", "x^{5/6}"] {
        assert_eq!(verdict("x^(5/6)", &power, learner), Some(true), "{learner}");
    }
    for learner in ["x^(1/2)*x^(1/3)", "sqrt(x)*x^(1/3)", "x^(10/12)/1 + 0"] {
        assert_ne!(verdict("x^(5/6)", &power, learner), Some(true), "{learner}");
    }
    assert_eq!(verdict("3x^(1/2)", &power, "3x^(1/2)"), Some(true));
    assert_eq!(verdict("3x^(1/2)", &power, "3sqrt(x)"), Some(false));
    let radical = form(NumericForm::Radical);
    assert_eq!(verdict("x*sqrt(x)", &radical, "x√x"), Some(true));
    assert_eq!(verdict("x*sqrt(x)", &radical, "x^(3/2)"), Some(false));
}
