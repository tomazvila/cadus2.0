//! The statement renderer: a scanner over the `{name}` grammar (A1, spec 3.3).
//!
//! 1.0 renders with `str.format_map` (`problem_templates.py:328-336`). That call
//! brings a whole mini-language with it — a format spec, a conversion, an
//! attribute access, an index — and 1.0 fences the language off with a narrow
//! regular expression (`_PLACEHOLDER_RE`, `:230`). 2.0 does not use a template
//! engine at all. It scans, and the scanner reads three things and refuses
//! everything else:
//!
//! - `{{` writes one `{`, and `}}` writes one `}`.
//! - `{name}` writes the canonical string of the bound value, where `name`
//!   matches `[A-Za-z_][A-Za-z0-9_]*`.
//! - Every other brace is a [`RenderError::StrayBrace`].
//!
//! # Why the doubled brace stays
//!
//! Every statement is KaTeX-subset LaTeX inside `$…$`, and LaTeX is full of
//! braces: `$7^{{2}}$` renders `$7^{2}$` and `$7{{,}}329$` renders `$7{,}329$`.
//! The doubling is the renderer's grammar, not a leftover of the Python call
//! (spec section 8, trap 5).
//!
//! # Why a scanner and not two regular expressions
//!
//! The two constructs interleave. A pattern that reads `{{` and `{name}` apart
//! still flags the closing brace of a valid placeholder, and 1.0 shipped that
//! bug once (`problem_templates.py:238-241`). One left-to-right scan has no such
//! reading.
//!
//! # The value never goes through a number formatter
//!
//! [`super::domain::Value::canonical_string`] writes an exact rational, so a
//! rendered statement never carries a float, and it never introduces a thousands
//! separator the checker would then have to undo (spec section 8, trap 7).
//!
//! # A value that is not atomic takes brackets
//!
//! The evaluator brackets a negative literal and a fraction on the answer side,
//! because `-3**2` and `3/2**2` re-read. The statement side needs the same rule,
//! or the printed problem asks a different question than the stored answer
//! answers (M4 review 1, finding 18). The renderer therefore writes `(-3)` and
//! `(3/2)`, and [`super::domain::Value::needs_brackets`] holds the rule. An
//! author who wants the bare form writes the sign in the statement.

use std::collections::BTreeSet;

use super::domain::Bindings;

/// The count of characters the stray-brace report quotes (1.0 `:521`).
pub const SNIPPET_CHARS: usize = 12;

/// A statement the renderer refuses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    /// A brace that is neither doubled nor part of a placeholder.
    #[error("unescaped brace at index {index} ({snippet:?})")]
    StrayBrace {
        /// The character index of the brace in the statement.
        index: usize,
        /// The [`SNIPPET_CHARS`] characters that start at `index`.
        snippet: String,
    },
    /// A placeholder that names a parameter the tuple does not bind.
    ///
    /// The renderer refuses the whole statement. A hole never reaches a learner,
    /// which is the one property 1.0 buys with `format_map`'s `KeyError`.
    #[error("the statement uses undeclared parameter {name:?}")]
    Undeclared {
        /// The name the placeholder writes.
        name: String,
    },
}

/// A brace the scanner refuses, with the report the gate quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrayBrace {
    /// The character index of the brace in the statement.
    pub index: usize,
    /// The [`SNIPPET_CHARS`] characters that start at `index`.
    pub snippet: String,
}

