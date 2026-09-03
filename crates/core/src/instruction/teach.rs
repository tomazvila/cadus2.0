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

/// The last step names no answer of a served problem OTHER than the worked one.
///
/// Hard Rule 1. The worked example ends with its own answer, so a rule that
/// refused every served answer refused every worked example a knowledge point
/// with a template holds. The rule is therefore the PAIR rule: the gate reads
/// each served problem beside its answer, it skips the served problem the page
/// works, and it refuses the last step that names any other served answer.
///
/// The check reads the LAST step alone. That step is the answer of the page, and
/// a second answer stated there is a second answer handed over. An earlier step
/// carries the method, and a numeral inside it is arithmetic on the way to the
/// answer.
///
/// The teach half of finding F15 stood open until this rule: `verify_teach`
/// filled [`InstructionSpec::instance_answers`] and `gate_teach` read the
/// exemplar problems alone, so no test and no mutation separated a build that
/// carried the field from a build that dropped it (M6 review 2, findings V2 and
/// V11).
fn check_no_other_answer(
    problem: &str,
    last: &str,
    spec: &InstructionSpec<'_>,
) -> Result<(), Rejection> {
    for (served_problem, answer) in spec.served() {
        if answer.is_empty() || served_problem.trim() == problem.trim() {
            continue;
        }
        if contains_token(last, answer) {
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
