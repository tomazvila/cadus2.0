//! Exact finite-decimal template answers and required learner notation.
#![allow(clippy::unwrap_used)]

use cadus_core::answer::{AnswerContract, NumericForm, Outcome, canonical_form, check_contract};
use cadus_core::template::{Answer, Bindings, EvalError, answer_for_contract, parse_answer_expr};

fn decimal() -> AnswerContract {
    AnswerContract::RequiredForm {
        form: NumericForm::Decimal,
    }
}

fn write(expression: &str, contract: &AnswerContract) -> Result<Answer, EvalError> {
    answer_for_contract(
        &parse_answer_expr(expression)?,
        &Bindings::new(),
        Some(contract),
    )
}

fn correct(expected: &str, learner: &str, contract: &AnswerContract) -> bool {
    matches!(check_contract(expected, learner, contract.clone()), Outcome::Decided(v) if v.correct)
}

#[test]
fn finite_decimals_preserve_exact_values_and_required_notation() {
    for (expression, expected) in [
        ("1/2", "0.5"),
        ("3/8", "0.375"),
        ("5/16", "0.3125"),
        ("-7/20", "-0.35"),
        ("7/2", "3.5"),
        ("1/125", "0.008"),
        ("2", "2.0"),
        ("-2", "-2.0"),
        ("0", "0.0"),
        ("-0/8", "0.0"),
    ] {
        let result = write(expression, &decimal()).unwrap();
        assert_eq!(result.text, expected, "{expression}");
        assert_eq!(result.canon, canonical_form(expression).unwrap());
        assert!(correct(&result.text, expected, &decimal()));
    }
    let result = write("1/2", &decimal()).unwrap();
    assert!(correct(&result.text, "0.50", &decimal()));
    for learner in ["1/2", "2/4", "0.49", "0.5+0"] {
        assert!(!correct(&result.text, learner, &decimal()), "{learner}");
    }
}

#[test]
fn terminating_rational_grid_round_trips_without_float_rounding() {
    for numerator in -7..=7 {
        for denominator in [2, 4, 5, 8, 10, 16, 20, 25, 40] {
            let expression = format!("({numerator})/{denominator}");
            let rendered = write(&expression, &decimal()).unwrap();
            assert_eq!(rendered.canon, canonical_form(&expression).unwrap());
            assert!(decimal().validate_expected(&rendered.text).is_ok());
        }
    }
}

#[test]
fn repeating_symbolic_and_oversized_decimals_are_refused() {
    for expression in ["1/3", "5/6", "sqrt(2)", "x+1", "1/(2^1000*2)", "1/0"] {
        assert!(write(expression, &decimal()).is_err(), "{expression}");
    }
    let boundary = write("1/2^1000", &decimal()).unwrap();
    assert_eq!(boundary.text.len(), 1002);
    assert_eq!(boundary.canon, canonical_form("1/2^1000").unwrap());
    assert_eq!(write("1/2", &AnswerContract::Exact).unwrap().text, "1/2");
}

#[test]
fn multipart_decimal_fields_enforce_notation_and_all_parts() {
    let contract: AnswerContract = serde_json::from_value(serde_json::json!({
        "kind":"multipart", "parts":[
            {"name":"whole", "contract":{"kind":"required_form","form":"integer"}},
            {"name":"decimal", "contract":{"kind":"required_form","form":"decimal"}}
        ]
    }))
    .unwrap();
    let result = write("multipart(3,5/16)", &contract).unwrap();
    assert_eq!(result.text, "whole = 3; decimal = 0.3125");
    assert!(correct(&result.text, "decimal=0.31250; whole=3", &contract));
    for learner in [
        "whole=3; decimal=5/16",
        "whole=3.0; decimal=0.3125",
        "decimal=0.3125",
        "whole=3; decimal=0.312",
    ] {
        assert!(!correct(&result.text, learner, &contract), "{learner}");
    }
}
