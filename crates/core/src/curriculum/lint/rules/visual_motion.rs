//! The motion rules of the full lint (D-FA1).
//!
//! A figure with a learner-set parameter belongs to a teach page. A graded
//! exemplar never carries one, because a slider lets the learner read the
//! answer off the figure.
//!
//! - `visual_motion_on_exemplar`: an exemplar names a figure that has `motion`.
//! - `visual_motion_table`: the figure has `motion`, and the server cannot build
//!   its table (a step figure fails its checks, or the table is too large).

use super::super::super::finding::Finding;
use super::super::super::model::{KnowledgePoint, Topic};
use crate::visual::{RenderOptions, render_with_motion};

/// The findings of the slider figures of one knowledge point.
pub(super) fn check_point(topic: &Topic, file: &str, kp: &KnowledgePoint) -> Vec<Finding> {
    let mut found = Vec::new();
    for (index, spec) in kp.visuals.iter().enumerate() {
        if spec.motion().is_none() {
            continue;
        }
        let report = |code: &str, reason: String| {
            Finding::new(
                code,
                format!("{}/{} figure {index}: {reason}", topic.id, kp.id),
            )
            .with_topic(topic.id.as_str())
            .with_file(file)
            .with_context(vec![kp.id.as_str().to_owned(), index.to_string()])
        };
        if let Err(error) = render_with_motion(spec, &RenderOptions::default()) {
            found.push(report(
                "visual_motion_table",
                format!("the slider figure does not hold: {error}"),
            ));
        }
        let named = kp.exemplars.iter().enumerate();
        for (at, _) in named.filter(|(_, exemplar)| exemplar.visual == Some(index)) {
            found.push(report(
                "visual_motion_on_exemplar",
                format!(
                    "exemplar {at} names a figure with a slider, and a graded problem never \
                     shows a slider"
                ),
            ));
        }
    }
    found
}
