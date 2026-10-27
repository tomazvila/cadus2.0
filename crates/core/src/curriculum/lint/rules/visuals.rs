//! The figure rules of the full lint: a figure belongs to the item that names it.
//!
//! A knowledge point lists its figures in `visuals`. An exemplar shows one only
//! when it carries `visual: <index>`. These rules find the three ways an item and
//! its figure disagree:
//!
//! - `visual_mismatch`: the figure names a label or a value that the item never
//!   states in its problem, answer or solution sketch.
//! - `visual_letter_clash`: a figure label reuses a letter that the problem
//!   already uses as a quantity (`A` for an area, `h` for a height).
//! - `visual_missing`: the problem points at "the diagram" or "the figure", but
//!   the item names no figure.
//!
//! Every finding carries the context `[kp, exemplar index, visual index or
//! "none"]` so a sweep can list the items in a table.

use super::super::super::finding::Finding;
use super::super::super::model::{Exemplar, KnowledgePoint};
use super::super::Lint;
use crate::visual::{absent_terms, clashing_labels};

/// Phrases that point a learner at a drawing. Matched on lowercase text.
const POINTING: &[&str] = &[
    "diagram shows",
    "figure shows",
    "in the figure",
    "in the diagram",
    "in the picture",
    "pictured",
    "number line shown",
    "graph shown",
    "shown above",
    "shown below",
    "drawn above",
    "drawn below",
];

pub(in crate::curriculum::lint) fn check_visuals(lint: &mut Lint<'_>) {
    for position in 0..lint.table.topics.len() {
        let topic = lint.table.topics[position];
        let file = lint.table.file(position).to_owned();
        for kp in &topic.knowledge_points {
            lint.findings
                .extend(super::visual_motion::check_point(topic, &file, kp));
            for (at, exemplar) in kp.exemplars.iter().enumerate() {
                check_item(
                    &mut lint.findings,
                    (topic.id.as_str(), &file),
                    kp.id.as_str(),
                    Some(kp),
                    at,
                    exemplar,
                );
            }
        }
        if let Some(dx) = &topic.diagnostic_exemplar {
            check_item(
                &mut lint.findings,
                (topic.id.as_str(), &file),
                "diagnostic",
                None,
                0,
                dx,
            );
        }
    }
}

fn check_item(
    findings: &mut Vec<Finding>,
    (topic_id, file): (&str, &str),
    kp_id: &str,
    kp: Option<&KnowledgePoint>,
    at: usize,
    exemplar: &Exemplar,
) {
    let named = match (exemplar.visual, kp) {
        (Some(index), Some(kp)) => kp.visuals.get(index).map(|spec| (index, spec)),
        _ => None,
    };
    let context = vec![
        kp_id.to_owned(),
        at.to_string(),
        exemplar
            .visual
            .map_or_else(|| "none".to_owned(), |index| index.to_string()),
    ];
    let mut report = |code: &str, reason: String| {
        findings.push(
            Finding::new(code, format!("{topic_id}/{kp_id} exemplar {at}: {reason}"))
                .with_topic(topic_id)
                .with_file(file)
                .with_context(context.clone()),
        );
    };
    let Some((_, spec)) = named else {
        if exemplar.visual.is_none() && points_at_a_drawing(&exemplar.problem) {
            report(
                "visual_missing",
                "the problem points at a drawing, but the item names no visual".to_owned(),
            );
        }
        if exemplar.visual.is_some() && kp.is_none() {
            report(
                "visual_missing",
                "the diagnostic exemplar names a visual, but it has no knowledge point".to_owned(),
            );
        }
        return;
    };
    let text = format!(
        "{} {} {}",
        exemplar.problem,
        exemplar.answer,
        exemplar.solution_sketch.as_deref().unwrap_or("")
    );
    let absent = absent_terms(spec, &text);
    if !absent.is_empty() {
        report(
            "visual_mismatch",
            format!(
                "the figure names {} that the item never states",
                absent.join(", ")
            ),
        );
    }
    for letter in clashing_labels(spec, &exemplar.problem) {
        report(
            "visual_letter_clash",
            format!("the figure label {letter} is also a quantity in the problem"),
        );
    }
}

fn points_at_a_drawing(problem: &str) -> bool {
    let lower = problem.to_lowercase();
    POINTING.iter().any(|phrase| lower.contains(phrase))
}