/// Render one statement against a bound tuple.
///
/// # Errors
///
/// Returns [`RenderError::StrayBrace`] for a brace that is neither doubled nor
/// part of a placeholder, and [`RenderError::Undeclared`] for a placeholder the
/// tuple does not bind.
pub fn render(statement: &str, bindings: &Bindings) -> Result<String, RenderError> {
    let chars: Vec<char> = statement.chars().collect();
    let mut out = String::with_capacity(statement.len());
    let mut at = 0_usize;
    while let Some(character) = chars.get(at).copied() {
        if character != '{' && character != '}' {
            out.push(character);
            at += 1;
            continue;
        }
        if chars.get(at + 1).copied() == Some(character) {
            out.push(character);
            at += 2;
            continue;
        }
        if character == '{'
            && let Some((name, end)) = placeholder_at(&chars, at)
        {
            let Some(value) = bindings.get(&name) else {
                return Err(RenderError::Undeclared { name });
            };
            if value.needs_brackets() {
                out.push('(');
                out.push_str(&value.canonical_string());
                out.push(')');
            } else {
                out.push_str(&value.canonical_string());
            }
            at = end;
            continue;
        }
        return Err(RenderError::StrayBrace {
            index: at,
            snippet: snippet_from(&chars, at),
        });
    }
    Ok(out)
}

/// Every placeholder name the statement writes, and the first stray brace.
///
/// The scan reads the whole statement. It does not stop at the first stray
/// brace: it records that brace, steps over the one character, and goes on
/// collecting names. The gate of U2 needs both answers from one pass, because
/// 1.0 reports an undeclared parameter BEFORE it reports a stray brace
/// (`problem_templates.py:513-522`), and a scan that stopped at the brace could
/// not name an undeclared parameter that comes after it.
#[must_use]
pub fn scan(statement: &str) -> (BTreeSet<String>, Option<StrayBrace>) {
    let chars: Vec<char> = statement.chars().collect();
    let mut names = BTreeSet::new();
    let mut stray: Option<StrayBrace> = None;
    let mut at = 0_usize;
    while let Some(character) = chars.get(at).copied() {
        if character != '{' && character != '}' {
            at += 1;
            continue;
        }
        if chars.get(at + 1).copied() == Some(character) {
            at += 2;
            continue;
        }
        if character == '{'
            && let Some((name, end)) = placeholder_at(&chars, at)
        {
            names.insert(name);
            at = end;
            continue;
        }
        if stray.is_none() {
            stray = Some(StrayBrace {
                index: at,
                snippet: snippet_from(&chars, at),
            });
        }
        at += 1;
    }
    (names, stray)
}

/// Every placeholder name the statement writes, in name order.
///
/// # Errors
///
/// Returns [`RenderError::StrayBrace`] for the first brace that is neither
/// doubled nor part of a placeholder.
pub fn placeholders(statement: &str) -> Result<BTreeSet<String>, RenderError> {
    match scan(statement) {
        (names, None) => Ok(names),
        (_, Some(StrayBrace { index, snippet })) => Err(RenderError::StrayBrace { index, snippet }),
    }
}

/// The first brace that is neither doubled nor part of a placeholder.
///
/// This is 1.0 `_stray_brace` (`problem_templates.py:231-259`), and the gate of
/// U2 turns the report into the rejection message 1.0 writes.
#[must_use]
pub fn stray_brace(statement: &str) -> Option<StrayBrace> {
    scan(statement).1
}

/// Read a `{name}` placeholder that starts at `at`, and the index after it.
///
/// The name grammar is 1.0 `_PLACEHOLDER_RE`: one leading letter or underscore,
/// then letters, digits, and underscores. There is no format spec, no
/// conversion, no attribute access, and no index.
fn placeholder_at(chars: &[char], at: usize) -> Option<(String, usize)> {
    let first = chars.get(at + 1).copied()?;
    if !first.is_ascii_alphabetic() && first != '_' {
        return None;
    }
    let mut name = String::new();
    name.push(first);
    let mut index = at + 2;
    while let Some(character) = chars.get(index).copied() {
        if character.is_ascii_alphanumeric() || character == '_' {
            name.push(character);
            index += 1;
            continue;
        }
        break;
    }
    if chars.get(index).copied() != Some('}') {
        return None;
    }
    Some((name, index + 1))
}

/// The [`SNIPPET_CHARS`] characters that start at `at`.
fn snippet_from(chars: &[char], at: usize) -> String {
    chars.iter().skip(at).take(SNIPPET_CHARS).collect()
}
