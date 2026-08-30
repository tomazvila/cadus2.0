//! The two authored instruction documents: the teach page (L4) and the hint
//! ladder (L5), with the gate that judges each one (A2, C6, R3).
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` sections 2.2 and 7 row
//! R6, and `docs/reference/web-service-1.0-spec.md:53` for the teach reply.
//!
//! # Why these documents have a gate at all
//!
//! 1.0 asked a model for a teach page and for every hint on the request path,
//! and its grade cache excluded both on purpose (`grade_cache.py:451-467`). So
//! 1.0 had no document to judge. 2.0 authors both offline, stores them in
//! `content_store`, and serves them with zero model tokens (L4, L5, T1). An
//! authored document that no machine read is a document a learner reads first.
//!
//! # What each gate refuses
//!
//! | Document | Rule | Why |
//! |---|---|---|
//! | teach | `concept` is a non-empty string | a page with no method states nothing |
//! | teach | `worked_example.problem` is a non-empty string | the concept alone is not instruction |
//! | teach | `worked_example.steps` is a list of at least one non-empty step | the worked solution IS the page (spec section 7, row R6) |
//! | teach | the worked problem is not an exemplar | A6 serves the exemplars, so an exemplar worked out hands the learner an answer before the attempt (Hard Rule 1) |
//! | hint | `hints` holds at least one rung | the hint route serves the rungs and nothing else |
//! | hint | no rung repeats an earlier rung | each rung goes one step past the one before it |
//! | hint | no rung names an answer the knowledge point serves | Hard Rule 3: a hint is a question, never the final step |
//! | both | no unknown field | the serve reader refuses one, so a stored body it cannot read serves nothing |
//!
//! # What "an answer the knowledge point serves" means
//!
//! One ladder serves every instance of one knowledge point, so the gate reads the
//! whole set of answers that knowledge point hands a learner, and not one
//! instance:
//!
//! - every exemplar answer — A6 serves the exemplars when no template is
//!   approved;
//! - every answer of an instance the approved templates render
//!   ([`InstructionSpec::instance_answers`], filled by the worker).
//!
//! The set carries no exemption. An earlier build skipped an exemplar whose own
//! problem showed its answer; a rendered instance is a different statement that
//! shows nothing, and the L5 route serves the stored rung with no re-check, so
//! the skip handed the answer to the learner (M6 review, findings F2, F15 and
//! F25). The template gate reads the same rule over the instances it renders
//! (`crate::template::gate`).
//!
//! # No panic, on any body
//!
//! Every step returns a [`Rejection`]. Nothing here indexes, unwraps, or divides
//! (C4). A body that is not a document at all is a rejection, not a crash.

use serde::{Deserialize, Serialize};

use crate::curriculum::Exemplar;
use crate::template::gate::{Rejection, contains_token, py_str};

/// The fields a teach body carries.
pub const TEACH_FIELDS: [&str; 2] = ["concept", "worked_example"];

/// The fields a worked example carries.
pub const WORKED_EXAMPLE_FIELDS: [&str; 2] = ["problem", "steps"];

/// The fields a hint body carries.
pub const HINT_FIELDS: [&str; 1] = ["hints"];

/// One fully worked example (`docs/reference/web-service-1.0-spec.md:53`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkedExample {
    /// The example problem. It is self-contained: it never names the served
    /// practice problem and it never reveals its answer (Hard Rule 1).
    pub problem: String,
    /// The solution steps, in order, ending with the answer.
    pub steps: Vec<String>,
}

/// The body of a `content_store` row of kind `teach` (L4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeachPage {
    /// The method or the rule, in one or two sentences.
    pub concept: String,
    /// The fully worked example.
    pub worked_example: WorkedExample,
}

/// The body of a `content_store` row of kind `hint_ladder` (L5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HintLadder {
    /// The rungs, from the widest nudge to the narrowest.
    pub hints: Vec<String>,
}

/// What one instruction gate knows about the knowledge point it judges for.
#[derive(Debug, Clone)]
pub struct InstructionSpec<'a> {
    /// The authored problems of the knowledge point. A6 serves these when no
    /// template is approved, so their answers are answers a learner sees.
    pub exemplars: &'a [Exemplar],
    /// The answers of the instances the approved templates of this knowledge
    /// point render. The serve path draws one instance per attempt, so every
    /// answer here is an answer a learner reads.
    ///
    /// The worker fills the list from the approved `template` documents of the
    /// knowledge point (`cadus_worker::authoring::job::served_answers`). An
    /// empty list is the honest value for a knowledge point with no approved
    /// template: A6 serves the exemplars there and nothing else.
    pub instance_answers: Vec<String>,
}

impl InstructionSpec<'_> {
    /// Every answer this knowledge point serves, in one list and in a fixed
    /// order: the exemplar answers first, then the instance answers.
    ///
    /// The list holds each answer once, and it holds no empty answer.
    #[must_use]
    pub fn served_answers(&self) -> Vec<&str> {
        let mut answers: Vec<&str> = Vec::new();
        let exemplars = self.exemplars.iter().map(|exemplar| &exemplar.answer);
        for answer in exemplars.chain(self.instance_answers.iter()) {
            let answer = answer.as_str();
            if answer.is_empty() || answers.contains(&answer) {
                continue;
            }
            answers.push(answer);
        }
        answers
    }
}

/// Read one JSON object, or refuse the body with the reader's own words.
fn object(body: &str, what: &str) -> Result<serde_json::Map<String, serde_json::Value>, Rejection> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|err| Rejection {
        code: "body",
        message: format!("the {what} body is not JSON: {err}"),
    })?;
    match value {
        serde_json::Value::Object(fields) => Ok(fields),
        _ => Err(Rejection {
            code: "body",
            message: format!("the {what} body is not a JSON object"),
        }),
    }
}

/// No field the reader does not know.
///
/// The serve reader carries `deny_unknown_fields`, so a stored body with a spare
/// field reads as a `500` at the route and teaches nobody. The gate says so
/// first, in words the next attempt acts on.
fn only_known(
    fields: &serde_json::Map<String, serde_json::Value>,
    known: &[&str],
    what: &str,
    code: &'static str,
) -> Result<(), Rejection> {
    for name in fields.keys() {
        if !known.contains(&name.as_str()) {
            return Err(Rejection {
                code,
                message: format!(
                    "the {what} carries the unknown field {} — it holds {} and nothing else",
                    py_str(name),
                    py_names(known)
                ),
            });
        }
    }
    Ok(())
}

/// Write a field list the way the messages read it.
fn py_names(names: &[&str]) -> String {
    let written: Vec<String> = names.iter().map(|name| py_str(name)).collect();
    written.join(" and ")
}

/// One non-empty string field, or the refusal it earns.
fn text(
    fields: &serde_json::Map<String, serde_json::Value>,
    name: &str,
    code: &'static str,
    reason: &str,
) -> Result<String, Rejection> {
    match fields.get(name).and_then(serde_json::Value::as_str) {
        Some(value) if !value.trim().is_empty() => Ok(value.to_owned()),
        _ => Err(Rejection {
            code,
            message: reason.to_owned(),
        }),
    }
}

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
    let mut written = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        match step.as_str() {
            Some(value) if !value.trim().is_empty() => written.push(value.to_owned()),
            _ => {
                return Err(Rejection {
                    code: "teach-steps",
                    message: format!(
                        "step {index} of 'worked_example.steps' is not a non-empty string — every \
step is one line of the solution a learner reads"
                    ),
                });
            }
        }
    }
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
    Ok(TeachPage {
        concept,
        worked_example: WorkedExample {
            problem,
            steps: written,
        },
    })
}

/// The gate of a hint ladder (L5, spec section 7 row R6).
///
/// # Errors
///
/// Returns the [`Rejection`] of the first rule the body breaks. The message is
/// the literal text the next authoring attempt reads.
pub fn gate_hint_ladder(body: &str, spec: &InstructionSpec<'_>) -> Result<HintLadder, Rejection> {
    let fields = object(body, "hint")?;
    only_known(&fields, &HINT_FIELDS, "hint ladder", "hint-unknown-field")?;
    let rungs = fields
        .get("hints")
        .and_then(serde_json::Value::as_array)
        .ok_or(Rejection {
            code: "hint-missing",
            message: "a hint ladder needs a 'hints' list, widest rung first".to_owned(),
        })?;
    if rungs.is_empty() {
        return Err(Rejection {
            code: "hint-missing",
            message: "a hint ladder needs at least one rung, and a hint may never give the answer \
away"
                .to_owned(),
        });
    }
    let mut hints: Vec<String> = Vec::with_capacity(rungs.len());
    for (index, rung) in rungs.iter().enumerate() {
        let text = match rung.as_str() {
            Some(value) if !value.trim().is_empty() => value.to_owned(),
            _ => {
                return Err(Rejection {
                    code: "hint-rung",
                    message: format!(
                        "rung {index} is not a non-empty string — every rung is one nudge a \
learner reads"
                    ),
                });
            }
        };
        if let Some(earlier) = hints.iter().position(|seen| seen.trim() == text.trim()) {
            return Err(Rejection {
                code: "hint-repeat",
                message: format!(
                    "rung {index} repeats rung {earlier} — every rung goes one small step past \
the one before it, and a repeated rung leaves the learner exactly as stuck"
                ),
            });
        }
        hints.push(text);
    }
    check_no_answer(&hints, spec)?;
    Ok(HintLadder { hints })
}

/// No rung names an answer this knowledge point serves.
///
/// Hard Rule 3 (`docs/WEB_SERVICE.md:22-32`). The answer set is
/// [`InstructionSpec::served_answers`]: every exemplar answer and every answer
/// of an instance the approved templates render. There is no exemption.
///
/// # Why the "the problem shows it already" exemption is gone
///
/// The gate used to skip an exemplar whose own problem carried its answer, on
/// the reading that a learner already reads the token in the problem. One ladder
/// serves EVERY instance of the knowledge point, and a rendered template instance
/// is a different problem with a different statement, so the token the exemplar
/// showed is a token the served instance hides. 307 shipped knowledge points took
/// that exemption, and 62 of them took it on every exemplar they hold, so a
/// ladder that stated every answer passed the whole gate (M6 review, findings F2,
/// F15 and F25; `crates/core/tests/instruction_gate.rs` pins both counts). The
/// serve path re-reads nothing: the L5 route returns the stored rung as it
/// stands.
fn check_no_answer(hints: &[String], spec: &InstructionSpec<'_>) -> Result<(), Rejection> {
    let served = spec.served_answers();
    for (index, rung) in hints.iter().enumerate() {
        for answer in &served {
            if contains_token(rung, answer) {
                return Err(Rejection {
                    code: "hint-answer",
                    message: format!(
                        "rung {index} reads {}, which names the answer {} this knowledge point \
serves — a hint is a question, never the final step (Hard Rule 3)",
                        py_str(rung),
                        py_str(answer)
                    ),
                });
            }
        }
    }
    Ok(())
}
