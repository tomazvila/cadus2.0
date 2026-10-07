//! The figure rules of the full lint: a figure belongs to the item that names it.
//!
//! A knowledge point lists its figures in `visuals`. An exemplar shows one only
//! when it carries `visual: <index>`. These rules find the three ways an item and
//! its figure disagree:
//!
//! - `visual_mismatch`: the figure names a label or a value that the item never
//!   states in its problem, answer or solution sketch.
//! - `visual_letter_clash`: a figure label reuses a letter that the problem
//!   already uses as a quantity (`A` for an area, `h` for a height).
//! - `visual_missing`: the problem points at "the diagram" or "the figure", but
//!   the item names no figure.
//!
//! Every finding carries the context `[kp, exemplar index, visual index or
//! "none"]` so a sweep can list the items in a table.

use super::super::super::finding::Finding;
use super::super::super::model::{Exemplar, KnowledgePoint};
use super::super::Lint;

/// Phrases that point a learner at a drawing. Matched on lowercase text.
const POINTING: &[&str] = &[
    "diagram shows",
    "figure shows",
    "in the figure",
    "in the diagram",
    "in the picture",
    "pictured",
    "number line shown",
    "graph shown",
    "shown above",
    "shown below",
    "drawn above",
    "drawn below",
];

/// Letters that name a quantity more often than a point.
const QUANTITY_LETTERS: &[&str] = &[
    "A", "P", "V", "C", "S", "b", "h", "l", "w", "r", "d", "x", "y",
];

/// Words that introduce a quantity letter ("area $A$").
const QUANTITY_WORDS: &[&str] = &[
    "area",
    "perimeter",
    "volume",
    "circumference",
    "radius",
    "diameter",
    "height",
    "width",
    "length",
    "base",
];

pub(in crate::curriculum::lint) fn check_visuals(lint: &mut Lint<'_>) {
    for position in 0..lint.table.topics.len() {
        let topic = lint.table.topics[position];
        let file = lint.table.file(position).to_owned();
        for kp in &topic.knowledge_points {
            for (at, exemplar) in kp.exemplars.iter().enumerate() {
                check_item(
                    &mut lint.findings,
                    (topic.id.as_str(), &file),
                    kp.id.as_str(),
                    Some(kp),
                    at,
                    exemplar,
                );
            }
        }
        if let Some(dx) = &topic.diagnostic_exemplar {
            check_item(
                &mut lint.findings,
                (topic.id.as_str(), &file),
                "diagnostic",
                None,
                0,
                dx,
            );
        }
    }
}

fn check_item(
    findings: &mut Vec<Finding>,
    (topic_id, file): (&str, &str),
    kp_id: &str,
    kp: Option<&KnowledgePoint>,
    at: usize,
    exemplar: &Exemplar,
) {
    let named = match (exemplar.visual, kp) {
        (Some(index), Some(kp)) => kp.visuals.get(index).map(|spec| (index, spec)),
        _ => None,
    };
    let context = vec![
        kp_id.to_owned(),
        at.to_string(),
        exemplar
            .visual
            .map_or_else(|| "none".to_owned(), |index| index.to_string()),
    ];
    let mut report = |code: &str, reason: String| {
        findings.push(
            Finding::new(code, format!("{topic_id}/{kp_id} exemplar {at}: {reason}"))
                .with_topic(topic_id)
                .with_file(file)
                .with_context(context.clone()),
        );
    };
    let Some((_, spec)) = named else {
        if exemplar.visual.is_none() && points_at_a_drawing(&exemplar.problem) {
            report(
                "visual_missing",
                "the problem points at a drawing, but the item names no visual".to_owned(),
            );
        }
        if exemplar.visual.is_some() && kp.is_none() {
            report(
                "visual_missing",
                "the diagnostic exemplar names a visual, but it has no knowledge point".to_owned(),
            );
        }
        return;
    };
    let text = format!(
        "{} {} {}",
        exemplar.problem,
        exemplar.answer,
        exemplar.solution_sketch.as_deref().unwrap_or("")
    );
    let terms = spec.item_terms();
    let numbers = number_tokens(&text);
    let words = letter_tokens(&text);
    let mut absent: Vec<String> = Vec::new();
    for label in &terms.labels {
        let digits = number_tokens(label);
        if digits.is_empty() {
            if !label_appears(label, &words) {
                absent.push(label.clone());
            }
        } else if !digits.iter().all(|number| numbers.contains(number)) {
            absent.push(label.clone());
        }
    }
    for value in &terms.values {
        let digits = number_tokens(value);
        if !digits.iter().all(|number| numbers.contains(number)) {
            absent.push(value.clone());
        }
    }
    if !absent.is_empty() {
        report(
            "visual_mismatch",
            format!(
                "the figure names {} that the item never states",
                absent.join(", ")
            ),
        );
    }
    for label in &terms.labels {
        if let Some(letter) = clashing_letter(label, &exemplar.problem) {
            report(
                "visual_letter_clash",
                format!("the figure label {letter} is also a quantity in the problem"),
            );
        }
    }
}

fn points_at_a_drawing(problem: &str) -> bool {
    let lower = problem.to_lowercase();
    POINTING.iter().any(|phrase| lower.contains(phrase))
}

/// Every unsigned number in `text`, as written without a leading zero run.
fn number_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let joins = c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if c.is_ascii_digit() || (joins && !current.is_empty()) {
            current.push(c);
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// The alphabetic runs of `text` outside of `\command` names.
fn letter_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_command = false;
    for c in text.chars() {
        if c == '\\' {
            in_command = true;
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else if c.is_alphabetic() {
            if !in_command {
                current.push(c);
            }
        } else {
            in_command = false;
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Whether the problem names the label: as its own token, or as one capital
/// inside a run of capitals such as `ABC` or `AB`.
fn label_appears(label: &str, words: &[String]) -> bool {
    let name: String = label.chars().take_while(|c| c.is_alphabetic()).collect();
    if name.is_empty() {
        return true;
    }
    words.iter().any(|word| {
        *word == name
            || (name.chars().count() == 1
                && word.chars().count() <= 4
                && word.chars().all(char::is_uppercase)
                && word.contains(&name))
    })
}

/// A quantity letter the figure label reuses, when the problem uses it as a
/// quantity and never as a point.
fn clashing_letter<'a>(label: &'a str, problem: &str) -> Option<&'a str> {
    let name: String = label.chars().take_while(|c| c.is_alphabetic()).collect();
    let letter = QUANTITY_LETTERS.iter().find(|letter| **letter == name)?;
    let words = letter_tokens(problem);
    let lower = problem.to_ascii_lowercase();
    let wanted = letter.to_ascii_lowercase();
    let words_lower: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let as_point = words_lower.windows(2).any(|pair| {
        ["point", "points", "vertex", "vertices"].contains(&pair[0]) && pair[1] == wanted
    });
    if as_point || !words.iter().any(|word| word == letter) {
        return None;
    }
    let compact: String = problem.chars().filter(|c| !c.is_whitespace()).collect();
    let assigned = compact.contains(&format!("{letter}="));
    let introduced = QUANTITY_WORDS.iter().any(|word| {
        lower.match_indices(word).any(|(at, _)| {
            let rest = problem[at + word.len()..].trim_start_matches([' ', '$', '(']);
            rest.strip_prefix(letter)
                .is_some_and(|after| !after.starts_with(char::is_alphabetic))
        })
    });
    (assigned || introduced).then_some(label)
}
