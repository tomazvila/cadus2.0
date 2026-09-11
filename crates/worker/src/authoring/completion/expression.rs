//! Derive arithmetic variants only after the authored answer verifies exactly.
use cadus_core::{
    answer::{Canon, same_answer},
    curriculum::Exemplar,
    template::{self, Bindings},
};

#[derive(Clone)]
pub(super) struct Calculation {
    pub expression: String,
    pub answer: String,
    pub integer: bool,
}

pub(super) fn evaluate(expression: &str) -> Option<Calculation> {
    let ast = template::parse_answer_expr(expression).ok()?;
    let value = template::answer(&ast, &Bindings::new()).ok()?;
    // Closed rational calculations only: no silently free symbols or guessed word-problem model.
    let Canon::Rational(number) = &value.canon else {
        return None;
    };
    Some(Calculation {
        expression: expression.to_owned(),
        answer: value.text,
        integer: number.is_integer(),
    })
}

pub(super) fn source(exemplar: &Exemplar) -> Option<Calculation> {
    let problem = exemplar.problem.trim();
    let rest = ["Compute ", "Calculate ", "Evaluate ", "Simplify "]
        .iter()
        .find_map(|prefix| problem.strip_prefix(prefix))
        .or_else(|| (problem.starts_with('$') && problem.ends_with('$')).then_some(problem))?;
    let expression = rest.trim_end_matches('.').trim().trim_matches('$').trim();
    let calculated = evaluate(expression)?;
    let canonical = cadus_core::answer::canonical_form(&calculated.answer).ok()?;
    if !same_answer(&canonical, &exemplar.canonical_answer().ok()?) {
        return None;
    }
    Some(calculated)
}

/// Change a whole-number token, preserving an integer result when the source had one.
/// Decimal components, scientific notation and symbolic identifiers are never rewritten.
type Variants = (usize, usize, Vec<(i64, Calculation)>);

pub(super) fn variants(source: &Calculation) -> Option<Variants> {
    let bytes = source.expression.as_bytes();
    let start = bytes.iter().position(u8::is_ascii_digit)?;
    let end = start
        + bytes[start..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
    if start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'.') {
        return None;
    }
    if end < bytes.len() && (bytes[end].is_ascii_alphabetic() || bytes[end] == b'.') {
        return None;
    }
    let original = source.expression[start..end].parse::<i64>().ok()?;
    if !(0..=1000).contains(&original) {
        return None;
    }
    let mut out = Vec::new();
    for offset in 1..=64 {
        let value = original + offset;
        let expression = format!(
            "{}{value}{}",
            &source.expression[..start],
            &source.expression[end..]
        );
        let Some(calculation) = evaluate(&expression) else {
            continue;
        };
        if calculation.integer != source.integer {
            continue;
        }
        out.push((value, calculation));
        if out.len() == 24 {
            break;
        }
    }
    (!out.is_empty()).then_some((start, end, out))
}

pub(super) fn escape(text: &str) -> String {
    text.replace('{', "{{").replace('}', "}}")
}
