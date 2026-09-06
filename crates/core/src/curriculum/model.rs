//! Curriculum data types (C5, spec section 1).
//!
//! Each type mirrors the 1.0 pydantic model of the same name (`cadus/model.py`).
//! Every struct denies unknown keys, the same as the 1.0 `extra="forbid"` base
//! class, so a typo in a YAML file is an error and not a silent drop. Optional
//! fields keep their `null` in the serialized output, because the M1 parity dump
//! (spec section 8) writes them.

use std::fmt;

use serde::de::{Deserializer, Unexpected, Visitor};
use serde::{Deserialize, Serialize};

/// The default of `Topic::core` (spec section 1).
fn default_true() -> bool {
    true
}

/// Reads an integer field the way 1.0 pydantic reads one in lax mode: a YAML
/// float with no fractional part is an integer, so `expected_time_secs: 60.0`
/// is 60 (spec section 7, "2.0 strictness"). The loader walk accepts the same
/// values, so the two stay in step and no file is dropped between them.
struct LaxI64;

impl Visitor<'_> for LaxI64 {
    type Value = i64;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an integer, or a float with no fractional part")
    }

    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<i64, E> {
        Ok(value)
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<i64, E> {
        i64::try_from(value).map_err(|_| E::invalid_value(Unexpected::Unsigned(value), &self))
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<i64, E> {
        // The upper bound is exclusive: `i64::MAX` has no `f64` form, so the
        // cast below would saturate on the first float above it.
        if value.fract() == 0.0
            && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value)
        {
            return Ok(value as i64);
        }
        Err(E::invalid_value(Unexpected::Float(value), &self))
    }
}

/// Deserialize an integer field in the lax mode of 1.0 (see [`LaxI64`]).
fn lax_i64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    deserializer.deserialize_any(LaxI64)
}

/// A slug is empty after the outer whitespace is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("String should have at least 1 character")]
pub struct SlugError;

/// The 1.0 `Slug` (`model.py:22-23`): a string with the outer whitespace
/// removed and at least one character left. Kebab-case is a convention of the
/// authors, not a rule of the type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Slug(String);

impl Slug {
    /// Make a slug. The outer whitespace goes away; an empty rest is an error.
    pub fn new(raw: impl AsRef<str>) -> Result<Self, SlugError> {
        let trimmed = raw.as_ref().trim();
        if trimmed.is_empty() {
            return Err(SlugError);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The slug text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Slug {
    type Error = SlugError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(raw)
    }
}

impl AsRef<str> for Slug {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The answer grammar of a topic (spec section 4). `MultiStep` keeps the
/// hyphenated wire value `multi-step` of 1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AnswerKind {
    Numeric,
    Expression,
    MultiStep,
    Proof,
}

/// The wire values of [`AnswerKind`], in declaration order.
pub const ANSWER_KINDS: [&str; 4] = ["numeric", "expression", "multi-step", "proof"];

impl AnswerKind {
    /// The wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Numeric => "numeric",
            Self::Expression => "expression",
            Self::MultiStep => "multi-step",
            Self::Proof => "proof",
        }
    }
}

impl fmt::Display for AnswerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The card kind of an Anki seed (1.0 `AnkiSeedType`).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum AnkiType {
    #[default]
    Basic,
    Cloze,
}

/// The wire values of [`AnkiType`], in declaration order.
pub const ANKI_TYPES: [&str; 2] = ["basic", "cloze"];

impl AnkiType {
    /// The wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Cloze => "cloze",
        }
    }
}

impl fmt::Display for AnkiType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A worked problem that anchors a knowledge point or a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exemplar {
    pub problem: String,
    /// The policy captured with this item; absence preserves legacy semantics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_contract: Option<crate::answer::AnswerContract>,
    pub answer: String,
    #[serde(default)]
    pub solution_sketch: Option<String>,
}

impl Exemplar {
    /// Validate this item's authored answer under its captured policy.
    pub fn canonical_answer(&self) -> Result<crate::answer::Canon, crate::answer::Undecidable> {
        self.answer_contract.as_ref().map_or_else(
            || crate::answer::canonical_form(&self.answer),
            |contract| contract.validate_expected(&self.answer),
        )
    }
}

/// A declarative-recall card candidate authored on a topic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnkiSeed {
    pub front: String,
    pub back: String,
    #[serde(default, rename = "type")]
    pub kind: AnkiType,
}

/// A weighted prerequisite edge. `weight` is the encompassing weight; 0 means
/// conceptual familiarity only. The same type carries `encompassings_extra`,
/// where `key` stays at its default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrereqEdge {
    pub id: Slug,
    pub weight: f64,
    #[serde(default)]
    pub key: bool,
}

/// One knowledge point of a topic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgePoint {
    pub id: Slug,
    pub name: String,
    #[serde(default)]
    pub key_prerequisites: Vec<Slug>,
    #[serde(default)]
    pub exemplars: Vec<Exemplar>,
    /// Free text in 1.0. M1 carries it as an opaque string; the structured form
    /// of A1 and D-S4 is a 2.0 addition and not a port.
    #[serde(default)]
    pub constraints: Option<String>,
}

/// A curriculum topic. The cardinality rules — at least one knowledge point and
/// so on — belong to the lint, not to the schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Topic {
    pub id: Slug,
    pub name: String,
    #[serde(default = "default_true")]
    pub core: bool,
    pub difficulty: f64,
    #[serde(default)]
    pub drill: bool,
    pub answer_kind: AnswerKind,
    #[serde(deserialize_with = "lax_i64")]
    pub expected_time_secs: i64,
    #[serde(default)]
    pub prerequisites: Vec<PrereqEdge>,
    #[serde(default)]
    pub encompassings_extra: Vec<PrereqEdge>,
    #[serde(default)]
    pub knowledge_points: Vec<KnowledgePoint>,
    #[serde(default)]
    pub diagnostic_exemplar: Option<Exemplar>,
    #[serde(default)]
    pub anki_seeds: Vec<AnkiSeed>,
}

/// One unit file, `curriculum/<course>/<nn>-<unit>.yaml`.
///
/// The loader does not compare `course` with the directory the file came from.
/// 1.0 trusts the field (`graph.py:602-614`), and the arena of U2 reads the
/// field, not the directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unit {
    pub unit: String,
    pub course: Slug,
    pub module: String,
    #[serde(default)]
    pub topics: Vec<Topic>,
}

/// One course entry of `courses.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Course {
    pub id: Slug,
    pub name: String,
    #[serde(deserialize_with = "lax_i64")]
    pub order: i64,
    #[serde(default)]
    pub mastery_floor: Vec<Slug>,
    #[serde(default)]
    pub mastery_floor_course: Option<Slug>,
}

/// The `courses.yaml` document. 1.0 calls the model `CourseCatalog`; the schema
/// messages of the loader keep that name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    #[serde(default)]
    pub courses: Vec<Course>,
}
