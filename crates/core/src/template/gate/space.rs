//! Rows 15 to 19b of the gate: the satisfying walk, the worked samples, and the
//! distractors.

use crate::answer::{Outcome, same_answer};

use super::text::{py_bindings, py_list, py_str};
use super::{GATE_SAMPLES, GateSpec, Rejection};
use crate::template::document::{Compiled, TemplateDoc};
use crate::template::domain::{Bindings, MIN_SPACE_SIZE, SpaceSize, walk_satisfying};
use crate::template::eval::{answer as evaluate_answer, parse_answer_expr};

/// The tuples the gate reads, and the count it stores.
pub(super) struct Walk {
    /// The satisfying count: exact below the limit, the found count above it.
    pub(super) space: SpaceSize,
    /// The distinct tuples the instance check reads.
    pub(super) tuples: Vec<Bindings>,
    /// True when `tuples` is every satisfying tuple.
    pub(super) exhaustive: bool,
    /// The count of tuples the sampled walk drew, and `None` below the limit.
    pub(super) drawn: Option<u32>,
    /// What the walk did not do, and why.
    pub(super) notes: Vec<String>,
}

/// Count the satisfying tuples and collect the ones the gate reads.
///
/// The count and the tuples come from ONE call, so the number the body records
/// is the count of distinct instances the walk actually found (M4 review 2,
/// findings 2 and 5).
pub(super) fn build_walk(doc: &TemplateDoc) -> Result<Walk, Rejection> {
    let walked = walk_satisfying(&doc.params, &doc.constraints).map_err(|err| {
        Rejection::new(
            "domain-size",
            format!("the declared domains do not count: {err}"),
        )
    })?;
    let mut notes = Vec::new();
    if let Some(drawn) = walked.drawn {
        let found = walked.tuples.len();
        if u32::try_from(found).unwrap_or(u32::MAX) < GATE_SAMPLES {
            notes.push(format!(
                "the sampled walk found {found} satisfying tuple(s) in {drawn} draw(s), and the instance check read those {found}"
            ));
        }
    }
    Ok(Walk {
        space: walked.space,
        tuples: walked.tuples,
        exhaustive: walked.exhaustive,
        drawn: walked.drawn,
        notes,
    })
}

/// The satisfying set is not empty, the stored count is the true one, and the
/// space clears the distinct-problem floor.
pub(super) fn check_space(doc: &TemplateDoc, walk: &Walk) -> Result<(), Rejection> {
    if walk.tuples.is_empty() {
        // The sampled branch names what it did, because it read a sample and not
        // the whole space (M4 review 1, finding 12).
        let message = match walk.drawn {
            None => "the constraints refuse every tuple of the declared domains, so the template has no instance to serve".to_string(),
            Some(drawn) => format!(
                "the sampled walk drew {drawn} tuple(s) of the declared domains and 0 satisfied the constraints, so the template has no instance to serve"
            ),
        };
        return Err(Rejection::new("no-satisfying-tuple", message));
    }
    if let Some(stated) = doc.space_size
        && (stated.count() != walk.space.count() || stated.is_exact() != walk.space.is_exact())
    {
        return Err(Rejection::new(
            "space-size",
            format!(
                "space_size states {} and the gate counts {} — the gate fills space_size, not the author",
                stated.count(),
                walk.space.count()
            ),
        ));
    }
    let count = walk.space.count();
    if count < MIN_SPACE_SIZE {
        return Err(Rejection::new(
            "space-floor",
            format!(
                "the declared domains produce only {count} distinct problem(s); at least {MIN_SPACE_SIZE} are needed for randomized values and for avoidance of a recently-served problem to mean anything (Hard Rule 4)"
            ),
        ));
    }
    Ok(())
}

// --------------------------------------------------------------------------
// Rows 16 to 19b, and the 2.0 constraint check on the samples
// --------------------------------------------------------------------------

/// Every worked sample agrees with what the server computes.
pub(super) fn check_samples(
    doc: &TemplateDoc,
    compiled: &Compiled<'_>,
    spec: &GateSpec,
) -> Result<Vec<Bindings>, Rejection> {
    if doc.samples.is_empty() {
        return Err(Rejection::new(
            "no-samples",
            "a template needs worked samples to verify it".to_string(),
        ));
    }
    let declared: Vec<String> = doc.params.keys().cloned().collect();
    let mut validated = Vec::with_capacity(doc.samples.len());
    for sample in &doc.samples {
        let bound: Vec<String> = sample.params.keys().cloned().collect();
        if bound != declared {
            return Err(Rejection::new(
                "sample-binding",
                format!(
                    "sample binds {}, template declares {}",
                    py_list(&bound),
                    py_list(&declared)
                ),
            ));
        }
        let bindings = sample.bindings();
        let computed = evaluate_answer(compiled.answer_ast(), &bindings).map_err(|err| {
            Rejection::new(
                "sample-eval",
                format!(
                    "answer_expr failed on sample {}: {err}",
                    py_bindings(&bindings)
                ),
            )
        })?;
        let claimed = sample.expected.text();
        let outcome = doc.answer_contract.map_or_else(
            || crate::answer::check(&computed.text, &claimed, spec.answer_kind),
            |contract| crate::answer::check_contract(&computed.text, &claimed, contract),
        );
        let agrees = matches!(
            outcome,
            Outcome::Decided(verdict) if verdict.correct
        );
        if !agrees {
            return Err(Rejection::new(
                "sample-agreement",
                format!(
                    "answer_expr gives {} for {} but the sample claims {} — the expression does not compute the stated answer",
                    py_str(&computed.text),
                    py_bindings(&bindings),
                    py_str(&claimed)
                ),
            ));
        }
        validated.push(bindings);
    }
    Ok(validated)
}

/// Every distractor parses, carries a tag, and is not the right answer.
pub(super) fn check_distractors(
    doc: &TemplateDoc,
    compiled: &Compiled<'_>,
) -> Result<(), Rejection> {
    for (index, distractor) in doc.distractors.iter().enumerate() {
        if distractor.error_tag.trim().is_empty() {
            return Err(Rejection::new(
                "distractor",
                format!("distractor {index} carries no error_tag"),
            ));
        }
        let wrong = parse_answer_expr(&distractor.answer).map_err(|reason| {
            Rejection::new(
                "distractor",
                format!(
                    "distractor {index} answers {}, which is outside the decidable grammar: {}",
                    py_str(&distractor.answer),
                    reason.reason
                ),
            )
        })?;
        for sample in &doc.samples {
            let bindings = sample.bindings();
            let (Ok(wrong_value), Ok(right_value)) = (
                evaluate_answer(&wrong, &bindings),
                evaluate_answer(compiled.answer_ast(), &bindings),
            ) else {
                continue;
            };
            if same_answer(&right_value.canon, &wrong_value.canon) {
                return Err(Rejection::new(
                    "distractor",
                    format!(
                        "distractor {index} answers {} for {}, which is the right answer — a distractor names a mistake",
                        py_str(&wrong_value.text),
                        py_bindings(&bindings)
                    ),
                ));
            }
        }
    }
    Ok(())
}
