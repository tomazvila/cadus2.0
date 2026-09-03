//! The 1.0 answer corpus, read from the fixture of the core crate so that one
//! file serves every milestone and the readers cannot drift apart.

use std::path::Path;

use cadus_core::curriculum::AnswerKind;

/// How many answers the 1.0 corpus holds (`crates/core/tests/answer_check.rs`).
pub const CORPUS_ROWS: usize = 3_492;

/// One row of `crates/core/tests/fixtures/answers/corpus_1_0.jsonl`.
#[derive(serde::Deserialize)]
pub struct CorpusRow {
    /// The authored answer, as 1.0 wrote it.
    pub answer: String,
    /// The answer kind of its topic.
    pub answer_kind: String,
}

/// Read the whole corpus, in file order.
pub fn corpus() -> Vec<CorpusRow> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures/answers/corpus_1_0.jsonl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|err| panic!("row {line}: {err}")))
        .collect()
}

/// The answer kind of a corpus row. The corpus holds verifiable kinds only.
pub fn kind_of(row: &CorpusRow) -> AnswerKind {
    match row.answer_kind.as_str() {
        "numeric" => AnswerKind::Numeric,
        "expression" => AnswerKind::Expression,
        other => panic!("the corpus holds only verifiable kinds, and this row is {other}"),
    }
}
