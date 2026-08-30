//! The pre-authored diagnosis of A4: the `diagnosis` document and its gate.
//!
//! Requirements: A4 (a wrong answer the bank names is diagnosed with no model
//! call), A2 (the machine gate), C6 (a human approves the digest), T1 (no model
//! token is spent here), C4 (no step panics on any document).
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` section 2.2 step 1 and
//! row R7 of section 7; `docs/reference/web-service-1.0-spec.md` section 5.3
//! (the error-tag vocabulary) and section 6.2 (what skips the call).
//!
//! # Two documents carry distractors, and they are not the same document
//!
//! A [`TemplateDoc`](super::TemplateDoc) carries `distractors` whose `answer` is
//! an EXPRESSION over the declared parameters, and the M4 gate verifies it
//! against the worked samples. This module owns the OTHER one: the
//! `content_store` row of `kind = 'diagnosis'`, which a knowledge point that
//! serves authored exemplars gets, because it has no parameters and no template
//! (spec section 2.2, "Bank target"). Its distractor answers are literal wrong
//! answers, and the grade path reads them (spec section 6.2).
//!
//! # The vocabulary filter
//!
//! An `error_tag` outside the controlled vocabulary of spec section 5.3 is
//! DROPPED, exactly as 1.0's `coerce_error_tags` drops it
//! (`prompts.py:1133-1142`). [`keep_known_tags`] drops the whole distractor,
//! because one distractor carries one tag: a distractor with the tag removed
//! carries an empty tag, which [`gate_diagnosis`] refuses. The authoring prompt
//! states the rule to the model ("A tag outside it is dropped"), so the drop is
//! the documented answer and not a surprise.
//!
//! The filter runs on the RAW body, before the gate reads it, so the document
//! the gate accepts is the document the row stores. [`keep_known_tags`] gives
//! the reason in full.
//!
//! # A note is served verbatim
//!
//! The note of a `diagnosis` document reaches the learner as prose. Nothing
//! renders it through the `{name}` placeholder grammar, so its braces are its
//! own and `$\frac{1}{2}$` reaches the learner the way it stands. That is the
//! one rule this gate does NOT share with the template gate, which doubles every
//! literal brace because [`render`](super::render) reads that field.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::answer::{Canon, Outcome, canonical_form, check, same_answer};
use crate::curriculum::AnswerKind;

use super::document::Distractor;
use super::gate::{GateSpec, Rejection, TEMPLATABLE_KINDS, py_list, py_str};

/// The version of the 2.0 diagnosis document.
///
/// It is [`TEMPLATE_VERSION`](super::TEMPLATE_VERSION): one pipeline authors
/// both documents and one table stores both, so one number retires both.
pub const DIAGNOSIS_VERSION: u32 = super::document::TEMPLATE_VERSION;

/// The `content_store.kind` of an authored distractor list (A4).
pub const KIND_DIAGNOSIS: &str = "diagnosis";

/// The authored distractor list of one knowledge point (`content_store.body`,
/// `kind = 'diagnosis'`).
///
/// The body holds the document ONLY. The knowledge point and the approval state
/// are columns of the row, and a second copy of either is a second source of
/// truth (C6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosisDoc {
    /// The document version. [`DIAGNOSIS_VERSION`] for a current document.
    pub v: u32,
    /// The topic whose exemplars the distractors diagnose.
    pub topic_id: String,
    /// The answer grammar the checker reads the answers in.
    pub answer_kind: AnswerKind,
    /// The wrong answers, each with the mistake it names.
    pub distractors: Vec<Distractor>,
}

/// The lenient view of any stored body that carries distractors.
#[derive(Debug, Default, Deserialize)]
struct AnyDistractors {
    #[serde(default)]
    distractors: Vec<Distractor>,
}

/// Read the distractors of one stored body, whatever else the body holds.
///
/// The reader takes the `distractors` list and nothing else, so one reader
/// serves a dedicated diagnosis row and a template document stored under the
/// same kind. A body with no list, and a body whose list does not read, both
/// give the empty list, which matches nothing.
#[must_use]
pub fn read_distractors(body: &Value) -> Vec<Distractor> {
    serde_json::from_value::<AnyDistractors>(body.clone())
        .unwrap_or_default()
        .distractors
}

/// The pre-authored diagnosis of one wrong answer (spec section 6.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preauthored {
    /// The authored tag, after the section 5.3 vocabulary filter.
    pub error_tags: Vec<String>,
    /// The authored prose the learner reads.
    pub prose: Option<String>,
}

/// Find the authored distractor that names this answer (spec section 6.2).
///
/// The match runs through [`check`], so `12` and `12.0` name the same mistake,
/// exactly as they name the same right answer. A distractor the checker cannot
/// decide never matches, so a template document read under this kind is safe.
///
/// The authored tag goes through the vocabulary of spec section 5.3 first: a tag
/// the vocabulary lacks is dropped. A hit with neither a surviving tag nor prose
/// carries nothing a learner reads, so it is NOT a hit and the caller enqueues.
#[must_use]
pub fn match_answer(
    distractors: &[Distractor],
    answer: &str,
    kind: AnswerKind,
    vocabulary: &[String],
) -> Option<Preauthored> {
    let hit = distractors.iter().find(|distractor| {
        matches!(
            check(&distractor.answer, answer, kind),
            Outcome::Decided(verdict) if verdict.correct
        )
    })?;
    let error_tags: Vec<String> = vocabulary
        .iter()
        .filter(|tag| *tag == &hit.error_tag)
        .cloned()
        .collect();
    let prose = hit.note.clone().filter(|note| !note.trim().is_empty());
    if error_tags.is_empty() && prose.is_none() {
        return None;
    }
    Some(Preauthored { error_tags, prose })
}

/// The field both authored documents keep their distractors in.
const FIELD_DISTRACTORS: &str = "distractors";

/// The field that names the mistake one distractor makes.
const FIELD_ERROR_TAG: &str = "error_tag";

/// Drop every distractor whose `error_tag` is outside the vocabulary.
///
/// The filter reads the RAW body and one field of it, so one rule holds for the
/// diagnosis document and for the template document alike. A body that carries
/// no `distractors` array loses nothing.
///
/// The answer is the dropped tags, in the order they stood, so the caller
/// records what the document lost.
///
/// # The filter runs BEFORE the gate, and the order is load bearing
///
/// A distractor note is a rendered field: [`gate`](super::gate) reads the
/// placeholders of a note and counts each one as a use of that parameter. A drop
/// AFTER the gate therefore stores a document the gate refuses, because the
/// parameter whose only use stood in the dropped note is now dead. The drop runs
/// first, so the document the gate accepts is the document the row stores, and a
/// document the drop breaks earns the dead-parameter message the next attempt
/// reads.
///
/// A distractor whose `error_tag` is absent, is not a string, or is blank stays
/// here. The gate names it ("distractor N carries no error_tag"), and a filter
/// that swallows it turns that refusal into a silent drop.
pub fn keep_known_tags(body: &mut Value, vocabulary: &[String]) -> Vec<String> {
    let mut dropped: Vec<String> = Vec::new();
    let Some(list) = body
        .get_mut(FIELD_DISTRACTORS)
        .and_then(Value::as_array_mut)
    else {
        return dropped;
    };
    list.retain(|entry| {
        let Some(tag) = entry.get(FIELD_ERROR_TAG).and_then(Value::as_str) else {
            return true;
        };
        if tag.trim().is_empty() {
            return true;
        }
        let known = vocabulary.iter().any(|name| name == tag);
        if !known {
            dropped.push(tag.to_owned());
        }
        known
    });
    dropped
}

/// Read one diagnosis body, verify it, and drop the tags outside the vocabulary.
///
/// The answer is the FILTERED document — the body the row stores and the digest
/// covers — and the tags the filter dropped.
///
/// # Errors
///
/// Returns the [`Rejection`] of a body that does not read as a diagnosis
/// document, of a list the filter empties, and of every check
/// [`gate_diagnosis`] runs.
pub fn gate_diagnosis_body(
    body: &str,
    spec: &GateSpec<'_>,
    vocabulary: &[String],
) -> Result<(DiagnosisDoc, Vec<String>), Rejection> {
    let unread = |err: &serde_json::Error| Rejection {
        code: "diagnosis-body",
        message: format!("the distractor list does not read as a diagnosis document: {err}"),
    };
    let mut raw: Value = serde_json::from_str(body).map_err(|err| unread(&err))?;
    // The drop runs first: the gate verifies the document the row stores.
    let dropped = keep_known_tags(&mut raw, vocabulary);
    let doc: DiagnosisDoc = serde_json::from_value(raw).map_err(|err| unread(&err))?;
    if doc.distractors.is_empty() && !dropped.is_empty() {
        return Err(Rejection {
            code: "distractor-vocabulary",
            message: format!(
                "no distractor carries an error_tag from the vocabulary {} — a tag outside it is dropped, so the list diagnoses nothing",
                py_list(vocabulary)
            ),
        });
    }
    gate_diagnosis(&doc, spec)?;
    Ok((doc, dropped))
}

/// Write the document the gate accepted, as the row stores it.
///
/// # Errors
///
/// Returns the `serde_json` error when the document does not write.
pub fn to_diagnosis_body(doc: &DiagnosisDoc) -> Result<String, serde_json::Error> {
    serde_json::to_string(doc)
}

/// Verify one diagnosis document (A2).
///
/// The checks run in this order, and each one writes the sentence the next
/// authoring attempt reads (spec section 2.2, step 3):
///
/// 1. the version is the version this server reads;
/// 2. the answer kind is symbolically decidable, or nothing ever matches;
/// 3. the list holds at least one distractor;
/// 4. every distractor carries an `error_tag` and a note;
/// 5. every answer is inside the decidable grammar;
/// 6. no answer is the right answer of an exemplar;
/// 7. no two distractors name one answer.
///
/// # Errors
///
/// Returns the [`Rejection`] of the first check that refuses the document.
pub fn gate_diagnosis(doc: &DiagnosisDoc, spec: &GateSpec<'_>) -> Result<(), Rejection> {
    if doc.v != DIAGNOSIS_VERSION {
        return Err(Rejection {
            code: "diagnosis-version",
            message: format!(
                "a diagnosis document of version {} is not the version this server reads ({DIAGNOSIS_VERSION})",
                doc.v
            ),
        });
    }
    if !TEMPLATABLE_KINDS.contains(&doc.answer_kind) {
        return Err(Rejection {
            code: "kind",
            message: format!(
                "answer kind {} is not symbolically decidable",
                doc.answer_kind
            ),
        });
    }
    if doc.distractors.is_empty() {
        return Err(Rejection {
            code: "distractor-missing",
            message: "a diagnosis document needs at least one distractor: the wrong answer a real mistake produces, the tag it carries, and the note the learner reads"
                .to_owned(),
        });
    }

    let mut seen: Vec<(usize, Canon)> = Vec::new();
    for (index, distractor) in doc.distractors.iter().enumerate() {
        if distractor.error_tag.trim().is_empty() {
            return Err(Rejection {
                code: "distractor",
                message: format!("distractor {index} carries no error_tag"),
            });
        }
        if distractor
            .note
            .as_ref()
            .is_none_or(|note| note.trim().is_empty())
        {
            return Err(Rejection {
                code: "distractor-note",
                message: format!(
                    "distractor {index} carries no note — the note is the diagnosis the learner reads, and a tag alone explains nothing"
                ),
            });
        }
        let canon = canonical_form(&distractor.answer).map_err(|reason| Rejection {
            code: "distractor",
            message: format!(
                "distractor {index} answers {}, which is outside the decidable grammar: {}",
                py_str(&distractor.answer),
                reason.reason
            ),
        })?;
        for (position, exemplar) in spec.exemplars.iter().enumerate() {
            let Ok(right) = canonical_form(&exemplar.answer) else {
                continue;
            };
            if same_answer(&right, &canon) {
                return Err(Rejection {
                    code: "distractor",
                    message: format!(
                        "distractor {index} answers {}, which is the right answer of exemplar {position} — a distractor names a mistake",
                        py_str(&distractor.answer)
                    ),
                });
            }
        }
        for (earlier, other) in &seen {
            if same_answer(other, &canon) {
                return Err(Rejection {
                    code: "distractor",
                    message: format!(
                        "distractor {index} answers {}, which distractor {earlier} already names — one wrong answer carries one diagnosis",
                        py_str(&distractor.answer)
                    ),
                });
            }
        }
        seen.push((index, canon));
    }
    Ok(())
}
