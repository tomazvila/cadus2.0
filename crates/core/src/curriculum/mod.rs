//! The curriculum: data types, the YAML loader, and the findings they report
//! (C5, D2).
//!
//! The authority is `docs/reference/curriculum-1.0-spec.md`. Persisted ids are
//! strings here; the arena interns them to indices (D2).

pub mod arena;
pub mod dump;
pub mod finding;
pub mod finite;
pub mod graph;
pub mod lint;
pub mod load;
pub mod model;

pub use arena::{
    Curriculum, CurriculumError, EncLink, EncNode, KpIdx, LoadError, TopicIdx, load_curriculum,
};
pub(crate) use dump::render_json;
pub use dump::{DUMP_SCHEMA, canonical_dump, curriculum_hash, python_repr_f64, sha256_hex};
pub use finding::Finding;
pub use finite::{FiniteCaseRole, FiniteCaseVariant, FiniteObjectiveCase, FiniteObjectiveDomain};
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

use sha2::{Digest, Sha256};

/// Stable semantic identity of one fully loaded curriculum snapshot.
///
/// This includes every topic field (including finite domains and prerequisites),
/// its hierarchy labels, course metadata, and the authored unit count.
pub fn review_context_digest(curriculum: &Curriculum) -> Result<String, String> {
    let topics = curriculum
        .topics()
        .iter()
        .map(|topic| {
            let idx = curriculum
                .idx_of(topic.id.as_str())
                .ok_or_else(|| format!("loaded topic {} is absent from its own index", topic.id))?;
            Ok((
                curriculum.course_of(idx),
                curriculum.module_of(idx),
                curriculum.unit_of(idx),
                topic,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bytes = serde_json::to_vec(&(
        "cadus-curriculum-review-context-v1",
        curriculum.unit_count(),
        curriculum.courses(),
        topics,
    ))
    .map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
