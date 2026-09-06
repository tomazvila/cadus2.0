//! The pool row documents and the serving key (D-S5, D-O4).
//!
//! # Why the documents live in the core
//!
//! `serving_pool` keeps two jsonb columns: `problem` and `expected_answer`
//! (`migrations/0005_content.sql`). The store writes them and the web tier reads
//! them, so the shape belongs to neither adapter. It lives here beside
//! [`Ring`](super::Ring), which is the D-S6 document of the same serve.
//!
//! # What the problem document carries
//!
//! The M4 decision states it: "the seed and the drawn bindings are stored inside
//! `serving_pool.problem`". Two properties follow. A reviewer reproduces any
//! served instance from the recorded `u64` alone, and an operator reads the drawn
//! tuple without a second query.
//!
//! The bindings arrive as canonical strings, not as JSON numbers. JSON has one
//! number type and reads `1.5` as a float, and a float in a value the checker
//! then computes with is the trap D6 forbids.
//! [`Value::canonical_string`](crate::template::Value::canonical_string) writes
//! the exact form (`3/4`, `-7`, `\times`), so the document holds no float
//! spelling at all.
//!
//! # What the answer document carries
//!
//! The answer string, inside the M2 grammar. The canonical form is NOT stored:
//! [`Canon`](crate::answer::Canon) holds `BigRational` and `BigInt` values, and a
//! JSON spelling of those is a second wire format for one fact. The grade path
//! canonicalizes the string, exactly as it canonicalizes the learner's answer
//! (L2, V2).
//!
//! # The serving key
//!
//! A knowledge-point id is unique inside its topic and NOT across the curriculum:
//! `kp1` names 1090 different knowledge points in `curriculum/`. `serving_pool`
//! and `content_store` both carry one flat `kp_id` text column, so the value in
//! that column is the topic-qualified key that [`kp_key`] writes.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::template::{Bindings, Instance};

/// The version of the two documents below.
///
/// A bump retires every unclaimed row, because the reader refuses a version it
/// does not know.
pub const POOL_ROW_VERSION: u32 = 1;

/// The separator of the topic-qualified serving key.
///
/// No topic id and no knowledge-point id of `curriculum/` holds this character,
/// so the split of [`split_kp_key`] is unambiguous.
pub const KP_KEY_SEPARATOR: char = '/';

/// Write the serving key of one knowledge point.
///
/// The key is `"<topic_id>/<kp_id>"`. It is the value of `serving_pool.kp_id` and
/// of `content_store.kp_id`.
#[must_use]
pub fn kp_key(topic_id: &str, kp_id: &str) -> String {
    let mut key = String::with_capacity(topic_id.len() + kp_id.len() + 1);
    key.push_str(topic_id);
    key.push(KP_KEY_SEPARATOR);
    key.push_str(kp_id);
    key
}

/// Read a serving key back into its topic id and its knowledge-point id.
///
/// The split takes the FIRST separator, so a knowledge-point id that holds one
/// still reads back to the same topic. A key with an empty half returns `None`,
/// and so does a key with no separator.
#[must_use]
pub fn split_kp_key(key: &str) -> Option<(&str, &str)> {
    let (topic, kp) = key.split_once(KP_KEY_SEPARATOR)?;
    if topic.is_empty() || kp.is_empty() {
        return None;
    }
    Some((topic, kp))
}

/// A pool row document that did not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct PoolBodyError {
    /// What the reader refused.
    pub message: String,
}

impl fmt::Display for PoolBodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl PoolBodyError {
    /// Build the error from a reader message.
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<serde_json::Error> for PoolBodyError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}

/// Give one row document its `to_body` and `from_body` methods.
macro_rules! pool_body {
    ($document:ident) => {
        impl $document {
            /// Write the document.
            ///
            /// # Errors
            ///
            /// Returns [`PoolBodyError`] when the document does not serialize.
            pub fn to_body(&self) -> Result<String, PoolBodyError> {
                serde_json::to_string(self).map_err(PoolBodyError::from)
            }

            /// Read the document and refuse an unknown version.
            ///
            /// # Errors
            ///
            /// Returns [`PoolBodyError`] when the text is not this document, and when
            /// `v` is not [`POOL_ROW_VERSION`].
            pub fn from_body(body: &str) -> Result<Self, PoolBodyError> {
                let doc: Self = serde_json::from_str(body)?;
                check_version(doc.v)?;
                Ok(doc)
            }
        }
    };
}

/// The document of `serving_pool.problem`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolProblem {
    /// [`POOL_ROW_VERSION`].
    pub v: u32,
    /// The rendered statement, as the learner reads it.
    pub text: String,
    /// The drawn tuple, one canonical string per parameter name.
    ///
    /// An exemplar row binds no parameter, so the map is empty and the field
    /// leaves the document.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<String, String>,
    /// The seed of the refill batch that drew this instance.
    pub seed: u64,
}

impl PoolProblem {
    /// Build the document of one drawn instance.
    #[must_use]
    pub fn from_instance(instance: &Instance, seed: u64) -> Self {
        Self {
            v: POOL_ROW_VERSION,
            text: instance.text.clone(),
            bindings: canonical_bindings(&instance.bindings),
            seed,
        }
    }
}

pool_body!(PoolProblem);

/// The document of `serving_pool.expected_answer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolAnswer {
    /// [`POOL_ROW_VERSION`].
    pub v: u32,
    /// The policy captured with this item; absence preserves legacy semantics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_contract: Option<crate::answer::AnswerContract>,
    /// The expected answer, inside the M2 grammar.
    pub answer: String,
}

impl PoolAnswer {
    /// Build the document of one instance's answer.
    #[must_use]
    pub fn from_instance(instance: &Instance) -> Self {
        Self {
            v: POOL_ROW_VERSION,
            answer: instance.answer.clone(),
            answer_contract: instance.answer_contract,
        }
    }
}

pool_body!(PoolAnswer);

/// Refuse a document version this build does not know.
fn check_version(v: u32) -> Result<(), PoolBodyError> {
    if v == POOL_ROW_VERSION {
        return Ok(());
    }
    Err(PoolBodyError::new(format!(
        "pool row version {v} is not {POOL_ROW_VERSION}"
    )))
}

/// Write every bound value as its exact canonical string (D6).
fn canonical_bindings(bindings: &Bindings) -> BTreeMap<String, String> {
    bindings
        .iter()
        .map(|(name, value)| (name.clone(), value.canonical_string()))
        .collect()
}
