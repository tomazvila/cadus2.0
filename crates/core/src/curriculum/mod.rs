//! The curriculum: data types, the YAML loader, and the findings they report
//! (C5, D2).
//!
//! The authority is `docs/reference/curriculum-1.0-spec.md`. Persisted ids are
//! strings here; the arena interns them to indices (D2).

pub mod arena;
pub mod dump;
pub mod finding;
pub mod graph;
pub mod lint;
pub mod load;
pub mod model;

pub use arena::{
    Curriculum, CurriculumError, EncLink, EncNode, KpIdx, LoadError, TopicIdx, load_curriculum,
};
pub use dump::{DUMP_SCHEMA, canonical_dump, curriculum_hash, python_repr_f64, sha256_hex};
pub use finding::Finding;
pub use graph::{Csr, EncCsr, EncEdge};
pub use lint::lint_curriculum;
pub use load::{
    ParseError, Parsed, ParsedUnit, RawCurriculum, RawTopic, RawUnit, load_raw_curriculum,
    parse_curriculum,
};
pub use model::{
    ANKI_TYPES, ANSWER_KINDS, AnkiSeed, AnkiType, AnswerKind, Catalog, Course, Exemplar,
    KnowledgePoint, PrereqEdge, Slug, SlugError, Topic, Unit,
};
