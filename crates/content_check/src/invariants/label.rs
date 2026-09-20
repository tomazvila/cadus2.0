//! The label rules of CK8: I8, I9, I11, the option quality rules, and the
//! pack rule D27 (label parts of a `multipart`).

use std::collections::BTreeSet;

use serde_json::Value;

use super::super::kp_view::{Item, KpView};
use super::finding;
use crate::output::Finding;

const FORBIDDEN: [&str; 3] = ["all of the above", "none of the above", "both a and b"];
/// The option texts of rule R16. They have no type and no length rule.
const R16_TOKENS: [&str; 3] = ["dne", "infinity", "-infinity"];

/// The text that the core compares for a label: one space between words, lower case.
fn choice_key(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The aliases of one option.
fn aliases(option: &Value) -> Vec<&str> {
    let list = option.as_array().into_iter().flatten();
    list.filter_map(Value::as_str).collect()
}

/// The alias lists of a `label` contract.
fn options(contract: &Value) -> Vec<Vec<&str>> {
    let list = contract["options"].as_array().into_iter().flatten();
    list.map(aliases).collect()
}

/// The display text of each option: its first alias.
fn display_texts(contract: &Value) -> Vec<&str> {
    options(contract)
        .iter()
        .filter_map(|aliases| aliases.first().copied())
        .collect()
}

/// The parts of a `multipart` contract: the name and the part contract.
pub fn parts(contract: &Value) -> Vec<(&str, &Value)> {
    contract["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|part| (part["name"].as_str().unwrap_or(""), &part["contract"]))
        .collect()
}

fn label_parts(contract: &Value) -> Vec<(&str, &Value)> {
    let mut all = parts(contract);
    all.retain(|(_, part)| part["kind"] == "label");
    all
}

/// True if the item has a top-level `label` contract.
pub fn is_label(item: &Item) -> bool {
    item.kind() == Some("label")
}

/// The index of the one option that has the key as an alias. `None` if no
/// option or more than one option has it.
pub fn correct_option(item: &Item) -> Option<usize> {
    let key = choice_key(&item.exemplar.answer);
    let all = options(&item.contract);
    let mut hits = all
        .iter()
        .enumerate()
        .filter(|(_, aliases)| aliases.iter().any(|alias| choice_key(alias) == key));
    let first = hits.next()?;
    hits.next().is_none().then_some(first.0)
}

/// I8: the option count of a top-level label, and the options of a label part.
pub fn option_count_breach(item: &Item) -> Option<String> {
    if is_label(item) {
        let count = options(&item.contract).len();
        return (count < 4).then(|| format!("label with {count} options (minimum 4)"));
    }
    label_parts(&item.contract)
        .into_iter()
        .find_map(|(name, part)| {
            let texts = display_texts(part);
            let long = texts
                .iter()
                .find(|text| text.split_whitespace().count() > 1);
            if texts.len() < 2 {
                Some(format!(
                    "label part `{name}` with {} options (minimum 2)",
                    texts.len()
                ))
            } else {
                long.map(|text| {
                    format!("label part `{name}`: the option `{text}` has more than 1 word")
                })
            }
        })
}

/// D27: a `multipart` has 1 or more parts that are not a `label`; a label part
/// is legal only with the rules R14 and R16 (a row item names its rule).
fn d27(item: &Item) -> Option<String> {
    let labels = label_parts(&item.contract).len();
    if labels == 0 {
        return None;
    }
    let rule = item.rule.as_deref();
    if labels == parts(&item.contract).len() {
        Some("D27: each part of the multipart is a label part".to_owned())
    } else {
        rule.filter(|rule| !matches!(*rule, "R14" | "R16"))
            .map(|rule| format!("D27: a label part is legal only with R14 and R16, not {rule}"))
    }
}

fn is_numeric(text: &str) -> bool {
    let body = text.trim().trim_start_matches('-');
    body.chars().any(|ch| ch.is_ascii_digit())
        && body
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '/'))
}

fn forbidden_text(contract: &Value) -> Option<String> {
    options(contract)
        .into_iter()
        .flatten()
        .find(|alias| FORBIDDEN.contains(&choice_key(alias).as_str()))
        .map(|alias| format!("the option text `{alias}` is forbidden"))
}

