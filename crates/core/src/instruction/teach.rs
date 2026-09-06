//! The gate of a teach page (L4, spec section 7 row R6).

use crate::answer::{canonical_form, same_answer};
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
    check_no_other_answer(&problem, written.last().map_or("", String::as_str), spec)?;
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

/// Refuse a served problem identity and additional answers from other problems.
/// For a direct calculation, derive its own result with the deterministic checker.
/// Equal results from different problems are ordinary arithmetic coincidences.
/// Unsupported word problems retain the conservative answer check.
fn check_no_other_answer(
    problem: &str,
    last: &str,
    spec: &InstructionSpec<'_>,
) -> Result<(), Rejection> {
    let own_answer = compute_answer(problem);
    for (served_problem, answer) in spec.served() {
        if served_problem.trim() == problem.trim() {
            return Err(Rejection {
                code: "teach-worked-example",
                message: "the worked example repeats a served problem; use different operands before the learner attempts it".to_owned(),
            });
        }
        if answer.is_empty() {
            continue;
        }
        let coincides = own_answer
            .as_ref()
            .zip(canonical_form(answer).ok().as_ref())
            .is_some_and(|(own, served)| same_answer(own, served));
        if !coincides && contains_token(last, answer) {
            return Err(Rejection {
                code: "teach-answer",
                message: format!(
                    "the last step of 'worked_example.steps' reads {}, which names {}, the answer \
of {} — this knowledge point serves that problem too, and the page works {}, so the step hands the \
learner an answer before the attempt (Hard Rule 1)",
                    py_str(last),
                    py_str(answer),
                    py_str(served_problem),
                    py_str(problem)
                ),
            });
        }
    }
    Ok(())
}

/// Derive the result of a direct calculation without a language-model guess.
fn compute_answer(problem: &str) -> Option<crate::answer::Canon> {
    let expression = ["Compute ", "Calculate ", "Evaluate ", "Simplify "]
        .into_iter()
        .find_map(|prefix| problem.trim().strip_prefix(prefix))?;
    canonical_form(expression.trim().trim_end_matches('.').trim()).ok()
}
