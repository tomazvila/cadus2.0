//! The fixture texts that two crates read.

use std::path::Path;

use serde::de::DeserializeOwned;

use crate::stop_on;

/// The reviewer's template of the M4 re-check: 10,000 declared tuples, one of
/// which breaks the envelope (C4, C6; M4 review rounds 1 and 2).
///
/// The declared space is above `EXHAUSTIVE_SPACE_LIMIT`, so the gate reads a
/// 4,096-tuple sample from its constant seed and never meets `a = 100, b = 100`.
/// That tuple answers `-1`, and every authored answer of the knowledge point is a
/// non-negative whole number, so the envelope refuses that instance.
pub const BIG_SUBTRACTION_BODY: &str = r#"{
  "v": 1,
  "topic_id": "big-subtraction",
  "answer_kind": "numeric",
  "statement": "Compute $9999 - {a} \\times {b}$.",
  "params": {"a": {"kind": "int", "low": 1, "high": 100},
             "b": {"kind": "int", "low": 1, "high": 100}},
  "answer_expr": "9999 - a*b",
  "hints": ["What is the product first?"],
  "samples": [{"params": {"a": 1, "b": 1}, "expected": "9998"},
              {"params": {"a": 100, "b": 1}, "expected": "9899"},
              {"params": {"a": 1, "b": 100}, "expected": "9899"}]
}"#;

/// The insert of one approved `content_store` template row (C6).
///
/// The three bind parameters are the digest, the serving key, and the template
/// body, in that order. The row lands approved, so a reader that filters on
/// `status` finds it.
pub const INSERT_APPROVED_TEMPLATE: &str =
    "INSERT INTO content_store (digest, kp_id, kind, body, status, approved_at)
     VALUES ($1, $2, 'template', $3::text::jsonb, 'approved', now())";

/// Read a JSON Lines file: one document of `T` per line, in file order.
pub fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Vec<T> {
    let text = stop_on(
        format!("read {}", path.display()),
        std::fs::read_to_string(path),
    );
    text.lines()
        .map(|line| stop_on(format!("row {line}"), serde_json::from_str(line)))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::Value;

    use super::{BIG_SUBTRACTION_BODY, read_jsonl};

    #[test]
    fn the_template_body_is_a_document() {
        let doc: Value = serde_json::from_str(BIG_SUBTRACTION_BODY).unwrap();
        assert_eq!(doc["topic_id"], "big-subtraction");
    }

    /// The 1.0 answer corpus of the core reads as one document per line.
    #[test]
    fn the_corpus_reads_one_document_per_line() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../core/tests/fixtures/answers/corpus_1_0.jsonl");
        let rows: Vec<Value> = read_jsonl(&path);
        assert_eq!(rows.len(), 3_492);
        assert!(rows[0].is_object());
    }

    #[test]
    #[should_panic(expected = "row not json")]
    fn a_line_that_is_not_json_stops_the_read() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/testkit-bad.jsonl");
        std::fs::write(&path, "not json\n").unwrap();
        let _: Vec<Value> = read_jsonl(&path);
    }
}
