//! The three depth parts of a teach page: `picture`, `watch_out` and `second_example`.
//!
//! The checks run in a fixed order. The first failure is the message that the
//! next authoring attempt reads.
//!
//! A machine cannot check that a second example READS a result and does not
//! compute a new one. The prompt asks for it and the reviewer judges it in
//! `/review`. The machine checks the same leak rules as for the first example.
use crate::curriculum::FiniteObjectiveCase;
use crate::template::gate::{Rejection, py_str};

use super::body::{only_known, text};
use super::names_answer::names_any_answer;
use super::teach::{check_teach_disclosures, refuse_exemplar_problem, step_text};
use super::{
    InstructionSpec, MAX_PICTURE_CHARS, MAX_SECOND_STEPS, MAX_WATCH_OUT_CHARS,
    WORKED_EXAMPLE_FIELDS, WorkedExample,
};

type Fields = serde_json::Map<String, serde_json::Value>;

/// The `picture`, the `watch_out` and the `second_example` of a page.
pub(super) type Depth = (Option<String>, Option<String>, Option<WorkedExample>);

fn refuse(code: &'static str, message: String) -> Rejection {
    Rejection { code, message }
}

/// Read the three optional depth parts, in the order `picture`, `watch_out`,
/// `second_example`.
///
/// # Errors
///
/// Returns the [`Rejection`] of the first rule a part breaks.
pub(super) fn read_depth(
    fields: &Fields,
    concept: &str,
    problem: &str,
    spec: &InstructionSpec<'_>,
    taught: Option<&FiniteObjectiveCase>,
) -> Result<Depth, Rejection> {
    let picture = read_prose(fields, "picture", MAX_PICTURE_CHARS, spec)?;
    let watch_out = read_prose(fields, "watch_out", MAX_WATCH_OUT_CHARS, spec)?;
    if let Some(value) = &watch_out
        && value.trim().to_lowercase() == concept.trim().to_lowercase()
    {
        return Err(refuse(
            "teach-watch-out",
            "'watch_out' repeats 'concept' — write where the method fails".to_owned(),
        ));
    }
    let second = read_second(fields, problem, spec, taught)?;
    Ok((picture, watch_out, second))
}

/// Count the sentences of a text. A mark inside `$...$` and a point inside a
/// number such as `3.5` do not end a sentence.
fn sentence_count(value: &str) -> usize {
    let chars: Vec<char> = value.trim().chars().collect();
    let mut count = 0;
    let mut in_math = false;
    let mut open = false;
    for (index, &ch) in chars.iter().enumerate() {
        if ch == '$' {
            in_math = !in_math;
            open = true;
        } else if matches!(ch, '.' | '?' | '!') && !in_math {
            let next = chars.get(index + 1);
            if next.is_none_or(|next| next.is_whitespace()) {
                count += 1;
                open = false;
            }
        } else if !ch.is_whitespace() {
            open = true;
        }
    }
    count + usize::from(open)
}

/// Read `picture` or `watch_out`: a plain text of one or two sentences.
fn read_prose(
    fields: &Fields,
    key: &'static str,
    max_chars: usize,
    spec: &InstructionSpec<'_>,
) -> Result<Option<String>, Rejection> {
    let (code, answer_code): (&'static str, &'static str) = if key == "picture" {
        ("teach-picture", "teach-picture-answer")
    } else {
        ("teach-watch-out", "teach-watch-out-answer")
    };
    let value = match fields.get(key) {
        None | Some(serde_json::Value::Null) => return Ok(None),
        Some(value) => value,
    };
    let Some(written) = value.as_str().filter(|text| !text.trim().is_empty()) else {
        return Err(refuse(
            code,
            format!("'{key}' is a plain text of one or two sentences, or is left out"),
        ));
    };
    let sentences = sentence_count(written);
    if sentences > 2 {
        return Err(refuse(
            code,
            format!(
                "'{key}' holds {sentences} sentences — write two at most: one short, plain part \
of the page, and nothing else"
            ),
        ));
    }
    let length = written.chars().count();
    if length > max_chars {
        return Err(refuse(
            code,
            format!("'{key}' holds {length} characters — write at most {max_chars}"),
        ));
    }
    if let Some(answer) = names_any_answer(written, &spec.served_answers()) {
        return Err(refuse(
            answer_code,
            format!(
                "'{key}' reads {}, which names the answer {} that this knowledge point serves — \
this part shows the idea, never a result of a served problem (Hard Rule 3)",
                py_str(written),
                py_str(answer)
            ),
        ));
    }
    Ok(Some(written.to_owned()))
}

/// Read the optional `second_example` object.
fn read_second(
    fields: &Fields,
    first_problem: &str,
    spec: &InstructionSpec<'_>,
    taught: Option<&FiniteObjectiveCase>,
) -> Result<Option<WorkedExample>, Rejection> {
    let example = match fields.get("second_example") {
        None | Some(serde_json::Value::Null) => return Ok(None),
        Some(serde_json::Value::Object(example)) => example,
        Some(_) => {
            return Err(refuse(
                "teach-second-example",
                "'second_example' is an object with 'problem' and 'steps', or is left out"
                    .to_owned(),
            ));
        }
    };
    only_known(
        example,
        &WORKED_EXAMPLE_FIELDS,
        "second example",
        "teach-unknown-field",
    )?;
    let problem = text(
        example,
        "problem",
        "teach-second-example",
        "'second_example.problem' is a non-empty text: a short situation that states a result",
    )?;
    let steps = example
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .filter(|steps| !steps.is_empty())
        .ok_or_else(|| {
            refuse(
                "teach-second-example",
                "'second_example.steps' is a list of one to four lines that read the result"
                    .to_owned(),
            )
        })?;
    if steps.len() > MAX_SECOND_STEPS {
        return Err(refuse(
            "teach-second-example",
            format!(
                "'second_example.steps' holds {} steps — write four at most: this example reads \
a result and does not compute a new one",
                steps.len()
            ),
        ));
    }
    let written = steps
        .iter()
        .enumerate()
        .map(|(index, step)| step_text(index, step))
        .collect::<Result<Vec<String>, Rejection>>()?;
    if problem.trim() == first_problem.trim() {
        return Err(refuse(
            "teach-second-example",
            "'second_example.problem' repeats the first worked problem — write a different \
problem whose result the learner reads"
                .to_owned(),
        ));
    }
    refuse_exemplar_problem(&problem, spec, taught)?;
    check_teach_disclosures(&problem, &written, spec, taught)?;
    Ok(Some(WorkedExample {
        problem,
        steps: written,
    }))
}

#[cfg(test)]
mod tests {
    use super::sentence_count;

    #[test]
    fn sentences_are_counted_outside_math_and_numbers() {
        assert_eq!(sentence_count("One. Two."), 2);
        assert_eq!(sentence_count("It costs 3.5 coins."), 1);
        assert_eq!(sentence_count("Take $a.b$ now. Done"), 2);
        assert_eq!(sentence_count("Is it? Yes! Fine."), 3);
    }
}
