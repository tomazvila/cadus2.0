//! Shape and vocabulary checks for reviewed policies.

use std::collections::BTreeSet;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};

use super::{AnswerContract, AnswerPart, Canon, NumericForm, Undecidable, canonical_form};

pub(super) fn tolerance_value(text: &str) -> Result<BigRational, Undecidable> {
    if text.len() <= 80
        && let Ok(Canon::Rational(value)) = canonical_form(text)
        && value.is_positive()
    {
        return Ok(value);
    }
    Err(Undecidable::new(
        "the tolerance must be an exact positive rational of at most 80 characters",
    ))
}

/// Liquid and cubic volume use equal base magnitudes: one millilitre is one
/// cubic centimetre. Rebind that shared dimension inside a Unit contract while
/// preserving the legacy wire variants and global canonical representation.
pub(super) fn unit_compatible_value(contract: &AnswerContract, value: Canon) -> Canon {
    let AnswerContract::Unit { quantity, .. } = contract else {
        return value;
    };
    let Canon::Quantity {
        quantity: actual,
        value: magnitude,
    } = value
    else {
        return value;
    };
    let compatible_volumes = matches!(
        (actual, *quantity),
        (super::Quantity::Volume, super::Quantity::CubicVolume)
            | (super::Quantity::CubicVolume, super::Quantity::Volume)
    );
    if compatible_volumes || actual == *quantity {
        Canon::Quantity {
            quantity: *quantity,
            value: magnitude,
        }
    } else {
        Canon::Quantity {
            quantity: actual,
            value: magnitude,
        }
    }
}

pub(super) fn validate_shape(contract: &AnswerContract, value: &Canon) -> bool {
    match contract {
        AnswerContract::Approx { .. } => number(value),
        AnswerContract::Tolerance { .. } => matches!(value, Canon::Rational(_)),
        AnswerContract::RequiredForm { form } => match form {
            NumericForm::FactoredLinear
            | NumericForm::FactoredPolynomial
            | NumericForm::ExpandedPolynomial
            | NumericForm::StandardFormPolynomial => matches!(value, Canon::Poly(_)),
            NumericForm::SimplestRadicalSum => {
                matches!(value, Canon::Rational(_) | Canon::Radical(_))
            }
            NumericForm::RationalExponent | NumericForm::Radical => true,
            _ => matches!(value, Canon::Rational(_)),
        },
        AnswerContract::Unit { quantity, .. } => {
            matches!(value, Canon::Quantity { quantity: actual, .. } if actual == quantity)
        }
        AnswerContract::Coordinates { arity } => {
            matches!(value, Canon::Tuple(items) if items.len() == usize::from(*arity) && items.iter().all(number))
        }
        AnswerContract::Matrix { rows, cols } => matrix_shape(value, *rows, *cols),
        AnswerContract::QuotientRemainder { divisor } => quotient_shape(value, *divisor),
        AnswerContract::Set => matches!(value, Canon::Set(_)),
        _ => true,
    }
}

fn number(value: &Canon) -> bool {
    matches!(value, Canon::Rational(_) | Canon::Radical(_))
}

fn quotient_shape(value: &Canon, divisor: Option<u64>) -> bool {
    let Canon::Tuple(items) = value else {
        return false;
    };
    let [Canon::Rational(quotient), Canon::Rational(remainder)] = items.as_slice() else {
        return false;
    };
    quotient.is_integer()
        && remainder.is_integer()
        && remainder >= &BigRational::zero()
        && divisor.is_none_or(|value| remainder < &BigRational::from_integer(BigInt::from(value)))
}

/// The raw rows of a matrix answer: bracketed rows, or plain rows joined by `;`.
///
/// `[[1,2],[3,4]]` and `[1,2;3,4]` are one matrix. A structural failure is a
/// refusal; the caller owns the dimension rule and the entry rule.
pub(super) fn matrix_rows(text: &str) -> Result<Vec<Vec<&str>>, Undecidable> {
    let bad = || Undecidable::new("a matrix must be written as bracketed rows of entries");
    let text = text.trim();
    let inner = text
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .ok_or_else(bad)?;
    let mut rows = Vec::new();
    let mut rest = inner.trim();
    while !rest.is_empty() {
        if let Some(after_open) = rest.strip_prefix('[') {
            let close = after_open.find(']').ok_or_else(bad)?;
            let row = &after_open[..close];
            if row.contains(['[', ';']) {
                return Err(bad());
            }
            rows.push(row);
            rest = after_open[close + 1..].trim_start();
            rest = rest.strip_prefix([',', ';']).unwrap_or(rest).trim_start();
        } else {
            match rest.split_once(';') {
                Some((row, tail)) => {
                    rows.push(row.trim());
                    rest = tail.trim();
                }
                None => {
                    rows.push(rest);
                    rest = "";
                }
            }
        }
    }
    rows.iter()
        .map(|row| {
            let entries = row.split(',').map(str::trim).collect::<Vec<_>>();
            if entries.iter().any(|entry| entry.is_empty()) {
                return Err(Undecidable::new(
                    "a matrix row requires comma-separated entries",
                ));
            }
            Ok(entries)
        })
        .collect()
}

