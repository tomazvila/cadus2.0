//! The curriculum: data types, the YAML loader, and the findings they report
//! (C5, D2).
//!
//! The authority is `docs/reference/curriculum-1.0-spec.md`. Persisted ids are
//! strings here; the arena of U2 interns them to indices (D2).

pub mod finding;
pub mod load;
pub mod model;

pub use finding::Finding;
pub use load::{
    ParseError, Parsed, ParsedUnit, RawCurriculum, RawTopic, RawUnit, load_raw_curriculum,
    parse_curriculum,
};
pub use model::{
    ANKI_TYPES, ANSWER_KINDS, AnkiSeed, AnkiType, AnswerKind, Catalog, Course, Exemplar,
    KnowledgePoint, PrereqEdge, Slug, SlugError, Topic, Unit,
};
