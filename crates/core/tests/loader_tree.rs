//! U1 acceptance, part 5: the checked-in tree writes no YAML 1.1 form and no
//! numeric literal form 2.0 refuses (spec section 7, "2.0 strictness").
//!
//! The scans below are this file's own readers of the YAML text, so the
//! counts never come from the code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::path::{Path, PathBuf};

use common::paths::curriculum_root;

// --------------------------------------------------------------------------- //
// The checked-in tree writes no YAML 1.1 form
// --------------------------------------------------------------------------- //

/// Every `*.yaml` file under a directory, at any depth.
fn yaml_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).expect("the tree is readable") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "yaml")
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// The plain scalar one line writes, or `None` when the line writes a quoted
/// scalar, a block, a comment, or no value at all.
///
/// This walk is deliberately independent of the loader: it reads the bytes an
/// author wrote, so it sees the difference between `answer: no` and
/// `answer: "no"` that the parsed document no longer holds.
fn plain_scalar(line: &str) -> Option<&str> {
    let mut rest = line.trim();
    while let Some(tail) = rest.strip_prefix("- ") {
        rest = tail.trim_start();
    }
    if rest.starts_with('#') {
        return None;
    }
    let value = match rest.split_once(": ") {
        Some((_, value)) => value.trim(),
        None if rest.ends_with(':') => return None,
        None => rest,
    };
    if value.is_empty() || value.starts_with(['\'', '"', '|', '>', '&', '*', '#', '{', '[']) {
        return None;
    }
    Some(value)
}

/// True for a plain scalar that YAML 1.1 resolves to a boolean or an integer and
/// YAML 1.2 leaves as a string (spec section 7, "2.0 strictness").
fn is_yaml_1_1_form(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if matches!(lower.as_str(), "y" | "n" | "yes" | "no" | "on" | "off") {
        return true;
    }
    let digits = lower.trim_start_matches(['-', '+']);
    if digits.is_empty() {
        return false;
    }
    let octal = digits.len() > 1
        && digits.starts_with('0')
        && digits[1..].chars().all(|c| ('0'..='7').contains(&c));
    let underscored =
        digits.contains('_') && digits.chars().all(|c| c.is_ascii_digit() || c == '_');
    let sexagesimal =
        digits.contains(':') && digits.chars().all(|c| c.is_ascii_digit() || c == ':');
    octal || underscored || sexagesimal
}

#[test]
fn the_checked_in_tree_uses_no_yaml_1_1_form() {
    // The guard for the pinned choice of spec section 7, "2.0 strictness": the
    // tree must load the same way under 1.0 and under 2.0, so it may write no
    // form the two versions read differently. The positive control below runs
    // first, so a scan that stopped working cannot report a clean tree.
    for line in [
        "  core: yes",
        "  drill: Off",
        "  - N",
        "  expected_time_secs: 030",
        "  expected_time_secs: 1_200",
        "  expected_time_secs: 1:30",
    ] {
        let value = plain_scalar(line).expect("the line writes a plain scalar");
        assert!(is_yaml_1_1_form(value), "{line} writes a YAML 1.1 form");
    }
    for line in [
        "  answer: \"no\"",
        "  answer: 'yes'",
        "  core: true",
        "  expected_time_secs: 30",
        "  difficulty: 0.3",
        "  problem: |",
        "  # a comment",
    ] {
        let clean = plain_scalar(line).is_none_or(|value| !is_yaml_1_1_form(value));
        assert!(clean, "{line} writes no YAML 1.1 form");
    }

    let files = yaml_files(&curriculum_root());
    // Spec section 1: 88 unit files plus `courses.yaml`.
    assert_eq!(files.len(), 89, "YAML files in the tree");

    let mut hits: Vec<String> = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).expect("a unit file is readable");
        let name = path.display().to_string();
        assert!(!text.starts_with('\u{feff}'), "{name} starts with a BOM");
        for (index, line) in text.lines().enumerate() {
            let number = index + 1;
            if line.trim_start().starts_with("<<") {
                hits.push(format!("{name}:{number}: merge key"));
            }
            if let Some(value) = plain_scalar(line)
                && is_yaml_1_1_form(value)
            {
                hits.push(format!("{name}:{number}: {value}"));
            }
        }
    }
    assert_eq!(hits, Vec::<String>::new(), "YAML 1.1 forms in the tree");
}

/// True for the two numeric spellings 2.0 accepts: a plain decimal integer with
/// no leading zero, and a plain decimal float with an optional signed exponent.
///
/// This is a second, independent reading of the rule of spec section 7. It is
/// written against the text of the rule, not against the loader.
fn is_plain_decimal_form(value: &str) -> bool {
    let body = value.strip_prefix('-').unwrap_or(value);
    let digits = |text: &str| !text.is_empty() && text.chars().all(|c| c.is_ascii_digit());
    match body.split_once('.') {
        None => digits(body) && (body == "0" || !body.starts_with('0')),
        Some((whole, rest)) => {
            let (fraction, exponent) = match rest.split_once(['e', 'E']) {
                None => (rest, None),
                Some((fraction, exponent)) => (fraction, Some(exponent)),
            };
            let exponent_ok = match exponent {
                None => true,
                Some(exponent) => {
                    let signed = exponent.strip_prefix(['+', '-']);
                    signed.is_some_and(digits)
                }
            };
            digits(whole) && digits(fraction) && exponent_ok
        }
    }
}

#[test]
fn the_checked_in_tree_writes_only_plain_decimal_numbers() {
    // The guard for the numeric-literal rule of spec section 7, "2.0
    // strictness". Every other spelling of `order`, `difficulty`,
    // `expected_time_secs` and `weight` is a form the two YAML versions read
    // differently, or read as no number at all, so the tree may write none of
    // them. The positive control runs first, so a scan that stopped working
    // cannot report a clean tree.
    for form in ["1", "0", "-2", "0.3", "60.0", "1.5e-1", "-1.0e+30"] {
        assert!(is_plain_decimal_form(form), "{form} is a plain decimal");
    }
    for form in [
        "08", "060", "00", "1e3", "1.0e2", "0o17", "0b101", "0x1F", "1_000", "0.7_5", "1:30",
        ".nan", ".inf", "-.inf", "1.", ".5", "+30",
    ] {
        assert!(!is_plain_decimal_form(form), "{form} is no plain decimal");
    }

    const KEYS: [&str; 4] = ["order", "difficulty", "expected_time_secs", "weight"];
    let mut hits: Vec<String> = Vec::new();
    for path in yaml_files(&curriculum_root()) {
        let text = std::fs::read_to_string(&path).expect("a unit file is readable");
        let name = path.display().to_string();
        for (index, line) in text.lines().enumerate() {
            let line = line.split_once(" #").map_or(line, |(head, _)| head);
            let mut items: Vec<&str> =
                vec![line.trim_start().trim_start_matches("- ").trim_start()];
            if let Some(start) = line.find('{') {
                let flow = &line[start + 1..];
                let flow = flow.split_once('}').map_or(flow, |(head, _)| head);
                items.extend(flow.split(',').map(str::trim));
            }
            for item in items {
                for key in KEYS {
                    let Some(value) = item
                        .strip_prefix(key)
                        .and_then(|rest| rest.strip_prefix(':'))
                        .map(str::trim)
                    else {
                        continue;
                    };
                    if !is_plain_decimal_form(value) {
                        hits.push(format!("{name}:{}: {key}: {value}", index + 1));
                    }
                }
            }
        }
    }
    assert_eq!(hits, Vec::<String>::new(), "numeric forms in the tree");
}
