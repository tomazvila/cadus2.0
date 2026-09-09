//! The gate of a teach page (L4, spec section 7 row R6).
use crate::template::gate::{Rejection, contains_token, py_str};

use super::body::{object, only_known, text};
use super::{InstructionSpec, TEACH_FIELDS, TeachPage, WORKED_EXAMPLE_FIELDS, WorkedExample};

/// The gate of a teach page (L4, spec section 7 row R6).
///
/// # Errors
///
/// Returns the [`Rejection`] of the first rule the body breaks. The message is
/// the literal text the next authoring attempt reads.
pub fn gate_teach(body: &str, spec: &InstructionSpec<'_>) -> Result<TeachPage, Rejection> {
    let fields = object(body, "teach")?;
    only_known(&fields, &TEACH_FIELDS, "teach page", "teach-unknown-field")?;
    let concept = text(
        &fields,
        "concept",
        "teach-concept",
        "a teach page needs a non-empty 'concept': one or two plain sentences that state the \
method or the rule, because the learner may never have seen this material",
    )?;
    let example = fields
        .get("worked_example")
        .and_then(serde_json::Value::as_object)
        .ok_or(Rejection {
            code: "teach-worked-example",
            message: "a teach page needs a 'worked_example' object with 'problem' and 'steps'"
                .to_owned(),
        })?;
    only_known(
        example,
        &WORKED_EXAMPLE_FIELDS,
        "worked example",
        "teach-unknown-field",
    )?;
    let problem = text(
        example,
        "problem",
        "teach-problem",
        "a teach page needs a non-empty 'worked_example.problem': the concept states the method, \
and the worked problem is where the learner sees it done",
    )?;
    let steps = example
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .ok_or(Rejection {
            code: "teach-steps",
            message: "a teach page needs 'worked_example.steps': the complete solution, one step \
per entry, ending with the final answer — a concept with no worked solution teaches nothing"
                .to_owned(),
        })?;
    if steps.is_empty() {
        return Err(Rejection {
            code: "teach-steps",
            message: "'worked_example.steps' is empty: the complete solution goes here, one step \
per entry, ending with the final answer"
                .to_owned(),
        });
    }
    let written = steps
        .iter()
        .enumerate()
        .map(|(index, step)| step_text(index, step))
        .collect::<Result<Vec<String>, Rejection>>()?;
    for (index, exemplar) in spec.exemplars.iter().enumerate() {
        if exemplar.problem.trim() == problem.trim() {
            return Err(Rejection {
                code: "teach-worked-example",
                message: format!(
                    "'worked_example.problem' reads {}, which is exemplar {index} — the server \
serves the exemplars, so work the method on DIFFERENT values, or the page answers a problem the \
learner has not attempted yet (Hard Rule 1)",
                    py_str(&problem)
                ),
            });
        }
    }
    check_teach_disclosures(&problem, &written, spec)?;
    Ok(TeachPage {
        concept,
        worked_example: WorkedExample {
            problem,
            steps: written,
        },
    })
}
/// One step of the worked solution, or the refusal an empty step earns.
fn step_text(index: usize, step: &serde_json::Value) -> Result<String, Rejection> {
    match step.as_str() {
        Some(value) if !value.trim().is_empty() => Ok(value.to_owned()),
        _ => Err(Rejection {
            code: "teach-steps",
            message: format!(
                "step {index} of 'worked_example.steps' is not a non-empty string — every \
step is one line of the solution a learner reads"
            ),
        }),
    }
}

fn normalized_problem(value: &str) -> String {
    let mut text = value.trim().trim_end_matches(['.', '!', '?']).trim();
    for (open, close) in [("$", "$"), ("\\(", "\\)"), ("\\[", "\\]")] {
        if text.len() >= open.len() + close.len() && text.starts_with(open) && text.ends_with(close)
        {
            text = &text[open.len()..text.len() - close.len()];
        }
    }
    text.trim_end_matches(['.', '!', '?'])
        .replace("\\div", "/")
        .replace("\\times", "*")
        .replace("\\cdot", "*")
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .map(|ch| match ch as u32 {
            0x00F7 => '/',
            0x00D7 | 0x00B7 => '*',
            _ => ch,
        })
        .collect()
}
fn contains_expression(text: &str, expression: &str) -> bool {
    text.match_indices(expression).any(|(index, _)| {
        let before = text[..index].chars().next_back();
        let after = text[index + expression.len()..].chars().next();
        !before.is_some_and(|ch| ch.is_ascii_alphanumeric())
            && !after.is_some_and(|ch| ch.is_ascii_alphanumeric())
    })
}

fn direct_expression(problem: &str) -> Option<String> {
    ["Compute ", "Calculate ", "Evaluate ", "Simplify "]
        .into_iter()
        .find_map(|prefix| problem.trim().strip_prefix(prefix))
        .map(normalized_problem)
        .filter(|expression| expression.len() > 2)
}

fn same_problem(left: &str, right: &str) -> bool {
    normalized_problem(left) == normalized_problem(right)
        || direct_expression(left)
            .zip(direct_expression(right))
            .is_some_and(|(left, right)| left == right)
}

fn check_teach_disclosures(
    problem: &str,
    steps: &[String],
    spec: &InstructionSpec<'_>,
) -> Result<(), Rejection> {
    let last = steps.last().map_or("", String::as_str);
    let normalized_last = normalized_problem(last);
    for (served_problem, answer) in spec.served() {
        if same_problem(served_problem, problem) {
            return Err(Rejection {
                code: "teach-worked-example",
                message: "the worked example repeats a served problem; use different operands before the learner attempts it".to_owned(),
            });
        }
        if let Some(expression) = direct_expression(served_problem) {
            if contains_expression(&normalized_last, &expression)
                && !answer.is_empty()
                && contains_token(last, answer)
            {
                return Err(Rejection {
                    code: "teach-answer",
                    message: format!(
                        "the final step solves served problem {} and names its answer {} (Hard Rule 1)",
                        py_str(served_problem),
                        py_str(answer)
                    ),
                });
            }
        }
    }
    Ok(())
}
