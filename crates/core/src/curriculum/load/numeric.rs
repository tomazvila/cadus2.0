//! The numeric-literal pre-scan of one file (spec section 7, "2.0 strictness").

use super::super::finding::Finding;

/// The four fields that hold a number (spec section 1).
const NUMERIC_KEYS: [&str; 4] = ["order", "difficulty", "expected_time_secs", "weight"];

/// The findings of the numeric-literal pre-scan of one file.
///
/// 2.0 accepts a plain decimal integer and a plain decimal float in the four
/// numeric fields, and refuses every other spelling (spec section 7, "2.0
/// strictness"). The two YAML versions read those other spellings differently,
/// or one of them reads no number at all, so a shared spelling is the only form
/// the two implementations agree on.
///
/// The scan reads the raw text, because the parsed `Value` holds the number and
/// not the spelling: `0x1F` and `31` arrive as the same `Value`. It reads one
/// line at a time, in the block form `difficulty: 0.3` and in the flow form
/// `{id: a, weight: 0.3}`. A value on a continuation line is not scanned; spec
/// section 7 documents the limit.
pub(super) fn numeric_form_findings(text: &str, rel: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for raw in numeric_values(line) {
            if !is_numeric_literal(raw) || is_plain_decimal(raw) {
                continue;
            }
            let message = format!(
                "numeric literal form '{raw}' is not accepted; write a plain decimal number"
            );
            out.push(
                Finding::new("schema", format!("{rel}:{}: {message}", index + 1)).with_file(rel),
            );
        }
    }
    out
}

/// Every value one line writes into a numeric field.
///
/// The block form is `^\s*(- )?<key>:\s*<value>\s*(#.*)?$`; the flow form is one
/// `<key>: <value>` item of a `{...}` mapping. A line writes at most one block
/// value, and any number of flow values.
fn numeric_values(line: &str) -> Vec<&str> {
    // YAML starts a comment at a `#` that follows a space, so `0.3 # note` is
    // the value `0.3` and `0.3#4` is not.
    let line = line.split_once(" #").map_or(line, |(head, _)| head);
    let mut out = Vec::new();
    let head = line.trim_start();
    let head = match head.strip_prefix("- ") {
        Some(rest) => rest.trim_start(),
        None => head,
    };
    if let Some(value) = numeric_item(head) {
        out.push(value.trim_end());
    }
    if let Some(start) = line.find('{') {
        let flow = &line[start + 1..];
        let flow = flow.split_once('}').map_or(flow, |(head, _)| head);
        for item in flow.split(',') {
            if let Some(value) = numeric_item(item.trim_start()) {
                out.push(value.trim_end());
            }
        }
    }
    out
}

/// The value of a `<key>: <value>` item, when the key is a numeric field.
fn numeric_item(item: &str) -> Option<&str> {
    for key in NUMERIC_KEYS {
        if let Some(rest) = item.strip_prefix(key)
            && let Some(value) = rest.strip_prefix(':')
        {
            return Some(value.trim_start());
        }
    }
    None
}

/// True for a value the author wrote as a number.
///
/// A number starts with a digit, a sign or a decimal point. Every other value —
/// a quoted scalar, a boolean word, a block indicator, plain text — belongs to
/// the type check, which reports it with the message of its own type.
fn is_numeric_literal(raw: &str) -> bool {
    matches!(raw.chars().next(), Some('-' | '+' | '.') | Some('0'..='9'))
}

/// True for the two spellings 2.0 accepts: a plain decimal integer `-?[0-9]+`
/// with no leading zero, and a plain decimal float `-?[0-9]+\.[0-9]+` with an
/// optional signed exponent. YAML 1.1 needs the decimal point and the sign of
/// the exponent, so `1e3` and `1.0e2` are not numbers to 1.0 at all.
fn is_plain_decimal(raw: &str) -> bool {
    let body = raw.strip_prefix('-').unwrap_or(raw);
    let (whole, rest) = split_digits(body);
    if whole.is_empty() {
        return false;
    }
    let Some(rest) = rest.strip_prefix('.') else {
        // An integer. A leading zero is an octal number to 1.0.
        return rest.is_empty() && (whole == "0" || !whole.starts_with('0'));
    };
    let (fraction, rest) = split_digits(rest);
    if fraction.is_empty() {
        return false;
    }
    if rest.is_empty() {
        return true;
    }
    let Some(rest) = rest.strip_prefix(['e', 'E']) else {
        return false;
    };
    let Some(rest) = rest.strip_prefix(['+', '-']) else {
        return false;
    };
    let (exponent, rest) = split_digits(rest);
    !exponent.is_empty() && rest.is_empty()
}

/// The leading ASCII digits of a text, and the rest of it.
fn split_digits(text: &str) -> (&str, &str) {
    let end = text
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(text.len());
    text.split_at(end)
}