/// The matrix of `rows` by `cols` exact rational entries an answer names.
///
/// The grid reads as a list of rows, each row a list of exact rationals, so the
/// entry comparison is the exact equality of the numeric contracts.
pub(super) fn matrix_value(rows: u8, cols: u8, text: &str) -> Result<Canon, Undecidable> {
    let parsed = matrix_rows(text)?;
    let (rows, cols) = (usize::from(rows), usize::from(cols));
    let shaped = parsed.len() == rows && parsed.iter().all(|row| row.len() == cols);
    if !shaped {
        return Err(Undecidable::new(
            "a matrix answer requires the expected count of rows and entries per row",
        ));
    }
    let grid = parsed
        .iter()
        .map(|row| row.iter().map(|entry| matrix_entry(entry)).collect())
        .collect::<Result<Vec<Vec<Canon>>, Undecidable>>()?;
    Ok(Canon::List(grid.into_iter().map(Canon::List).collect()))
}

/// One exact rational or decimal entry of a matrix.
fn matrix_entry(text: &str) -> Result<Canon, Undecidable> {
    match canonical_form(text) {
        Ok(Canon::Rational(value)) => Ok(Canon::Rational(value)),
        _ => Err(Undecidable::new(
            "a matrix entry must be an exact rational or decimal",
        )),
    }
}

/// Whether the canonical form is the expected grid of exact rationals.
fn matrix_shape(value: &Canon, rows: u8, cols: u8) -> bool {
    let Canon::List(grid) = value else {
        return false;
    };
    grid.len() == usize::from(rows)
        && grid.iter().all(|row| {
            matches!(row, Canon::List(entries) if entries.len() == usize::from(cols) && entries.iter().all(|entry| matches!(entry, Canon::Rational(_))))
        })
}

