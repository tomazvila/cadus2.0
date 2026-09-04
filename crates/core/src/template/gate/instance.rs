//! Rows 24 to 28 of the gate, the canonical round trip, and the hint rule: the
//! rules that read one instance.

use std::collections::BTreeSet;

use indexmap::IndexMap;

use num_traits::{One, Signed};

use crate::answer::{Canon, canonical_form};
use crate::curriculum::AnswerKind;

use super::space::Walk;
use super::text::{
    contains_token, identifier_tokens, py_bindings, py_list, py_str, trailing_zero_run,
};
use super::{Envelope, GateSpec, NON_ANSWERS, RESERVED_NAMES, Rejection, exemplar_envelope};
use crate::template::document::{Compiled, Instance, InstantiateError, TemplateDoc};
use crate::template::domain::Bindings;
use crate::template::eval::EvalError;
use crate::template::render::{render, scan};

/// One rendered statement, and the answers the walk computed for it.
struct Statement {
    /// The rendered text, as the learner reads it.
    text: String,
    /// The count of walked tuples that render this text.
    tuples: u64,
    /// The distinct canonical answers those tuples computed.
    ///
    /// Equality of two answers is equality of two [`Canon`] values, so a set of
    /// more than one element is a statement with more than one right answer.
    answers: BTreeSet<Canon>,
}

/// Every instance renders, solves, canonicalizes, and looks like the topic, and
/// one statement carries one answer.
pub(super) fn check_instances(
    doc: &TemplateDoc,
    compiled: &Compiled<'_>,
    spec: &GateSpec,
    walk: &Walk,
) -> Result<(), Rejection> {
    let envelope = exemplar_envelope(spec.exemplars);
    let mut statements: IndexMap<String, Statement> = IndexMap::new();
    for bindings in &walk.tuples {
        let instance = instantiate(compiled, bindings)?;
        check_one_instance(doc, spec, envelope.as_ref(), &instance)?;
        match statements.get_mut(&instance.instance_hash) {
            Some(statement) => {
                statement.tuples = statement.tuples.saturating_add(1);
                statement.answers.insert(instance.canon);
            }
            None => {
                let mut answers = BTreeSet::new();
                answers.insert(instance.canon);
                statements.insert(
                    instance.instance_hash,
                    Statement {
                        text: instance.text,
                        tuples: 1,
                        answers,
                    },
                );
            }
        }
    }
    check_one_answer_per_statement(&statements)
}

/// Two tuples that render ONE statement must compute ONE answer (C4).
///
/// The pool keys an instance by the digest of its statement
/// (`serving_pool.instance_hash`), so a statement two tuples render with two
/// different answers reaches a learner as ONE printed problem whose stored
/// answer is the one of whichever tuple the fill met first. A learner who reads
/// the other one answers correctly and the grade path records a failure.
///
/// The hidden-parameter rule is a proxy for this rule and a strictly weaker one:
/// it refuses a parameter the statement never shows, and two SHOWN parameters
/// collide as well when the statement writes them next to each other, for
/// example `${a}{b}$` with `a = 1, b = 12` and `a = 11, b = 2` (M4 review 2,
/// finding 1). The gate holds the rendered text and the canonical answer of
/// every walked tuple already, so the rule is a grouping of what it read.
///
/// The rule runs on the sampled walk too. Above the exhaustive limit it reads
/// the tuples the walk drew, so it refuses a collision the sample carries and it
/// says nothing about a collision the sample missed; `TemplateSource::fill`
/// refuses that one per batch.
fn check_one_answer_per_statement(
    statements: &IndexMap<String, Statement>,
) -> Result<(), Rejection> {
    for statement in statements.values() {
        if statement.answers.len() > 1 {
            let count = statement.tuples;
            return Err(Rejection::new(
                "statement-collision",
                format!(
                    "statement {} renders from {count} tuples with different answers",
                    py_str(&statement.text)
                ),
            ));
        }
    }
    Ok(())
}

/// Verify ONE instance against the per-instance rules of the gate.
///
/// The refill of D-O4 calls this on every instance it builds, because the gate
/// reads a sample of a large space and the refill draws from the same space: an
/// instance the gate never saw must meet the same rules before it enters the
/// pool (M4 review 1, findings 1, 2, and 15). The rules are the ones that read
/// the instance alone — the exemplar envelope, the non-answer tokens, the free
/// symbols, the decimal trailing-zero run, the hint give-away, and the canonical
/// round trip.
///
/// # Errors
///
/// Returns [`Rejection`] with the literal message of the rule that refused the
/// instance.
pub fn check_instance(
    doc: &TemplateDoc,
    spec: &GateSpec<'_>,
    instance: &Instance,
) -> Result<(), Rejection> {
    let envelope = exemplar_envelope(spec.exemplars);
    check_one_instance(doc, spec, envelope.as_ref(), instance)
}

