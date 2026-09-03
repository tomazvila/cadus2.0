//! The four authoring prompts, their tool schemas, and the prompt digest (A2).
//!
//! Requirements: A2 (content is authored offline), L4 and L5 (a teach page and a
//! hint ladder are authored documents, not live model calls), A4 (the
//! pre-authored distractors), C6 (the digest a reviewer approves), T5 (the
//! system message is the cached prefix), R4 and L6 (this code runs in the
//! worker).
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` sections 1.1 and 2 in
//! full, and row R1 of section 7.
//!
//! # What this module owns, and what it does not
//!
//! It builds a [`ChatRequest`]. It makes no model call, it reads no database,
//! and it decides nothing about a document. The gate
//! (`cadus_core::template::gate`) judges what comes back, and unit R2 runs the
//! loop.
//!
//! # The prompt and the code are one statement
//!
//! Three lists reach the model from the code that enforces them: the error tags
//! come from [`MODEL_ERROR_TAGS`], the evaluation functions from
//! [`EVAL_FUNCTIONS`], and the bounds from `cadus_core::template`. 1.0 measured
//! what a second copy costs: a tag the prompt invited and the filter lacked was
//! dropped in silence (`prompts.py:529-536`).
//!
//! # Why the model authors a subset of the document
//!
//! The tool schemas ask for the authored fields only. `v`, `topic_id`,
//! `answer_kind` and `space_size` are server fields: the job knows the first
//! three, and the gate computes the fourth (spec section 2.3). A model that
//! writes them states a second source of truth for each.
//!
//! # The retry block is the yield lever
//!
//! 1.0 states the measurement directly: every rejection message reads as an
//! instruction, and a told retry converts most first refusals into a template
//! that lasts forever (`problem_templates.py:1386-1393`). [`retry_block`]
//! therefore reproduces the 1.0 wording of `prompts.py:766-774`, and the loop of
//! R2 passes the literal gate message through it.

use cadus_core::curriculum::{AnswerKind, Exemplar};
use cadus_model_client::ChatRequest;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

mod schema;
mod system;

pub use schema::{tool_schema, tool_spec};
pub use system::system_prompt;

/// The four kinds of authored content (`content_store.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A problem template: the structure every problem of one knowledge point
    /// is drawn from.
    Template,
    /// A teach page: the concept and one fully worked example (L4).
    Teach,
    /// A hint ladder: the rungs the hint route serves (L5).
    HintLadder,
    /// A distractor list: the pre-authored diagnosis of A4, for a knowledge
    /// point that serves exemplars and has no template.
    Diagnosis,
}

/// The four kinds, in the order `migrations/0005_content.sql:14` lists them.
pub const KINDS: [Kind; 4] = [
    Kind::Template,
    Kind::Teach,
    Kind::HintLadder,
    Kind::Diagnosis,
];

/// The forced tool of the template prompt (the 1.0 name, `prompts.py:398`).
pub const TOOL_TEMPLATE: &str = "emit_template";

/// The forced tool of the teach prompt.
///
/// 1.0 calls it `emit_instruction` (`prompts.py:298`). The 2.0 document is a
/// different shape — `worked_example` nests `problem` and `steps` — and the kind
/// is named `teach` in the schema, so the tool takes the kind's name.
pub const TOOL_TEACH: &str = "emit_teach";

/// The forced tool of the hint-ladder prompt. 1.0 has no such tool: it asks for
/// one hint at a time on the request path (`prompts.py:635`).
pub const TOOL_HINT_LADDER: &str = "emit_hint_ladder";

/// The forced tool of the distractor prompt. 1.0 has no such tool.
pub const TOOL_DISTRACTORS: &str = "emit_distractors";

/// The comparisons of the constraint language (`cadus_core::template::Cmp`).
///
/// The schema names them, so the model writes a constraint the gate reads.
/// `authoring_prompt::constraint_ops_parse_as_the_core_grammar` proves every
/// name below is a `Cmp`.
pub const CONSTRAINT_OPS: [&str; 9] = [
    "eq", "ne", "lt", "le", "gt", "ge", "divides", "coprime", "carries",
];

/// The count of hex characters the prompt digest keeps (1.0
/// `prompts.py:779-796`).
pub const DIGEST_CHARS: usize = 16;

/// The difficulty line of a spec that states no target (1.0 `prompts.py:745`).
pub const DEFAULT_DIFFICULTY: &str = "standard review (80-85% expected accuracy)";

