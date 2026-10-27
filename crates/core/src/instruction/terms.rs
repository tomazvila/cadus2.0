//! The term list of a teach page: at most six words, each with one plain sentence.
//!
//! The checks run in a fixed order. The first failure is the message that the
//! next authoring attempt reads.
use crate::template::gate::{Rejection, py_str};

use super::InstructionSpec;
use super::body::{only_known, text};
use super::names_answer::names_any_answer;
use super::{MAX_MEANING_CHARS, MAX_TERMS, TERM_FIELDS, Term};

fn refuse(code: &'static str, message: String) -> Rejection {
    Rejection { code, message }
}

/// Read and check the optional `terms` field of a teach body.
///
/// An absent key, a `null` and an empty list give the same empty result.
///
/// # Errors
///
/// Returns the [`Rejection`] of the first rule the list breaks.
pub(super) fn read_terms(
    fields: &serde_json::Map<String, serde_json::Value>,
    spec: &InstructionSpec<'_>,
) -> Result<Vec<Term>, Rejection> {
    let rows = match fields.get("terms") {
        None | Some(serde_json::Value::Null) => return Ok(Vec::new()),
        Some(serde_json::Value::Array(rows)) => rows,
        Some(_) => {
            return Err(refuse(
                "teach-terms",
                "'terms' is a list of {term, meaning} rows, at most 6, or is left out".to_owned(),
            ));
        }
    };
    if rows.len() > MAX_TERMS {
        return Err(refuse(
            "teach-terms",
            format!(
                "'terms' holds {} rows — keep at most 6: the words a learner meets on THIS page \
for the first time",
                rows.len()
            ),
        ));
    }
    let served = spec.served_answers();
    let mut read: Vec<Term> = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let Some(row) = row.as_object() else {
            return Err(refuse(
                "teach-term",
                format!("row {index} of 'terms' is not an object with 'term' and 'meaning'"),
            ));
        };
        only_known(row, &TERM_FIELDS, "term row", "teach-unknown-field")?;
        let term = text(
            row,
            "term",
            "teach-term",
            &format!("row {index} of 'terms' has an empty 'term'"),
        )?;
        let meaning = text(
            row,
            "meaning",
            "teach-term",
            &format!(
                "row {index} of 'terms' has an empty 'meaning': one plain sentence a learner reads"
            ),
        )?;
        let length = meaning.chars().count();
        if length > MAX_MEANING_CHARS {
            return Err(refuse(
                "teach-term",
                format!(
                    "row {index} of 'terms' has a 'meaning' of {length} characters — one plain \
sentence, at most 200 characters"
                ),
            ));
        }
        let key = term.trim().to_lowercase();
        if let Some(earlier) = read
            .iter()
            .position(|row| row.term.trim().to_lowercase() == key)
        {
            return Err(refuse(
                "teach-term-repeat",
                format!("row {index} of 'terms' repeats row {earlier}"),
            ));
        }
        if let Some(answer) = names_any_answer(&meaning, &served) {
            return Err(refuse(
                "teach-term-answer",
                format!(
                    "row {index} of 'terms' reads {}, which names the answer {} this knowledge \
point serves — a meaning explains a word, never a result (Hard Rule 3)",
                    py_str(&meaning),
                    py_str(answer)
                ),
            ));
        }
        read.push(Term { term, meaning });
    }
    Ok(read)
}
