//! The natural spellings of a correct multipart answer.
//!
//! The strict reading wants the authored part names (`count = 3; type =
//! composite`) or the authored order (`3, composite`). A learner writes
//! `factors = 3, composite`, `composite, 3 factors` or `3 composite`. This
//! reading runs only after the strict reading fails to grade the answer
//! correct, so it can never turn a correct verdict into a wrong one.

use super::structured::{thousands_comma, top_level_commas};
use super::{AnswerPart, Canon, check_contract};
use crate::answer::Outcome;

/// Most parts for which every order is tried.
const MAX_PERMUTED: usize = 6;

/// Whether the natural reading grades the learner text correct for every part.
pub(super) fn correct(parts: &[AnswerPart], keys: &[&str], learner: &str) -> bool {
    let kinds: Vec<_> = parts
        .iter()
        .zip(keys)
        .map(|(part, key)| {
            part.contract
                .validate_expected(key)
                .ok()
                .map(|canon| std::mem::discriminant(&canon))
        })
        .collect();
    if kinds.iter().any(Option::is_none) || parts.len() > MAX_PERMUTED {
        return false;
    }
    pieces(parts, learner).iter().any(|pieces| {
        let options: Vec<Vec<String>> = pieces.iter().map(|piece| readings(piece, parts)).collect();
        // For each piece, the readings that fit each part.
        let fits = |piece: usize, part: usize| -> Vec<&String> {
            // A word that is another authored part name binds the piece to that part.
            let named = bound_name(pieces[piece], parts);
            if named.is_some_and(|name| name != parts[part].name) {
                return Vec::new();
            }
            options[piece]
                .iter()
                .filter(|text| fits_part(&parts[part], kinds[part], text))
                .collect()
        };
        let mut order: Vec<usize> = (0..parts.len()).collect();
        let mut found = None;
        permute(&mut order, 0, &mut |order| {
            if found.is_none() && (0..parts.len()).all(|at| !fits(order[at], at).is_empty()) {
                found = Some(order.to_vec());
            }
        });
        found.is_some_and(|order| {
            (0..parts.len()).all(|at| {
                fits(order[at], at).into_iter().any(|text| {
                    matches!(
                        check_contract(keys[at], text, parts[at].contract.clone()),
                        Outcome::Decided(verdict) if verdict.correct
                    )
                })
            })
        })
    })
}

/// Visit the orders of `items` with the identity order first.
fn permute(items: &mut Vec<usize>, from: usize, visit: &mut impl FnMut(&[usize])) {
    if from == items.len() {
        visit(items);
        return;
    }
    for at in from..items.len() {
        // Rotate so the lexicographic order starts with the identity.
        items[from..=at].rotate_right(1);
        permute(items, from + 1, visit);
        items[from..=at].rotate_left(1);
    }
}

/// Whether the text reads as a value of the part's own kind.
fn fits_part(part: &AnswerPart, kind: Option<std::mem::Discriminant<Canon>>, text: &str) -> bool {
    part.contract
        .validate_expected(text)
        .ok()
        .is_some_and(|canon| Some(std::mem::discriminant(&canon)) == kind)
}

/// The ways to cut the learner text into one piece per part.
fn pieces<'a>(parts: &[AnswerPart], text: &'a str) -> Vec<Vec<&'a str>> {
    let text = text.trim().trim_end_matches('.').trim();
    let count = parts.len();
    if text.is_empty() || thousands_comma(text) {
        return Vec::new();
    }
    let mut cuts = Vec::new();
    let semicolons: Vec<&str> = text.split(';').map(str::trim).collect();
    let commas = top_level_commas(text);
    let ands: Vec<&str> = text.split(" and ").map(str::trim).collect();
    for split in [&semicolons, &commas, &ands] {
        if split.len() == count && split.iter().all(|piece| !piece.is_empty()) {
            cuts.push(split.clone());
        }
    }
    if cuts.is_empty() && semicolons.len() == 1 && commas.len() == 1 && count == 2 {
        // One space between the two parts: the first word or the last word is one part.
        if let Some((head, tail)) = text.split_once(' ') {
            cuts.push(vec![head.trim(), tail.trim()]);
        }
        if let Some((head, tail)) = text.rsplit_once(' ') {
            cuts.push(vec![head.trim(), tail.trim()]);
        }
    }
    cuts
}

/// The texts one piece may stand for: the piece, the piece without a leading
/// `word =` or `word:` or authored part name, and each of those without a count noun.
fn readings(piece: &str, parts: &[AnswerPart]) -> Vec<String> {
    let mut texts = vec![piece.trim().to_owned()];
    if let Some(value) = strip_word_prefix(piece) {
        texts.push(value);
    }
    if let Some((_, value)) = lead_name(piece, parts) {
        texts.push(value.to_owned());
    }
    let counted: Vec<String> = texts
        .iter()
        .filter_map(|text| crate::answer::natural::count_answer("how many", "0", text))
        .collect();
    texts.extend(counted);
    texts
}

