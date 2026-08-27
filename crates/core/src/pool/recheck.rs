//! The per-instance re-check that every instance passes before the pool (C4, C6).
//!
//! # Why the re-check exists
//!
//! The gate walks the whole space only at or under
//! [`EXHAUSTIVE_SPACE_LIMIT`](crate::template::EXHAUSTIVE_SPACE_LIMIT). Above it
//! the gate reads a sample of 4,096 tuples from a constant seed, while
//! [`TemplateSource::fill`](super::TemplateSource::fill) draws from the batch
//! seed of the refill. The two sets are different, so a corner the gate never
//! read reaches the pool with no per-instance rule applied to it. 1.0 records the
//! live incident this causes (`problem_templates.py:432-437`): a 10,000-instance
//! subtraction template was accepted on 22 of 60 seeds and served 55 negative
//! instances.
//!
//! 1.0 answers with a re-check at serve time (`problem_templates.py:399-403`).
//! 2.0 answers with the same re-check one step earlier, in the fill, so the cost
//! stays off the request path (L1) and no unchecked row exists at all.
//!
//! # What the re-check reads
//!
//! One instance, one document, and the [`GateSpec`] of the knowledge point. It
//! runs the rules that read ONE instance:
//!
//! | Rule | Code |
//! |---|---|
//! | no placeholder is left in the statement | `placeholder-left` |
//! | the answer is not empty | `empty-answer` |
//! | no token of [`NON_ANSWERS`] stands in the answer | `not-a-number` |
//! | a decimal answer has no trailing zero run | `decimal-answer` |
//! | a numeric answer carries no free symbol | `free-symbol` |
//! | the answer stays inside the exemplar envelope | `envelope-sign`, `envelope-integral` |
//! | no hint rung names the answer | `hint-answer` |
//!
//! The canonical round trip is not a rule here: `Compiled::instantiate` refuses
//! an answer that does not canonicalize, so an instance that reaches this
//! function canonicalized already (V2).
//!
//! # No clock, no socket, no model
//!
//! The function is pure (R3, T1).

use std::collections::BTreeSet;

use num_traits::{One, Signed};

use crate::answer::Canon;
use crate::template::{
    Bindings, Envelope, GateSpec, Instance, NON_ANSWERS, RESERVED_NAMES, Rejection, TemplateDoc,
    Value, exemplar_envelope, render, scan,
};

use crate::curriculum::AnswerKind;

/// The refusal of one instance.
///
/// FIXM4a exports the same type from `cadus_core::template`. This alias keeps the
/// call site of the fill unchanged when that lands.
pub type Refusal = Rejection;

/// Check one instance against every per-instance rule of the gate.
///
/// # Errors
///
/// Returns the [`Refusal`] of the first rule the instance breaks. The message is
/// the literal message the gate writes for the same rule, so an operator reads
/// one sentence for one defect wherever it is found.
pub fn check_instance(
    doc: &TemplateDoc,
    spec: &GateSpec,
    instance: &Instance,
) -> Result<(), Refusal> {
    let envelope = exemplar_envelope(spec.exemplars);
    check_one(doc, spec, envelope.as_ref(), instance)
}

/// Check one instance against a prepared envelope.
///
/// The fill reads the envelope of a knowledge point once per batch, and not once
/// per instance: [`exemplar_envelope`] canonicalizes every authored answer, and a
/// batch of 24 instances would run that work 24 times.
pub(super) fn check_one(
    doc: &TemplateDoc,
    spec: &GateSpec,
    envelope: Option<&Envelope>,
    instance: &Instance,
) -> Result<(), Refusal> {
    if !scan(&instance.text).0.is_empty() {
        return Err(Refusal {
            code: "placeholder-left",
            message: "a rendered problem still contains a placeholder".to_string(),
        });
    }
    if instance.answer.trim().is_empty() {
        return Err(Refusal {
            code: "empty-answer",
            message: format!(
                "instantiation for {} produced no answer",
                py_bindings(&instance.bindings)
            ),
        });
    }
    let tokens = identifier_tokens(&instance.answer);
    let poisoned: Vec<String> = tokens
        .iter()
        .filter(|token| NON_ANSWERS.contains(&token.as_str()))
        .cloned()
        .collect();
    if !poisoned.is_empty() {
        return Err(Refusal {
            code: "not-a-number",
            message: format!(
                "instance {} answers {}, which is not a number ({})",
                py_bindings(&instance.bindings),
                py_str(&instance.answer),
                py_list(&poisoned)
            ),
        });
    }
    if trailing_zero_run(&instance.answer) {
        return Err(Refusal {
            code: "decimal-answer",
            message: format!(
                "instance {} answers {}, which is a decimal with a trailing zero run — 2.0 answers hold exact values only (D6)",
                py_bindings(&instance.bindings),
                py_str(&instance.answer)
            ),
        });
    }
    if spec.answer_kind == AnswerKind::Numeric {
        let leftover: Vec<String> = tokens
            .into_iter()
            .filter(|token| !RESERVED_NAMES.contains(&token.as_str()))
            .collect();
        if !leftover.is_empty() {
            return Err(Refusal {
                code: "free-symbol",
                message: format!(
                    "numeric answer {} for {} still contains {} — a parameter is undeclared",
                    py_str(&instance.answer),
                    py_bindings(&instance.bindings),
                    py_list(&leftover)
                ),
            });
        }
        if let Some(envelope) = envelope {
            check_envelope(instance, envelope)?;
        }
    }
    check_hints(doc, instance)
}

