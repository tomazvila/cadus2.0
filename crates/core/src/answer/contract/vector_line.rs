//! A line in vector form: `(1, 2, 0) + t(-1, 1, 1)`, `t(-2, 1) + (4, 0)`, or the
//! named components `x = 4 - 2t, y = t`.
//!
//! Each reading gives one expression of the parameter for every component. Two
//! readings are the same when each component is the same polynomial in the
//! parameter, whatever letter names the parameter and in whatever order the
//! terms stand.

use super::structured::top_level_commas;
use super::{Canon, canonical_form};

/// The components of a vector line in the written form, or `None`.
fn components(text: &str) -> Option<Vec<Canon>> {
    let text = text.trim();
    if text.matches('=').count() > 0 {
        return named_components(text);
    }
    let mut sums: Vec<String> = Vec::new();
    for (negative, term) in terms(text)? {
        let (scalar, entries) = split_term(term)?;
        let scalar = rename_parameter(&scalar)?;
        if !sums.is_empty() && sums.len() != entries.len() {
            return None;
        }
        sums.resize(entries.len(), String::new());
        for (sum, entry) in sums.iter_mut().zip(entries) {
            let sign = if negative { "-" } else { "+" };
            sum.push_str(&format!("{sign}(1)*({scalar})*({entry})"));
        }
    }
    (sums.len() >= 2)
        .then(|| sums.iter().map(|sum| canonical_form(sum).ok()).collect())
        .flatten()
}

/// `x = 4 - 2t, y = t` as the components in the order x, y, z.
fn named_components(text: &str) -> Option<Vec<Canon>> {
    let pieces = top_level_commas(text);
    let mut found: Vec<(char, String)> = Vec::new();
    for piece in pieces {
        let (name, value) = piece.split_once('=')?;
        let mut letters = name.trim().chars();
        let (Some(letter), None) = (letters.next(), letters.next()) else {
            return None;
        };
        found.push((letter, rename_parameter(value.trim())?));
    }
    found.sort_by_key(|(letter, _)| *letter);
    let order = ['x', 'y', 'z'];
    let in_order = found.len() >= 2
        && found.len() <= 3
        && found
            .iter()
            .zip(order)
            .all(|((letter, _), want)| *letter == want);
    in_order
        .then(|| {
            found
                .iter()
                .map(|(_, value)| canonical_form(value).ok())
                .collect()
        })
        .flatten()
}

/// Split at each top-level `+` or `-` into signed terms.
fn terms(text: &str) -> Option<Vec<(bool, &str)>> {
    let mut out = Vec::new();
    let (mut depth, mut start, mut negative) = (0_i32, 0, false);
    for (at, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            '+' | '-' if depth == 0 && at > start => {
                out.push((negative, text[start..at].trim()));
                negative = ch == '-';
                start = at + 1;
            }
            '+' | '-' if depth == 0 => negative ^= ch == '-',
            _ => {}
        }
    }
    out.push((negative, text[start..].trim()));
    (depth == 0 && out.iter().all(|(_, term)| !term.is_empty())).then_some(out)
}

/// The scalar and the entries of one term: `t(-1, 1, 1)`, `(4, 0)`, `2(1, 2)t`.
fn split_term(term: &str) -> Option<(String, Vec<String>)> {
    let (scalar, group) = if term.ends_with(')') {
        let open = last_group_start(term)?;
        (term[..open].trim_end_matches(['*', ' ']), &term[open..])
    } else {
        let close = first_group_end(term)?;
        (term[close..].trim_start_matches(['*', ' ']), &term[..close])
    };
    let inner = group.strip_prefix('(')?.strip_suffix(')')?;
    let entries = top_level_commas(inner);
    if entries.len() < 2 {
        return None;
    }
    let scalar = if scalar.is_empty() { "1" } else { scalar };
    Some((
        scalar.to_owned(),
        entries.iter().map(|e| (*e).to_owned()).collect(),
    ))
}

fn last_group_start(text: &str) -> Option<usize> {
    let mut depth = 0_i32;
    for (at, ch) in text.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

fn first_group_end(text: &str) -> Option<usize> {
    if !text.starts_with('(') {
        return None;
    }
    let mut depth = 0_i32;
    for (at, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// The scalar with its one parameter letter written as `t`. A scalar with two
/// different letters, or a word, is not a parameter expression.
fn rename_parameter(scalar: &str) -> Option<String> {
    let mut letters = scalar.chars().filter(char::is_ascii_alphabetic);
    let first = letters.next();
    let same = letters.all(|ch| Some(ch) == first);
    let no_word = !scalar
        .split(|ch: char| !ch.is_ascii_alphabetic())
        .any(|run| run.len() > 1);
    (same && no_word && first != Some('e')).then(|| {
        scalar
            .chars()
            .map(|ch| if Some(ch) == first { 't' } else { ch })
            .collect()
    })
}

/// Whether both texts are vector lines with the same components.
pub(super) fn same_line(key: &str, learner: &str) -> bool {
    match (components(key), components(learner)) {
        (Some(key), Some(learner)) => key == learner,
        _ => false,
    }
}
