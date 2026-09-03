//! Readers of a lint result, as the canonical text the oracle comparison
//! reads and the literal lists the lint tests compare.

use std::collections::BTreeMap;

use cadus_core::curriculum::Finding;

use super::paths::lint_fixture;

/// The tail that replaces a YAML library's exception text on both sides.
pub const YAML_TAIL: &str = "<yaml parser message>";

/// The findings as canonical JSON: sorted keys, two-space indent, one trailing
/// newline. This is the format of `json.dump(..., sort_keys=True, indent=2)`
/// followed by `print()`, which is what the dumper writes.
#[must_use]
pub fn canonical(findings: &[Finding]) -> String {
    let rows: Vec<BTreeMap<String, serde_json::Value>> = findings
        .iter()
        .map(|finding| {
            let value = serde_json::to_value(finding).expect("a finding serializes");
            let serde_json::Value::Object(map) = value else {
                panic!("a finding serializes to an object");
            };
            let mut row: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            for (key, value) in map {
                let value = if key == "message" && finding.code == "yaml" {
                    let head = finding
                        .message
                        .split_once(": ")
                        .map_or(finding.message.clone(), |(head, _)| head.to_owned());
                    serde_json::Value::String(format!("{head}: {YAML_TAIL}"))
                } else {
                    value
                };
                row.insert(key, value);
            }
            row
        })
        .collect();
    let mut text = serde_json::to_string_pretty(&rows).expect("the rows serialize");
    text.push('\n');
    text
}

/// The committed 1.0 output for one fixture.
#[must_use]
pub fn expected(name: &str) -> String {
    let path = lint_fixture(name).join("expected.json");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The codes of a finding list, in order.
#[must_use]
pub fn codes(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.code.as_str()).collect()
}
/// The messages of a finding list, in order.
#[must_use]
pub fn messages(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.message.as_str()).collect()
}