/// The type rule and the length rule. The R16 option texts are exempt.
fn shape_breach(texts: &[&str]) -> Option<String> {
    let plain: Vec<&str> = texts
        .iter()
        .copied()
        .filter(|text| !R16_TOKENS.contains(&choice_key(text).as_str()))
        .collect();
    let numeric = plain.iter().filter(|text| is_numeric(text)).count();
    if numeric != 0 && numeric != plain.len() {
        return Some("the options are not all numeric or all text".to_owned());
    }
    let lengths = plain.iter().map(|text| text.chars().count());
    let shortest = lengths.clone().min()?;
    let longest = lengths.max().unwrap_or(shortest);
    (longest > 3 * shortest)
        .then(|| format!("option lengths {shortest} to {longest}: the ratio is more than 3"))
}

/// An option text of more than 3 characters that the problem text shows.
/// Options of the form `Step <n>` are exempt (content-spec X12).
fn shown_in_problem(item: &Item, texts: &[&str]) -> Option<String> {
    let problem = item.exemplar.problem.to_lowercase();
    let is_step = |text: &str| {
        text.strip_prefix("Step ")
            .is_some_and(|n| !n.is_empty() && n.chars().all(|ch| ch.is_ascii_digit()))
    };
    texts
        .iter()
        .find(|text| {
            text.chars().count() > 3 && !is_step(text) && problem.contains(&text.to_lowercase())
        })
        .map(|text| format!("the problem text shows the option `{text}`"))
}

/// The quality rules of one top-level label item: the rule id and the text.
fn quality(item: &Item) -> Vec<(&'static str, String)> {
    let texts = display_texts(&item.contract);
    let one_key = correct_option(item)
        .is_none()
        .then(|| "the key is not an alias of exactly one option".to_owned());
    [
        ("I10", forbidden_text(&item.contract)),
        ("L4", shape_breach(&texts)),
        ("X1", one_key),
        ("X12", shown_in_problem(item, &texts)),
    ]
    .into_iter()
    .filter_map(|(rule, text)| Some((rule, text?)))
    .collect()
}

/// I9: a top-level label counts 2 half units, a label part counts 1.
fn i9(view: &KpView) -> Option<Finding> {
    let halves: usize = view
        .items
        .iter()
        .map(|item| {
            if is_label(item) {
                2
            } else {
                label_parts(&item.contract).len()
            }
        })
        .sum();
    let limit = if view.proof_kp { 6 } else { 4 };
    (halves > limit).then(|| {
        let half = if halves % 2 == 1 { ".5" } else { "" };
        let detail = format!(
            "I9: label count {}{half} (limit {}; a label part of a multipart counts 0.5)",
            halves / 2,
            limit / 2
        );
        finding(view, "label-quality", "I9", None, detail)
    })
}

/// I11: no correct option text two times among the label items of the KP.
fn i11(view: &KpView) -> Vec<Finding> {
    let mut seen = BTreeSet::new();
    view.items
        .iter()
        .filter(|item| is_label(item) && !seen.insert(choice_key(&item.exemplar.answer)))
        .map(|item| {
            let detail = format!(
                "I11: the correct option `{}` occurs twice",
                item.exemplar.answer
            );
            finding(view, "label-quality", "I11", Some(item), detail)
        })
        .collect()
}

fn item_findings(view: &KpView, item: &Item) -> Vec<Finding> {
    let mut found = Vec::new();
    if let Some(text) = option_count_breach(item) {
        found.push(finding(
            view,
            "label-quality",
            "I8",
            Some(item),
            format!("I8: {text}"),
        ));
    }
    if let Some(text) = d27(item) {
        found.push(finding(view, "contract-rule", "D27", Some(item), text));
    }
    let part_texts = label_parts(&item.contract)
        .into_iter()
        .filter_map(|(_, part)| Some(("I10", forbidden_text(part)?)));
    let own = if is_label(item) {
        quality(item)
    } else {
        Vec::new()
    };
    found.extend(own.into_iter().chain(part_texts).map(|(rule, text)| {
        finding(
            view,
            "label-quality",
            rule,
            Some(item),
            format!("{rule}: {text}"),
        )
    }));
    found
}

/// The findings of I8, I9, I11, the quality rules and D27.
pub fn check(view: &KpView) -> Vec<Finding> {
    let mut findings: Vec<Finding> = view
        .items
        .iter()
        .flat_map(|item| item_findings(view, item))
        .collect();
    findings.extend(i9(view));
    findings.extend(i11(view));
    findings
}

#[cfg(test)]
#[path = "label_tests.rs"]
mod tests;
