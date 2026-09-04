//! Small readers and writers: identifiers, tokens, and the Python spellings the
//! messages quote.

use std::collections::BTreeSet;

use super::Rejection;
use crate::template::constraint::ConstraintError;
use crate::template::domain::{Bindings, Scalar, Value};

/// The rejection an undecidable constraint earns.
pub(super) fn constraint_rejection(err: ConstraintError) -> Rejection {
    Rejection::new("constraint-parameter", err.to_string())
}

/// Whether the name is a Python identifier (1.0 `str.isidentifier`).
///
/// The gate reads ASCII names. Python admits a wider set; a template whose
/// parameter is written in another script is refused here, and that is the
/// stricter reading.
pub(super) fn is_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Every `[A-Za-z_][A-Za-z0-9_]*` run of a string, in name order.
pub(super) fn identifier_tokens(text: &str) -> BTreeSet<String> {
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
///
/// A letter, a digit, or an underscore beside the run means the run is part of a
/// longer word or number, so `144` does not stand inside `1442`.
///
/// A point or a slash breaks the run only when a digit stands on its far side.
/// That one rule keeps three readings right at once: `16` stands at the end of
/// `is 16.`, because the point ends a sentence; `2` does not stand inside `1/2`,
/// because the slash carries the numerator; and `0` does not stand inside `0.5`,
/// because the point carries the decimal.
pub(crate) fn contains_token(text: &str, token: &str) -> bool {
    let needle: Vec<char> = token.chars().collect();
    let characters: Vec<char> = text.chars().collect();
    // An empty token stands nowhere: a window of one character never equals it.
    characters
        .windows(needle.len().max(1))
        .enumerate()
        .any(|(start, window)| {
            window == needle.as_slice() && stands_free(&characters, start, needle.len())
        })
}

/// Whether the run of `width` characters at `start` has a free side on both ends.
fn stands_free(characters: &[char], start: usize, width: usize) -> bool {
    let before = start.checked_sub(1);
    let after = start + width;
    let free_before = before.is_none_or(|index| {
        let outer = index.checked_sub(1).and_then(|far| characters.get(far));
        free_side(characters.get(index), outer)
    });
    free_before && free_side(characters.get(after), characters.get(after + 1))
}

/// Whether one side of a run leaves the run standing on its own.
///
/// `near` is the character beside the run and `far` is the one beyond it.
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
pub(super) fn trailing_zero_run(text: &str) -> bool {
    let Some((_, fraction)) = text.split_once('.') else {
        return false;
    };
    !fraction.is_empty()
        && fraction.chars().all(|character| character.is_ascii_digit())
        && fraction.ends_with('0')
}

/// Write one string the way Python's `repr` writes it.
///
/// Every message the gate quotes carries a name, a snippet, or an answer, and
/// none of them holds a quote character, so the quote-switching rule of Python's
/// `repr` never applies: the writer always uses single quotes and escapes a
/// backslash and a quote.
pub(crate) fn py_str(text: &str) -> String {
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
pub(crate) fn py_list(items: &[String]) -> String {
    let written: Vec<String> = items.iter().map(|item| py_str(item)).collect();
    format!("[{}]", written.join(", "))
}

/// Write a bound tuple the way Python writes a dictionary.
///
/// A number writes its digits and a text writes its `repr`, so the message reads
/// `{'a': 59, 'b': 63}` for the tuple 1.0 refuses in the specification's live
/// run (section 4).
pub(super) fn py_bindings(bindings: &Bindings) -> String {
    let written: Vec<String> = bindings
        .iter()
        .map(|(name, value)| match value {
            Value::Num(_) | Value::Spelled { .. } => {
                format!("{}: {}", py_str(name), value.canonical_string())
            }
            Value::Text(text) => format!("{}: {}", py_str(name), py_str(text)),
        })
        .collect();
    format!("{{{}}}", written.join(", "))
}

/// Write one sample scalar the way Python's `repr` writes it.
pub(super) fn scalar_repr(scalar: &Scalar) -> String {
    match scalar {
        Scalar::Int(value) => value.to_string(),
        Scalar::Text(text) => py_str(text),
    }
}