/// The value after a leading `word =` or `word:` (any one to three plain words).
fn strip_word_prefix(piece: &str) -> Option<String> {
    let at = piece.find(['=', ':'])?;
    let value = piece[at + 1..].trim();
    // A label with a digit (`11_percent`) names one thing, not a free word. If it is not an
    // authored part name, it is a different part, and the value must not be read as the
    // value of the authored one (grader pass 4, rule 2).
    prefix_name(piece)
        .filter(|name| !name.contains(|ch: char| ch.is_ascii_digit()))
        .and_then(|_| (!value.is_empty()).then(|| value.to_owned()))
}

/// The word before a leading `=` or `:`, when it is one to three plain words
/// and the value after it is not empty and holds no second `=` or `:`.
fn prefix_name(piece: &str) -> Option<&str> {
    let at = piece.find(['=', ':'])?;
    let (name, value) = (piece[..at].trim(), piece[at + 1..].trim());
    let words = name.split_whitespace().count();
    let plain = name
        .chars()
        .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == ' ');
    (plain && (1..=3).contains(&words) && !value.is_empty() && !value.contains(['=', ':']))
        .then_some(name)
}

/// The authored part name a piece is bound to, by `name =`, `name:` or `name value`.
fn bound_name<'p>(piece: &str, parts: &'p [AnswerPart]) -> Option<&'p str> {
    if let Some((at, _)) = lead_name(piece, parts) {
        return Some(parts[at].name.as_str());
    }
    let name = prefix_name(piece)?;
    parts
        .iter()
        .find(|part| part.name == name)
        .map(|part| part.name.as_str())
}

/// A piece that opens with an authored part name, as the part's position and the
/// text after the name: `center (-4,1)`, `amplitude 3`, `g(x) = x^2`, `a: 5`.
/// The longest name wins, and the name must end at a space, `=`, `:` or `(`.
fn lead_name<'a>(piece: &'a str, parts: &[AnswerPart]) -> Option<(usize, &'a str)> {
    let text = piece.trim();
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by_key(|&at| std::cmp::Reverse(parts[at].name.len()));
    order.into_iter().find_map(|at| {
        let name = parts[at].name.as_str();
        let head = text.get(..name.len())?;
        if !same_name(head, name) {
            return None;
        }
        let rest = text.get(name.len()..)?;
        let rest = match rest.chars().next()? {
            '(' if call_end(rest).is_some_and(|end| rest[end..].trim_start().starts_with('=')) => {
                &rest[call_end(rest)?..]
            }
            '(' | ' ' | '=' | ':' => rest,
            _ => return None,
        };
        let rest = rest.trim();
        let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest).trim();
        (!rest.is_empty()).then_some((at, rest))
    })
}

/// Whether a written name is the authored part name, ignoring case and
/// reading a hyphen or a space as the underscore (`x-intercept`, `x_intercept`).
fn same_name(written: &str, name: &str) -> bool {
    written.len() == name.len()
        && written
            .chars()
            .zip(name.chars())
            .all(|(a, b)| a.eq_ignore_ascii_case(&b) || (b == '_' && matches!(a, '-' | ' ')))
}

/// The offset just after the bracket pair that opens `text`.
fn call_end(text: &str) -> Option<usize> {
    let mut depth = 0_usize;
    for (at, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// The values of a tuple answer to a multipart question: `(6, -17)` or
/// `(a, b, c) = (1, 2, 3)`, read by position. The names before the `=` must be the
/// authored names in the authored order.
pub(super) fn tuple<'a>(parts: &[AnswerPart], text: &'a str) -> Option<Vec<&'a str>> {
    let mut text = text.trim();
    if let Some((names, values)) = text.split_once('=') {
        let names: Vec<&str> = names
            .trim()
            .strip_prefix('(')?
            .strip_suffix(')')?
            .split(',')
            .map(str::trim)
            .collect();
        if names.len() != parts.len()
            || names
                .iter()
                .zip(parts)
                .any(|(name, part)| *name != part.name)
        {
            return None;
        }
        text = values.trim();
    }
    let inner = text.strip_prefix('(')?.strip_suffix(')')?;
    // One bracket pair must enclose the whole text: `(1, 2), (3, 4)` is not a tuple.
    (call_end(text)? == text.len()).then_some(())?;
    super::structured::ordered_parts(parts, inner)
}
