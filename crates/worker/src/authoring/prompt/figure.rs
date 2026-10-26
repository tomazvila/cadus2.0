//! The reference figures that the teach prompt offers to the model.
//!
//! A teach page may name one figure of its knowledge point by index. An
//! exemplar figure is never offered: it draws a problem that the server serves.
//! A reference figure is a figure that no exemplar names.
use super::{AuthoringSpec, Kind};

/// Add the reference figures to the lines of a teach message. Any other kind,
/// and a knowledge point with no reference figure, leave the lines as they are.
pub(super) fn offer_reference_figures(kind: Kind, spec: &AuthoringSpec, lines: &mut Vec<String>) {
    if kind == Kind::Teach
        && let Some(figures) = render_reference_figures(spec)
    {
        lines.push(String::new());
        lines.push(figures);
    }
}

/// The list of reference figures for the user message, or [`None`] when the
/// knowledge point holds none. A [`None`] leaves the message as it was.
#[must_use]
pub(super) fn render_reference_figures(spec: &AuthoringSpec) -> Option<String> {
    let entries: Vec<String> = spec
        .visuals
        .iter()
        .enumerate()
        .filter(|(index, figure)| {
            figure.validate().is_ok()
                && !spec
                    .exemplars
                    .iter()
                    .any(|exemplar| exemplar.visual == Some(*index))
        })
        .map(|(index, figure)| {
            format!(
                "  Figure {index} ({}): {}\n    Text description: {}",
                figure.kind(),
                figure.caption().unwrap_or("no caption"),
                figure.text_equivalent()
            )
        })
        .collect();
    if entries.is_empty() {
        return None;
    }
    let mut lines = vec![
        "Reference figures for this knowledge point. If one fits, set 'figure' to its number and \
write the worked example about it, with its labels and its numbers:"
            .to_owned(),
    ];
    lines.extend(entries);
    Some(lines.join("\n"))
}
