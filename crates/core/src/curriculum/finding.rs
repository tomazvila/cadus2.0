//! One curriculum defect, reported by the loader or by the lint (C5).
//!
//! The shape mirrors the 1.0 `Finding` dataclass and its `as_dict` output
//! (`cadus/graph.py:67-107`): the three mandatory fields `code`, `message` and
//! `fatal`, plus three optional fields that the JSON output drops when they are
//! empty.

use serde::{Deserialize, Serialize};

/// A finding is fatal by default, the same as 1.0. A new code opts in to
/// "advisory" on purpose, so an unset flag never hides a dropped topic.
fn default_fatal() -> bool {
    true
}

/// True when the context list holds no entry. 1.0 drops an empty context from
/// `as_dict`, so the JSON of the two implementations stays the same.
fn context_is_empty(context: &Option<Vec<String>>) -> bool {
    match context {
        None => true,
        Some(items) => items.is_empty(),
    }
}

/// One violation of a curriculum rule.
///
/// `fatal` says whether the finding makes the curriculum unloadable — content
/// was dropped or could not be parsed — or is only advisory. The parse stage
/// blocks a load on a fatal finding; the graph stage reports every finding and
/// blocks nothing (spec section 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// The stable 1.0 code, for example `schema` or `weight_out_of_range`.
    pub code: String,
    /// The human-readable text. Parse-stage schema findings start with the
    /// dotted location inside the file.
    pub message: String,
    /// The topic the finding belongs to, if the finding names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    /// The file the finding belongs to, relative to the curriculum root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Extra data, for example the node list of a cycle.
    #[serde(default, skip_serializing_if = "context_is_empty")]
    pub context: Option<Vec<String>>,
    /// True when the finding blocks a load.
    #[serde(default = "default_fatal")]
    pub fatal: bool,
}

impl Finding {
    /// Make a fatal finding.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            topic: None,
            file: None,
            context: None,
            fatal: true,
        }
    }

    /// Make an advisory finding. An advisory finding drops no content, so a load
    /// continues after it.
    pub fn advisory(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            fatal: false,
            ..Self::new(code, message)
        }
    }

    /// Attach the topic id.
    #[must_use]
    pub fn with_topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = Some(topic.into());
        self
    }

    /// Attach the file path, relative to the curriculum root.
    #[must_use]
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Attach the context list.
    #[must_use]
    pub fn with_context(mut self, context: Vec<String>) -> Self {
        self.context = Some(context);
        self
    }
}
