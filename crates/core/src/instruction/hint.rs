//! The gate of a hint ladder (L5, spec section 7 row R6).

use crate::template::gate::{Rejection, contains_token, py_str};

use super::body::{object, only_known};
use super::{HINT_FIELDS, HintLadder, InstructionSpec};

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