/// The exemplar block of a knowledge point with no exemplars (1.0
/// `prompts.py:667`).
pub const NO_EXEMPLARS: &str =
    "(no exemplars provided — infer a reasonable problem for this topic)";

/// The first line of the retry block (1.0 `prompts.py:767-769`).
pub const RETRY_HEADER: &str = "YOUR PREVIOUS ATTEMPT WAS REFUSED. The server's exact reason was:";

/// The indent the rejection message takes (1.0 `prompts.py:770`).
pub const RETRY_INDENT: &str = "    ";

impl Kind {
    /// The value `content_store.kind` holds.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Template => "template",
            Self::Teach => "teach",
            Self::HintLadder => "hint_ladder",
            Self::Diagnosis => "diagnosis",
        }
    }

    /// The kind of a wire value, or [`None`] for anything else.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        KINDS.into_iter().find(|kind| kind.as_str() == value)
    }

    /// The name of the tool the reply must call.
    #[must_use]
    pub const fn tool_name(self) -> &'static str {
        match self {
            Self::Template => TOOL_TEMPLATE,
            Self::Teach => TOOL_TEACH,
            Self::HintLadder => TOOL_HINT_LADDER,
            Self::Diagnosis => TOOL_DISTRACTORS,
        }
    }

    /// The closing line of the retry block.
    ///
    /// The template line is the 1.0 wording of `prompts.py:771-774`, byte for
    /// byte. The other three name their own document and their own fields,
    /// because "change the domains, the samples, or the expression" instructs a
    /// teach-page author to edit fields a teach page does not have.
    #[must_use]
    pub const fn retry_fix(self) -> &'static str {
        match self {
            Self::Template => {
                "Fix that specifically. Do not restate the same template — change the domains, the samples, or the expression so the reason no longer applies."
            }
            Self::Teach => {
                "Fix that specifically. Do not restate the same page — change the concept, the worked problem, or its steps so the reason no longer applies."
            }
            Self::HintLadder => {
                "Fix that specifically. Do not restate the same ladder — change the rungs so the reason no longer applies."
            }
            Self::Diagnosis => {
                "Fix that specifically. Do not restate the same list — change the answers, the tags, or the notes so the reason no longer applies."
            }
        }
    }

    /// The closing instruction of the user message.
    #[must_use]
    pub const fn ask(self) -> &'static str {
        match self {
            Self::Template => "Now emit the template via the emit_template tool.",
            Self::Teach => "Now emit the teach page via the emit_teach tool.",
            Self::HintLadder => "Now emit the hint ladder via the emit_hint_ladder tool.",
            Self::Diagnosis => "Now emit the distractors via the emit_distractors tool.",
        }
    }
}

/// What one authoring call knows about the knowledge point it authors for.
///
/// It is 1.0's `ProblemSpec` minus the serve-path fields: a template is not one
/// instance, so the recent-problem list of `generate_user` has nothing to avoid
/// (1.0 `prompts.py:738-741`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringSpec {
    /// The knowledge point the content belongs to.
    pub kp_id: String,
    /// The name a reviewer reads.
    pub kp_name: String,
    /// The topic the knowledge point sits in.
    pub topic_id: String,
    /// The name of that topic.
    pub topic_name: String,
    /// The answer grammar of the topic.
    pub answer_kind: AnswerKind,
    /// The difficulty target, or [`None`] for [`DEFAULT_DIFFICULTY`].
    pub difficulty_target: Option<String>,
    /// The constraints the curriculum states in prose.
    pub constraints: Option<String>,
    /// The worked problems the content mirrors.
    pub exemplars: Vec<Exemplar>,
}

/// The retry block of `prompts.py:766-774`, with the gate's literal message.
///
/// The message goes in verbatim. A summarized reason is a different instruction,
/// and the instruction is what the 1.0 measurement rescued the template with.
#[must_use]
pub fn retry_block(kind: Kind, feedback: &str) -> String {
    format!(
        "{RETRY_HEADER}\n{RETRY_INDENT}{feedback}\n{}",
        kind.retry_fix()
    )
}

/// The exemplar block (1.0 `render_exemplars`, `prompts.py:663-675`).
#[must_use]
pub fn render_exemplars(exemplars: &[Exemplar]) -> String {
    if exemplars.is_empty() {
        return NO_EXEMPLARS.to_owned();
    }
    let mut lines = Vec::new();
    for (index, exemplar) in exemplars.iter().enumerate() {
        lines.push(format!("{}. Problem: {}", index + 1, exemplar.problem));
        lines.push(format!("   Answer: {}", exemplar.answer));
        if let Some(sketch) = exemplar.solution_sketch.as_deref() {
            lines.push(format!("   Method: {sketch}"));
        }
    }
    lines.join("\n")
}