fn choice_key(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub(super) fn validate_labels(options: &[Vec<String>]) -> Result<(), Undecidable> {
    let mut keys = BTreeSet::new();
    if options.is_empty() || options.len() > 32 {
        return Err(Undecidable::new(
            "a label contract requires one to 32 choices",
        ));
    }
    for option in options {
        if option.is_empty() || option.len() > 8 {
            return Err(Undecidable::new(
                "a choice requires one to eight explicit aliases",
            ));
        }
        for alias in option {
            let key = choice_key(alias);
            if key.is_empty() || alias.chars().count() > 80 || !keys.insert(key) {
                return Err(Undecidable::new(
                    "choice aliases must be bounded, nonempty, and unique",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn label_value(options: &[Vec<String>], text: &str) -> Option<Canon> {
    let key = choice_key(text);
    let exact = options
        .iter()
        .find(|aliases| aliases.iter().any(|alias| choice_key(alias) == key));
    exact
        .or_else(|| spoken_choice(options, text))
        .and_then(|aliases| aliases.first())
        .map(|alias| Canon::Label(choice_key(alias)))
}

/// The one option a spoken answer names, under [`spoken_key`].
///
/// The answer matches an alias after both are spoken-normalized, or it ends
/// with "is <alias>" / "are <alias>" and adds no negation the alias lacks ("the
/// bacterium is longer" names "longer"; "the bacterium is not longer" names
/// nothing). An answer that names two options names none.
fn spoken_choice<'a>(options: &'a [Vec<String>], text: &str) -> Option<&'a Vec<String>> {
    let spoken = spoken_key(text);
    if spoken.is_empty() {
        return None;
    }
    let negated = |words: &str| {
        words
            .split(' ')
            .any(|word| matches!(word, "not" | "no" | "never" | "isn't" | "aren't" | "nor"))
    };
    let names = |alias: &str| {
        let alias = spoken_key(alias);
        if alias.is_empty() {
            return false;
        }
        if spoken == alias {
            return true;
        }
        spoken.strip_suffix(alias.as_str()).is_some_and(|head| {
            (head.ends_with(" is ") || head.ends_with(" are ")) && !negated(head)
        })
    };
    let mut found = options
        .iter()
        .filter(|aliases| aliases.iter().any(|alias| names(alias)));
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

/// A choice spelling with the spoken variation removed: lower case, no
/// parentheses, no trailing punctuation, no leading "there are" / "there is" /
/// "the equation has", "roots" and "zeros" read as "solutions", the word
/// "real" dropped, a plural "solutions" read as "solution", and the empty-set
/// spellings `∅`, `{}` and "the empty set" read as "no solution".
fn spoken_key(text: &str) -> String {
    let mut key = choice_key(text).replace(['(', ')'], "");
    while let Some(stripped) = key
        .strip_suffix(['.', '!', '?', ';', ',', ':'])
        .map(str::trim_end)
    {
        key = stripped.to_owned();
    }
    for lead in [
        "there are ",
        "there is ",
        "the equation has ",
        "it has ",
        "the answer is ",
    ] {
        if let Some(rest) = key.strip_prefix(lead) {
            key = rest.to_owned();
        }
    }
    if matches!(
        key.as_str(),
        "∅" | "{}" | "{ }" | "the empty set" | "empty set"
    ) {
        return "no solution".to_owned();
    }
    key.split(' ')
        .filter(|word| !word.is_empty() && *word != "real")
        .map(|word| match word {
            "roots" | "root" | "zeros" | "zeroes" | "zero" | "solutions" => "solution",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn validate_parts(parts: &[AnswerPart]) -> Result<(), Undecidable> {
    if parts.is_empty() || parts.len() > 16 {
        return Err(Undecidable::new(
            "a multipart answer requires one to 16 parts",
        ));
    }
    let mut names = BTreeSet::new();
    for part in parts {
        if part.name.is_empty()
            || part.name.len() > 32
            || !part
                .name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            || !names.insert(&part.name)
        {
            return Err(Undecidable::new(
                "part names must be bounded unique identifiers",
            ));
        }
        if matches!(
            part.contract,
            AnswerContract::RequiredAssignment
                | AnswerContract::RequiredSimplestRadical
                | AnswerContract::Multipart { .. }
                | AnswerContract::None
        ) {
            return Err(Undecidable::new(
                "multipart parts require flat deterministic contracts",
            ));
        }
        part.contract.validate()?;
    }
    Ok(())
}

pub(super) fn named_parts<'a>(parts: &[AnswerPart], text: &'a str) -> Option<Vec<&'a str>> {
    // "a = 1; b = -3; c = 2" or, with every comma piece named, "a = 1, b = -3, c = 2".
    let pieces: Vec<&str> = if text.contains(';') {
        text.split(';').collect()
    } else {
        top_level_commas(text)
    };
    let fields: Vec<_> = pieces
        .into_iter()
        .map(|field| field.trim().split_once('='))
        .collect::<Option<_>>()?;
    if fields.len() != parts.len() {
        return None;
    }
    let mut names = BTreeSet::new();
    if fields.iter().any(|(name, _)| !names.insert(name.trim())) {
        return None;
    }
    parts
        .iter()
        .map(|part| {
            fields
                .iter()
                .find(|(name, _)| name.trim() == part.name)
                .map(|(_, value)| value.trim())
                .filter(|value| {
                    matches!(part.contract, AnswerContract::Label { .. })
                        || !matches!(canonical_form(value), Ok(Canon::Assign { .. }))
                })
        })
        .collect()
}

/// The learner's parts read in the key's order when no part carries a name.
///
/// A learner writes "6, composite" or "6; composite" for a two-part question.
/// The text splits on `;`, or else on commas outside brackets, and it reads
/// only when the piece count equals the part count and no piece is an
/// assignment. A comma that may group thousands ("1,000") makes the text
/// unreadable. A misread order gives a wrong verdict, which the background
/// equivalence check then reviews.
///
/// When the comma pieces outnumber the parts and exactly one part takes a
/// list, the surplus pieces belong to that list: `1, 2, 4, 8, 16, composite`
/// reads as the list `1, 2, 4, 8, 16` and the label `composite`.
pub(super) fn ordered_parts<'a>(parts: &[AnswerPart], text: &'a str) -> Option<Vec<&'a str>> {
    if let Some(pieces) = verdict_sentence(parts, text) {
        return Some(pieces);
    }
    let pieces: Vec<&str> = if text.contains(';') {
        text.split(';').map(str::trim).collect()
    } else if thousands_comma(text) {
        return None;
    } else {
        merge_into_list(parts, text, top_level_commas(text))
    };
    if pieces.len() != parts.len()
        || pieces
            .iter()
            .any(|piece| piece.is_empty() || piece.contains('='))
    {
        return None;
    }
    Some(pieces)
}

/// A verdict and its value in one sentence: "too high by 160" for a label part
/// followed by one value part. The text opens with an alias of the label (the
/// longest alias that fits, at a word boundary), then an optional "by", and the
/// rest is the value. A text with a comma or a semicolon keeps the plain split.
fn verdict_sentence<'a>(parts: &[AnswerPart], text: &'a str) -> Option<Vec<&'a str>> {
    let [first, second] = parts else {
        return None;
    };
    let AnswerContract::Label { options } = &first.contract else {
        return None;
    };
    let commas = text.matches(',').count();
    if matches!(second.contract, AnswerContract::Label { .. })
        || text.contains([';', '='])
        || commas > 1
    {
        return None;
    }
    let trimmed = text.trim();
    let mut aliases: Vec<&String> = options.iter().flatten().collect();
    aliases.sort_by_key(|alias| std::cmp::Reverse(alias.len()));
    aliases.into_iter().find_map(|alias| {
        let head = trimmed.get(..alias.len())?;
        if !head.eq_ignore_ascii_case(alias) {
            return None;
        }
        let rest = trimmed.get(alias.len()..)?;
        // One comma is allowed only between the label and "by": "too high, by 160".
        let (rest, comma) = match rest.strip_prefix(',') {
            Some(after) => (after, true),
            None => (rest, false),
        };
        if !rest.starts_with(' ') || commas != usize::from(comma) {
            return None;
        }
        let rest = rest.trim_start();
        let by = rest.strip_prefix("by ");
        if comma && by.is_none() {
            return None;
        }
        let value = by.unwrap_or(rest).trim();
        (!value.is_empty()).then(|| vec![head, value])
    })
}

/// Whether a comma sits between a digit and exactly three digits, as in "1,000".
fn thousands_comma(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.iter().enumerate().any(|(index, &byte)| {
        byte == b','
            && index
                .checked_sub(1)
                .and_then(|before| bytes.get(before))
                .is_some_and(u8::is_ascii_digit)
            && bytes
                .get(index + 1..index + 4)
                .is_some_and(|run| run.iter().all(u8::is_ascii_digit))
            && !bytes.get(index + 4).is_some_and(u8::is_ascii_digit)
    })
}

/// Join the surplus comma pieces into the one list part, or keep the pieces.
fn merge_into_list<'a>(parts: &[AnswerPart], text: &'a str, pieces: Vec<&'a str>) -> Vec<&'a str> {
    let lists: Vec<usize> = parts
        .iter()
        .enumerate()
        .filter(|(_, part)| matches!(part.contract, AnswerContract::List { .. }))
        .map(|(at, _)| at)
        .collect();
    let ([list], Some(surplus)) = (lists.as_slice(), pieces.len().checked_sub(parts.len())) else {
        return pieces;
    };
    if surplus == 0 {
        return pieces;
    }
    let (Some(first), Some(last)) = (pieces.get(*list), pieces.get(list + surplus)) else {
        return pieces;
    };
    // Every piece is a slice of `text`, so the offsets name one span of it.
    let start = first.as_ptr() as usize - text.as_ptr() as usize;
    let end = last.as_ptr() as usize - text.as_ptr() as usize + last.len();
    let Some(joined) = text.get(start..end) else {
        return pieces;
    };
    let mut merged = pieces[..*list].to_vec();
    merged.push(joined);
    merged.extend_from_slice(&pieces[list + surplus + 1..]);
    merged
}

/// Split on commas at bracket depth zero.
fn top_level_commas(text: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let (mut depth, mut start) = (0_i32, 0);
    for (index, ch) in text.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                pieces.push(text[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    pieces.push(text[start..].trim());
    pieces
}

pub(super) fn multipart_values(parts: &[AnswerPart], text: &str) -> Result<Canon, Undecidable> {
    let values = named_parts(parts, text)
        .ok_or_else(|| Undecidable::new("each named answer part must occur exactly once"))?;
    parts
        .iter()
        .zip(values)
        .map(|(part, value)| {
            part.contract
                .validate_expected(value)
                .map(|value| Canon::Assign {
                    var: part.name.clone(),
                    value: Box::new(value),
                })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Canon::Tuple)
}
