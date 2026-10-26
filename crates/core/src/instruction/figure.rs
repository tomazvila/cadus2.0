//! The figure of a teach page: one index into the `visuals` of the knowledge point.
//!
//! The checks run in a fixed order. The first failure is the message that the
//! next authoring attempt reads.
use crate::curriculum::FiniteObjectiveCase;
use crate::template::gate::Rejection;
use crate::visual::{absent_terms, clashing_labels};

use super::InstructionSpec;
use super::teach::check_teach_disclosures;

fn refuse(message: String) -> Rejection {
    Rejection {
        code: "teach-figure",
        message,
    }
}

/// Read and check the optional `figure` field of a teach body.
///
/// # Errors
///
/// Returns a `teach-figure` [`Rejection`] for the first rule the figure breaks.
pub(super) fn read_figure(
    fields: &serde_json::Map<String, serde_json::Value>,
    problem: &str,
    steps: &[String],
    spec: &InstructionSpec<'_>,
    taught: Option<&FiniteObjectiveCase>,
) -> Result<Option<usize>, Rejection> {
    let Some(value) = fields.get("figure") else {
        return Ok(None);
    };
    let index = value
        .as_u64()
        .and_then(|number| usize::try_from(number).ok())
        .ok_or_else(|| {
            refuse(format!(
                "'figure' is {value}, which is not a whole number of 0 or more — write the number \
of one figure from the list, or leave 'figure' out"
            ))
        })?;
    let Some(figure) = spec.visuals.get(index) else {
        return Err(refuse(format!(
            "'figure' is {index}, but this knowledge point has {} figures — the numbers run from 0 \
to {}; write one of them, or leave 'figure' out",
            spec.visuals.len(),
            spec.visuals.len().saturating_sub(1)
        )));
    };
    if let Err(error) = figure.validate() {
        return Err(refuse(format!(
            "figure {index} does not hold: {error} — leave 'figure' out"
        )));
    }
    if spec
        .exemplars
        .iter()
        .any(|exemplar| exemplar.visual == Some(index))
    {
        return Err(refuse(format!(
            "figure {index} draws a problem that the server serves, so the page would show the \
learner that problem before the learner tries it (Hard Rule 1) — write the number of a reference \
figure, or leave 'figure' out"
        )));
    }
    let worked = format!("{problem} {}", steps.join(" "));
    let absent = absent_terms(figure, &worked);
    if !absent.is_empty() {
        return Err(refuse(format!(
            "figure {index} shows {}, which the worked example never states — write the worked \
example about the figure and use its labels and its numbers, or leave 'figure' out",
            absent.join(", ")
        )));
    }
    let clashing = clashing_labels(figure, problem);
    if !clashing.is_empty() {
        return Err(refuse(format!(
            "the figure label {} is also a quantity in the worked problem — use other letters in \
the worked problem, or leave 'figure' out",
            clashing.join(", ")
        )));
    }
    let shown = format!(
        "{} {}",
        figure.caption().unwrap_or(""),
        figure.text_equivalent()
    );
    check_teach_disclosures(problem, &[shown], spec, taught).map_err(|rejection| {
        refuse(format!(
            "the caption or the text description of figure {index} gives away a served problem: {} \
— leave 'figure' out",
            rejection.message
        ))
    })?;
    Ok(Some(index))
}