/// The user message of one authoring call (1.0 `template_user`).
///
/// The frame is 1.0's: topic, answer kind, knowledge point, difficulty target,
/// constraints, then the exemplars. `feedback` appends the retry block.
#[must_use]
pub fn user_message(kind: Kind, spec: &AuthoringSpec, feedback: Option<&str>) -> String {
    let mut lines = vec![
        format!("Topic: {} (id: {})", spec.topic_name, spec.topic_id),
        format!("Answer kind: {}", spec.answer_kind.as_str()),
        format!("Knowledge point: {} (id: {})", spec.kp_name, spec.kp_id),
        format!(
            "Difficulty target: {}",
            spec.difficulty_target
                .as_deref()
                .unwrap_or(DEFAULT_DIFFICULTY)
        ),
    ];
    match spec.constraints.as_deref() {
        Some(text) => lines.push(format!(
            "Constraints (every instance must obey these exactly): {text}"
        )),
        None => lines.push("Constraints: none stated.".to_owned()),
    }
    lines.push(String::new());
    lines.push(exemplar_preamble(kind).to_owned());
    lines.push(render_exemplars(&spec.exemplars));
    if let Some(feedback) = feedback {
        lines.push(String::new());
        lines.push(retry_block(kind, feedback));
    }
    lines.push(String::new());
    lines.push(kind.ask().to_owned());
    lines.join("\n")
}

/// The line that introduces the exemplars, per kind.
const fn exemplar_preamble(kind: Kind) -> &'static str {
    match kind {
        Kind::Template => {
            "Exemplars to match in shape and method. Their specific values are examples of what should VARY — turn those into placeholders:"
        }
        Kind::Teach => {
            "Exemplars of the practice this page prepares. Teach the method they share, and give your worked example DIFFERENT values:"
        }
        Kind::HintLadder => {
            "Exemplars of the practice the ladder supports. The rungs must fit every problem of this shape, not one instance:"
        }
        Kind::Diagnosis => {
            "Exemplars of the practice the distractors diagnose. Read the method they share and name the ways it goes wrong:"
        }
    }
}

/// The whole request of one authoring attempt.
///
/// `feedback` is the literal rejection message of the previous attempt, or
/// [`None`] on the first one.
#[must_use]
pub fn request(kind: Kind, spec: &AuthoringSpec, feedback: Option<&str>) -> ChatRequest {
    ChatRequest {
        system: system_prompt(kind),
        user: user_message(kind, spec, feedback),
        tool: tool_spec(kind),
    }
}

// --------------------------------------------------------------------------- //
// The prompt digest (1.0 `template_prompt_digest`, `prompts.py:779-796`)
// --------------------------------------------------------------------------- //

/// The material the digest covers: the system message, the tool name, the schema.
///
/// It is 1.0's three keys. The user message is NOT in it, and must stay out: the
/// user message carries the knowledge point, so a fold of it gives one digest per
/// knowledge point instead of one digest per prompt.
#[must_use]
pub fn digest_material(kind: Kind) -> Value {
    let tool = tool_spec(kind);
    json!({
        "system": system_prompt(kind),
        "tool": tool.name,
        "schema": tool.parameters,
    })
}

/// The first [`DIGEST_CHARS`] hex characters of the SHA-256 of `material`.
///
/// `serde_json` holds an object in a `BTreeMap`, so the text is key-sorted at
/// every depth and a re-serialization gives the same bytes. That is 1.0's
/// `sort_keys=True` and the reason the digest is stable.
#[must_use]
pub fn digest_of(material: &Value) -> String {
    let text = material.to_string();
    let digest = Sha256::digest(text.as_bytes());
    let mut out = String::with_capacity(DIGEST_CHARS);
    for byte in digest.iter().take(DIGEST_CHARS.div_ceil(2)) {
        out.push_str(&format!("{byte:02x}"));
    }
    out.truncate(DIGEST_CHARS);
    out
}

/// The prompt digest of one kind (C6, spec section 2.2).
///
/// 1.0 folds the digest into the cache key, so a prompt edit retires every
/// stored template (`problem_templates.py:1129-1163`). 2.0 stores it on the row
/// instead: the C6 approval binds to the CONTENT, so a prompt edit marks the
/// affected rows for re-authoring and never silently unapproves one.
#[must_use]
pub fn prompt_digest(kind: Kind) -> String {
    digest_of(&digest_material(kind))
}
