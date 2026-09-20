//! The rules for new and changed exemplars: I12 (answer format), I13 (sketch)
//! and I14 (explicit contract). Also the pack rule D28 for each `function`
//! key: no `log` name and no number in `e` notation.

use serde_json::Value;

use super::super::kp_view::{Item, KpView};
use super::{finding, label};
use crate::output::Finding;

const ANSWER_CHARS: usize = 40;
const LONG_ANSWER_CHARS: usize = 80;
const SKETCH_WORDS: usize = 12;

/// The `name = value` fields of a `multipart` key, in key order.
fn key_fields(answer: &str) -> Vec<(&str, &str)> {
    answer
        .split(';')
        .filter_map(|field| field.split_once('='))
        .map(|(name, value)| (name.trim(), value.trim()))
        .collect()
}

fn plain_breach(what: &str, text: &str, limit: usize) -> Option<String> {
    let count = text.chars().count();
    if text.contains(['$', '\\']) {
        Some(format!("{what} has `$` or a backslash"))
    } else {
        (count > limit).then(|| format!("{what} has {count} characters (limit {limit})"))
    }
}

/// I12 with content-spec X1: the limits by contract kind.
fn i12(item: &Item) -> Option<String> {
    let answer = item.exemplar.answer.as_str();
    match item.kind() {
        Some("label") => {
            let count = answer.chars().count();
            (count > LONG_ANSWER_CHARS)
                .then(|| format!("the label key has {count} characters (limit 80)"))
        }
        Some("multipart") => plain_breach("the answer", answer, LONG_ANSWER_CHARS).or_else(|| {
            key_fields(answer).into_iter().find_map(|(name, value)| {
                plain_breach(&format!("the part `{name}`"), value, ANSWER_CHARS)
            })
        }),
        _ => plain_breach("the answer", answer, ANSWER_CHARS),
    }
}

/// The sentences of a sketch: a `.`, `!` or `?` before a space or the end.
fn sentence_count(sketch: &str) -> usize {
    let text = sketch.trim();
    let mut marks = text.char_indices().filter(|(at, ch)| {
        let next = text[at + ch.len_utf8()..].chars().next();
        matches!(ch, '.' | '!' | '?') && next.is_none_or(char::is_whitespace)
    });
    let ends = marks.by_ref().count();
    let open_end = !text.ends_with(['.', '!', '?']) && !text.is_empty();
    ends + usize::from(open_end)
}

fn digit_runs(key: &str) -> Vec<&str> {
    key.split(|ch: char| !ch.is_ascii_digit())
        .filter(|run| !run.is_empty())
        .collect()
}

/// I13 with content-spec X11: words, steps, and the digits of the key.
fn i13(item: &Item) -> Option<String> {
    let sketch = item.exemplar.solution_sketch.as_deref().unwrap_or("");
    let words = sketch.split_whitespace().count();
    let steps = sketch
        .split(';')
        .filter(|step| !step.trim().is_empty())
        .count();
    if words < SKETCH_WORDS {
        return Some(format!(
            "the sketch has {words} words (minimum {SKETCH_WORDS})"
        ));
    }
    if sentence_count(sketch) < 2 && steps < 2 {
        return Some("the sketch has fewer than 2 sentences or `;` steps".to_owned());
    }
    if matches!(item.kind(), Some("label" | "none")) {
        return None;
    }
    digit_runs(&item.exemplar.answer)
        .into_iter()
        .find(|run| !sketch.contains(run))
        .map(|run| format!("the sketch does not show the digits `{run}` of the key"))
}

/// True if the text has the name `log` (`log(x)`, `log10(x)`, `2log x`).
fn has_log(key: &str) -> bool {
    key.match_indices("log").any(|(at, _)| {
        let before = key[..at].chars().next_back();
        let after = key[at + 3..].chars().next();
        !before.is_some_and(char::is_alphabetic) && !after.is_some_and(char::is_alphabetic)
    })
}

/// True if the text has a number in `e` notation (`1e-5`, `2.5E3`).
fn has_e_notation(key: &str) -> bool {
    let chars: Vec<char> = key.chars().collect();
    chars.windows(3).enumerate().any(|(at, window)| {
        let sign = matches!(window[2], '-' | '+');
        let exponent = if sign {
            chars.get(at + 3)
        } else {
            Some(&window[2])
        };
        window[0].is_ascii_digit()
            && matches!(window[1], 'e' | 'E')
            && exponent.is_some_and(char::is_ascii_digit)
    })
}

fn d28_text(what: &str, key: &str) -> Option<String> {
    if has_log(key) {
        Some(format!(
            "D28: {what} has the name `log`; write `ln(x)` or `ln(x)/ln(10)`"
        ))
    } else {
        has_e_notation(key)
            .then(|| format!("D28: {what} has a number in `e` notation; write `10^(-5)`"))
    }
}

/// D28 for a top-level `function` key and for each `function` part.
pub fn d28(contract: &Value, answer: &str) -> Option<String> {
    if contract["kind"] == "function" {
        return d28_text("the function key", answer);
    }
    label::parts(contract)
        .into_iter()
        .zip(key_fields(answer))
        .filter(|((_, part), _)| part["kind"] == "function")
        .find_map(|((name, _), (_, value))| d28_text(&format!("the function part `{name}`"), value))
}

fn new_item_findings(view: &KpView, item: &Item) -> Vec<Finding> {
    let i14 = (item.verdict && item.contract.is_null())
        .then(|| "the verdict exemplar has no explicit answer_contract".to_owned());
    [
        ("answer-format", "I12", i12(item)),
        ("sketch", "I13", i13(item)),
        ("invariant:I14", "I14", i14),
    ]
    .into_iter()
    .filter_map(|(code, rule, text)| {
        Some(finding(
            view,
            code,
            rule,
            Some(item),
            format!("{rule}: {}", text?),
        ))
    })
    .collect()
}

/// The findings of I12, I13, I14 (new items) and D28 (each item).
pub fn check(view: &KpView) -> Vec<Finding> {
    view.items
        .iter()
        .flat_map(|item| {
            let mut found = if item.is_new {
                new_item_findings(view, item)
            } else {
                Vec::new()
            };
            let d28 = d28(&item.contract, &item.exemplar.answer);
            found.extend(d28.map(|text| finding(view, "answer-format", "D28", Some(item), text)));
            found
        })
        .collect()
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
