//! Deterministic diagnosis evidence from already-gated template distractors.

use crate::authoring::{
    job::verify_kind,
    prompt::{AuthoringSpec, Kind},
};
use cadus_core::template::{answer, from_body, parse_answer_expr, render};
use serde_json::{Value, json};

/// Build one pending diagnosis draft from an explicit wrong-answer derivation.
///
/// A result exists only when a stored template names the wrong expression, its
/// controlled error tag, and its learner-facing note. The production diagnosis
/// gate then proves the concrete wrong answer is decidable and is not correct.
#[must_use]
pub fn diagnosis_from_templates(spec: &AuthoringSpec, bodies: &[String]) -> Option<Value> {
    for body in bodies {
        let Ok(doc) = from_body(body) else {
            continue;
        };
        for sample in &doc.samples {
            let bindings = sample.bindings();
            for distractor in &doc.distractors {
                let Some(note) = distractor.note.as_deref() else {
                    continue;
                };
                let Ok(ast) = parse_answer_expr(&distractor.answer) else {
                    continue;
                };
                let (Ok(wrong), Ok(note)) = (answer(&ast, &bindings), render(note, &bindings))
                else {
                    continue;
                };
                let arguments = json!({"distractors":[{
                    "answer":wrong.text,
                    "error_tag":distractor.error_tag,
                    "note":note,
                }]});
                if verify_kind(Kind::Diagnosis, spec, &arguments, &[]).is_ok() {
                    return Some(json!({
                        "kp_id":format!("{}/{}", spec.topic_id, spec.kp_id),
                        "kind":"diagnosis",
                        "arguments":arguments,
                    }));
                }
            }
        }
    }
    None
}
