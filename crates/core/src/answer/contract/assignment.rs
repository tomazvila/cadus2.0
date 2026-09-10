//! Exact values that retain one authored assignment target.

use super::{Canon, Undecidable, canonical_form};
use crate::answer::same_answer;

pub(super) fn expected(text: &str) -> Result<Canon, Undecidable> {
    let value = canonical_form(text)?;
    if matches!(value, Canon::Assign { .. }) {
        Ok(value)
    } else {
        Err(Undecidable::new(
            "the authored answer must be an assignment such as y = 4",
        ))
    }
}

pub(super) fn equivalent(expected_text: &str, learner: &str) -> Result<bool, Undecidable> {
    let Canon::Assign {
        var: expected_var,
        value: expected_value,
    } = expected(expected_text)?
    else {
        unreachable!("expected validates the assignment shape");
    };
    let learner = canonical_form(learner)?;
    let Canon::Assign {
        var: learner_var,
        value: learner_value,
    } = learner
    else {
        return Ok(false);
    };
    Ok(expected_var.to_lowercase() == learner_var.to_lowercase()
        && same_answer(&expected_value, &learner_value))
}
