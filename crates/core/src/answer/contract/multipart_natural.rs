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
        let options: Vec<Vec<String>> = pieces.iter().map(|piece| readings(piece)).collect();
        // For each piece, the readings that fit each part.
        let fits = |piece: usize, part: usize| -> Vec<&String> {
            // A word that is another authored part name binds the piece to that part.
            let named = prefix_name(pieces[piece])
                .filter(|name| parts.iter().any(|other| other.name == *name));
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
/// `word =` or `word:`, and each of those without a count noun.
fn readings(piece: &str) -> Vec<String> {
    let mut texts = vec![piece.trim().to_owned()];
    if let Some(value) = strip_word_prefix(piece) {
        texts.push(value);
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
    prefix_name(piece).and_then(|_| (!value.is_empty()).then(|| value.to_owned()))
}

/// The word before a leading `=` or `:`, when it is one to three plain words
/// and the value after it is not empty and holds no second `=` or `:`.
fn prefix_name(piece: &str) -> Option<&str> {
    let at = piece.find(['=', ':'])?;
    let (name, value) = (piece[..at].trim(), piece[at + 1..].trim());
    let words = name.split_whitespace().count();
    let plain = name
        .chars()
        .all(|ch| ch.is_alphabetic() || ch == '_' || ch == ' ');
    (plain && (1..=3).contains(&words) && !value.is_empty() && !value.contains(['=', ':']))
        .then_some(name)
}