/// The instance answer looks like the knowledge point's authored answers.
fn check_envelope(instance: &Instance, envelope: &Envelope) -> Result<(), Refusal> {
    let sign_refusal = || Refusal {
        code: "envelope-sign",
        message: format!(
            "instance {} answers {}, but every authored answer for this knowledge point is non-negative — narrow the domains so no instance goes below zero",
            py_bindings(&instance.bindings),
            py_str(&instance.answer)
        ),
    };
    let whole_refusal = || Refusal {
        code: "envelope-integral",
        message: format!(
            "instance {} answers {}, but every authored answer for this knowledge point is a whole number",
            py_bindings(&instance.bindings),
            py_str(&instance.answer)
        ),
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
        // can speak.
        _ => {
            if envelope.integral {
                return Err(whole_refusal());
            }
            Ok(())
        }
    }
}

/// No hint rung names the answer of the instance it is shown with (Hard Rule 3).
fn check_hints(doc: &TemplateDoc, instance: &Instance) -> Result<(), Refusal> {
    if contains_token(&instance.text, &instance.answer) {
        return Ok(());
    }
    for (index, hint) in doc.hints.iter().enumerate() {
        let Ok(rendered) = render(hint, &instance.bindings) else {
            continue;
        };
        if contains_token(&rendered, &instance.answer) {
            return Err(Refusal {
                code: "hint-answer",
                message: format!(
                    "hint {index} reads {} for {}, which names the answer {} — a hint is a question, never the final step (Hard Rule 3)",
                    py_str(&rendered),
                    py_bindings(&instance.bindings),
                    py_str(&instance.answer)
                ),
            });
        }
    }
    Ok(())
}

// --------------------------------------------------------------------------
// The message writers
//
// FIXM4a owns the same six functions inside `template::gate`, where they are
// private. They are written out again here, and not shared, because the two
// units edit different files in one milestone. The FIXM4a export removes this
// half.
// --------------------------------------------------------------------------

/// Every `[A-Za-z_][A-Za-z0-9_]*` run of a string, in name order.
fn identifier_tokens(text: &str) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    let mut current = String::new();
    for character in text.chars() {
        let starts = character.is_ascii_alphabetic() || character == '_';
        let continues = starts || character.is_ascii_digit();
        if current.is_empty() && starts {
            current.push(character);
            continue;
        }
        if !current.is_empty() && continues {
            current.push(character);
            continue;
        }
        if !current.is_empty() {
            tokens.insert(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.insert(current);
    }
    tokens
}

/// Whether the answer stands in the text as a token of its own.
fn contains_token(text: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let characters: Vec<char> = text.chars().collect();
    let needle: Vec<char> = token.chars().collect();
    let last = characters.len().saturating_sub(needle.len());
    for start in 0..=last {
        let Some(window) = characters.get(start..start + needle.len()) else {
            continue;
        };
        if window != needle.as_slice() {
            continue;
        }
        let before = start.checked_sub(1);
        let after = start + needle.len();
        let free_before = before.is_none_or(|index| {
            let outer = index.checked_sub(1).and_then(|far| characters.get(far));
            free_side(characters.get(index), outer)
        });
        let free_after = free_side(characters.get(after), characters.get(after + 1));
        if free_before && free_after {
            return true;
        }
    }
    false
}

/// Whether one side of a run leaves the run standing on its own.
fn free_side(near: Option<&char>, far: Option<&char>) -> bool {
    let Some(near) = near else {
        return true;
    };
    if near.is_ascii_alphanumeric() || *near == '_' {
        return false;
    }
    if *near == '.' || *near == '/' {
        return !far.is_some_and(char::is_ascii_digit);
    }
    true
}

/// Whether the text is a decimal that ends in a zero run (spec trap 3).
fn trailing_zero_run(text: &str) -> bool {
    let Some((_, fraction)) = text.split_once('.') else {
        return false;
    };
    !fraction.is_empty()
        && fraction.chars().all(|character| character.is_ascii_digit())
        && fraction.ends_with('0')
}

/// Write one string the way Python's `repr` writes it.
fn py_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('\'');
    out
}

/// Write a name list the way Python writes a sorted list of strings.
fn py_list(items: &[String]) -> String {
    let written: Vec<String> = items.iter().map(|item| py_str(item)).collect();
    format!("[{}]", written.join(", "))
}

/// Write a bound tuple the way Python writes a dictionary.
fn py_bindings(bindings: &Bindings) -> String {
    let written: Vec<String> = bindings
        .iter()
        .map(|(name, value)| match value {
            Value::Num(_) => format!("{}: {}", py_str(name), value.canonical_string()),
            Value::Text(text) => format!("{}: {}", py_str(name), py_str(text)),
        })
        .collect();
    format!("{{{}}}", written.join(", "))
}