/// The per-instance rules, with the envelope already read.
fn check_one_instance(
    doc: &TemplateDoc,
    spec: &GateSpec,
    envelope: Option<&Envelope>,
    instance: &Instance,
) -> Result<(), Rejection> {
    let bindings = &instance.bindings;
    if !scan(&instance.text).0.is_empty() {
        return Err(Rejection::new(
            "placeholder-left",
            "a rendered problem still contains a placeholder".to_string(),
        ));
    }
    if instance.answer.trim().is_empty() {
        return Err(Rejection::new(
            "empty-answer",
            format!(
                "instantiation for {} produced no answer",
                py_bindings(bindings)
            ),
        ));
    }
    let tokens = identifier_tokens(&instance.answer);
    let poisoned: Vec<String> = tokens
        .iter()
        .filter(|token| NON_ANSWERS.contains(&token.as_str()))
        .cloned()
        .collect();
    if !poisoned.is_empty() {
        return Err(Rejection::new(
            "not-a-number",
            format!(
                "instance {} answers {}, which is not a number ({})",
                py_bindings(bindings),
                py_str(&instance.answer),
                py_list(&poisoned)
            ),
        ));
    }
    if trailing_zero_run(&instance.answer) {
        return Err(Rejection::new(
            "decimal-answer",
            format!(
                "instance {} answers {}, which is a decimal with a trailing zero run — 2.0 answers hold exact values only (D6)",
                py_bindings(bindings),
                py_str(&instance.answer)
            ),
        ));
    }
    if spec.answer_kind == AnswerKind::Numeric {
        let leftover: Vec<String> = tokens
            .into_iter()
            .filter(|token| !RESERVED_NAMES.contains(&token.as_str()))
            .collect();
        if !leftover.is_empty() {
            return Err(Rejection::new(
                "free-symbol",
                format!(
                    "numeric answer {} for {} still contains {} — a parameter is undeclared",
                    py_str(&instance.answer),
                    py_bindings(bindings),
                    py_list(&leftover)
                ),
            ));
        }
        if let Some(envelope) = envelope {
            check_envelope(instance, envelope)?;
        }
    }
    check_canonical(instance)?;
    check_hints(doc, instance)
}

/// The answer string reads back as the canonical form the instance carries (V2).
fn check_canonical(instance: &Instance) -> Result<(), Rejection> {
    let read = canonical_form(&instance.answer).map_err(|reason| {
        Rejection::new(
            "undecidable-answer",
            format!(
                "instance {} answers {}, which the answer checker cannot decide: {} — every instance answer must canonicalize (V2)",
                py_bindings(&instance.bindings),
                py_str(&instance.answer),
                reason.reason
            ),
        )
    })?;
    if read != instance.canon {
        return Err(Rejection::new(
            "canonical-mismatch",
            format!(
                "instance {} answers {}, which does not read back as the canonical form the instance carries — the answer and its canonical form must agree (V2)",
                py_bindings(&instance.bindings),
                py_str(&instance.answer)
            ),
        ));
    }
    Ok(())
}

/// Render and solve one tuple, or say which step refused it.
fn instantiate(compiled: &Compiled<'_>, bindings: &Bindings) -> Result<Instance, Rejection> {
    compiled
        .instantiate(bindings.clone())
        .map_err(|err| match err {
            InstantiateError::Eval(EvalError::NotCanonical { text, reason }) => Rejection::new(
                "undecidable-answer",
                format!(
                    "instance {} answers {}, which the answer checker cannot decide: {} — every instance answer must canonicalize (V2)",
                    py_bindings(bindings),
                    py_str(&text),
                    reason.reason
                ),
            ),
            other => Rejection::new(
                "instantiation",
                format!(
                    "instantiation failed for {}: {other}",
                    py_bindings(bindings)
                ),
            ),
        })
}

/// The instance answer looks like the knowledge point's authored answers.
fn check_envelope(instance: &Instance, envelope: &Envelope) -> Result<(), Rejection> {
    let sign_refusal = || {
        Rejection::new(
            "envelope-sign",
            format!(
                "instance {} answers {}, but every authored answer for this knowledge point is non-negative — narrow the domains so no instance goes below zero",
                py_bindings(&instance.bindings),
                py_str(&instance.answer)
            ),
        )
    };
    let whole_refusal = || {
        Rejection::new(
            "envelope-integral",
            format!(
                "instance {} answers {}, but every authored answer for this knowledge point is a whole number",
                py_bindings(&instance.bindings),
                py_str(&instance.answer)
            ),
        )
    };
    match &instance.canon {
        Canon::Rational(value) => {
            if envelope.non_negative && value.numer().is_negative() {
                return Err(sign_refusal());
            }
            if envelope.integral && !value.denom().is_one() {
                return Err(whole_refusal());
            }
            Ok(())
        }
        // A surd or a formula is not a plain number, so only the integrality rule
        // can speak. 1.0 decides the same case the same way (`:775-781`).
        _ => {
            if envelope.integral {
                return Err(whole_refusal());
            }
            Ok(())
        }
    }
}

/// No hint rung names the answer of the instance it is shown with.
///
/// Hard Rule 3: a hint is Socratic and never the final step
/// (`docs/WEB_SERVICE.md:22-32`). The rule reads the RENDERED rung, because a
/// rung that writes `${a}` gives nothing away until an instance binds it.
///
/// A token the statement already shows is not given away: the learner is reading
/// it in the problem. Without that exemption the rule refuses a whole template
/// because one degenerate instance answers a digit the statement carries — `a` of
/// 1 in `Compute ${a}^{{2}}$.` answers `1`, and every rung that names `${a}` then
/// looks like a give-away.
fn check_hints(doc: &TemplateDoc, instance: &Instance) -> Result<(), Rejection> {
    if contains_token(&instance.text, &instance.answer) {
        return Ok(());
    }
    // Every rung passed the rendered-field rules, so every rung renders.
    let rungs = doc.hints.iter().enumerate().filter_map(|(index, hint)| {
        render(hint, &instance.bindings)
            .ok()
            .map(|text| (index, text))
    });
    for (index, rendered) in rungs {
        if contains_token(&rendered, &instance.answer) {
            return Err(Rejection::new(
                "hint-answer",
                format!(
                    "hint {index} reads {} for {}, which names the answer {} — a hint is a question, never the final step (Hard Rule 3)",
                    py_str(&rendered),
                    py_bindings(&instance.bindings),
                    py_str(&instance.answer)
                ),
            ));
        }
    }
    Ok(())
}
