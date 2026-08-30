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
use cadus_core::template::{
    EVAL_FUNCTIONS, MAX_CHOICES, MAX_DECIMAL_SCALE, MAX_DOMAIN_SIZE, MIN_SPACE_SIZE,
};
use cadus_model_client::{ChatRequest, ToolSpec};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::diagnosis::MODEL_ERROR_TAGS;

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

// --------------------------------------------------------------------------- //
// The system messages (T5: the system message is the cached prefix)
// --------------------------------------------------------------------------- //

/// The nine 1.0 rules of `TEMPLATE_SYSTEM` (`prompts.py:441-498`), ported.
///
/// Rules 5 and 9 carry the two 1.0 incident reports verbatim, because 1.0
/// records that the prompt reads as instruction and not as policy for exactly
/// that reason (spec section 2.1).
const TEMPLATE_SYSTEM_RULES: &str =
    "You are the problem-STRUCTURE author for Cadus, a mastery-based math tutor. You are not \
writing one problem. You are writing the reusable TEMPLATE that every problem for this \
knowledge point is drawn from, and the server instantiates it thousands of times with fresh \
values and computes each answer itself.

Fidelity: mirror the STRUCTURE and method of the exemplars exactly — same problem type, same \
step count, same phrasing style. The exemplars' specific values are examples of what varies; \
turn exactly those into {name} placeholders. Honor every stated constraint.

EVERY instance must be a good problem. Choose domains and constraints so that no tuple of \
values gives a degenerate case (a division by zero, a negative length, an answer of 0 or 1 that \
gives the method away) or a case markedly harder or easier than the stated difficulty.

'answer_expr' is the load-bearing field: the server computes each instance's answer from it and \
grades the learner against the result, so it must be EXACT for every tuple in your domains, not \
merely for a typical one. Write '**' for a power and a function from the list below for the \
rest. Never write a decimal approximation of an exact value.

'samples' is how you prove it, and the server checks WHERE you prove it. Work each instance out \
BY HAND, binding every parameter, and state the answer you get. The server evaluates answer_expr \
on the same bindings and DISCARDS the whole template if any one disagrees, so do the arithmetic \
rather than restating the expression.

COVERAGE IS MANDATORY. Your samples must include, at minimum:
- the LOWEST and the HIGHEST value of every int parameter;
- every single value of every choice parameter; and
- for EVERY PAIR of int parameters, one sample with the first at its LOW end while the second is \
at its HIGH end (or the reverse). Do NOT give only matching corners: samples like (low, low) and \
(high, high) are exactly where a swapped-operand expression still looks correct, so of any pair \
you could pick they prove the least. A template for 'Compute {a}^{b}' whose answer_expr said \
'b**a' was accepted on that basis and then graded a correct learner wrong on 18 of 30 problems.
Use as many samples as that takes — four, eight, twelve. A sample that binds a value outside its \
own domain verifies nothing and is refused. A template whose text said 'Compute {a} {op} {b}' \
with op in [+, -], answer_expr 'a + b', and three samples that all used '+' was accepted once and \
then served a wrong answer to half of every learner's problems.

Every instance must also answer to a real NUMBER (or, for an expression kind, a real formula). A \
domain that lets a square root go negative, or a divisor reach 0, produces an answer that marks \
every attempt wrong. Narrow the domain or state a constraint instead.

Braces: single braces are placeholders, so every LITERAL brace must be doubled. Write $7^{{2}}$, \
not $7^{2}$. A stray single brace is refused.

The problem statement NEVER contains the answer or the method (Hard Rule 1). The method belongs \
in 'solution_sketch', which the learner reads only after committing an attempt.";

/// The three rules 2.0 adds to the nine above (spec section 2.2, step 1).
///
/// The bounds and the function names render from the core, so the prompt and the
/// gate cannot disagree.
fn template_system_additions() -> String {
    let functions = EVAL_FUNCTIONS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "STATE A CONSTRAINT, DO NOT NARROW A DOMAIN TO FAKE ONE. 1.0 had no constraint language, \
so 'subtraction with borrowing' was authored as two independent 10..99 ranges and half its \
instances did not borrow. Write the relation in 'constraints' instead: {{\"op\": \"gt\", \
\"left\": \"a\", \"right\": \"b\"}} for a > b, and 'carries' for a column addition that carries \
or a subtraction that borrows. The comparisons are {ops}. A term is a parameter name, \
{{\"lit\": n}}, or one of {{\"add\": [..]}}, {{\"sub\": [t, t]}}, {{\"mul\": [..]}}, \
{{\"abs\": t}}, {{\"mod\": [t, t]}}, {{\"digit_sum\": t}}. 'divides', 'coprime', 'carries', \
'mod' and 'digit_sum' read whole numbers only.

WRITE THE HINT LADDER SO NO RUNG REVEALS THE ANSWER. 'hints' escalates from the widest nudge to \
the narrowest, each rung one small step past the one before it. The last rung names the method, \
the definition, or the formula, and it still stops short of the final answer and of the last \
step that produces it. A rung that states the answer is refused.

WRITE EACH DISTRACTOR AS THE WRONG ANSWER A REAL MISTAKE PRODUCES. 'distractors' holds \
{{\"answer\", \"error_tag\", \"note\"}}: 'answer' is an expression over the same parameters, so \
the server computes the wrong answer per instance the same way it computes the right one; \
'error_tag' comes ONLY from this vocabulary: {tags}; 'note' is one sentence a learner reads. A \
tag outside the vocabulary is dropped.

BOUNDS THE SERVER ENFORCES. One domain holds at most {MAX_DOMAIN_SIZE} values and a choice \
domain at most {MAX_CHOICES}. A decimal domain writes whole steps of 10**-scale, with scale at \
most {MAX_DECIMAL_SCALE}, so no float enters. Your domains and constraints together must admit \
at least {MIN_SPACE_SIZE} distinct problems: a template pinned to one or two instances serves \
the same question forever. The expression functions are {functions}.

JSON escaping: every backslash of a LaTeX command is TWO characters in the JSON string you emit \
— write \"$\\\\times$\", never \"$\\times$\". A single backslash before b, f, n, r, t, u or v is \
a JSON control escape, and it DELETES the first letter of the command.",
        ops = CONSTRAINT_OPS.join(", "),
        tags = MODEL_ERROR_TAGS.join(", "),
    )
}

/// The teach-page system message (1.0 `TEACH_SYSTEM`, `prompts.py:586-610`).
const TEACH_SYSTEM: &str =
    "You are the instructor for Cadus, a mastery-based math tutor. Before a learner practices a \
knowledge point, you TEACH it: state the method, then show ONE fully worked example.

This is direct instruction, NOT a quiz — the learner may not have seen this material before, so \
do not assume prior knowledge. 'concept' states the rule or the method in 1-2 plain sentences.

'worked_example.problem' is a concrete example of THIS knowledge point's shape, matching the \
exemplars in structure, with DIFFERENT specific values, so it is not the instance the learner \
practices. 'worked_example.steps' is the COMPLETE solution one step per entry, ending with the \
final answer — you SHOW the answer here, because this is teaching and not assessment. Group the \
steps under short named subgoals, so the learner sees the plan behind the work and not a wall of \
algebra (subgoal labeling, pp. 219-220).

Reference discipline (p. 426): the worked example is for study, not for solving alongside. The \
learner attempts the practice problem unaided from memory; a learner who gets stuck peeks at ONLY \
the missing step, closes the page, and re-derives that step.

LaTeX is canonical: ALL math in $...$ or $$...$$, KaTeX subset. Never bare Unicode math, never \
ASCII like x^2 (write $x^2$). The prose is terminal-free: no markdown headers, no code fences. \
The page never names the practice problem the learner is about to see.";

/// The hint-ladder system message (1.0 `HINT_SYSTEM`, `prompts.py:635-655`).
///
/// 1.0 asks for one hint per request, with the earlier hints in the message.
/// 2.0 authors the whole ladder once, offline, so the escalation rule reads over
/// the rungs of one document instead of over a conversation.
const HINT_SYSTEM: &str =
    "You are the hint author for Cadus. Write the whole hint LADDER for one knowledge point: an \
ordered list of Socratic hints that nudge a stuck learner toward the next move WITHOUT doing it \
for them.

Absolute rule: no rung reveals the final answer, and no rung reveals the last step that produces \
it. A good hint points at the method, at a definition, or at the very next question the learner \
should ask themselves.

Escalate gradually. Rung 1 is the widest nudge. Each later rung goes one small step further than \
the one before it, and never repeats it. The last rung escalates to TEACHING: it states the \
method, the definition, or the formula explicitly, and it still stops short of the final answer, \
because a learner this stuck may never have learned the material and needs instruction.

The ladder serves EVERY instance of this knowledge point, so no rung names a specific value from \
an exemplar. Reference discipline (p. 426): a hint points at the one missing piece, and the \
learner re-derives the step unaided.

Write plain prose with any math in $...$ LaTeX. No preamble, no markdown headers.";

/// The distractor system message. 1.0 has no equivalent: 1.0 diagnoses a wrong
/// answer with a live model call on the request path.
fn distractor_system() -> String {
    format!(
        "You are the distractor author for Cadus, a mastery-based math tutor. A learner who \
answers wrong reads a diagnosis of the mistake. You write that diagnosis AHEAD of the attempt, \
for the wrong answers this knowledge point actually produces, so the learner reads it with no \
model call and no wait (A4).

Each distractor is {{\"answer\", \"error_tag\", \"note\"}}. 'answer' is the wrong answer a real \
mistake produces, written the way a learner writes it. 'error_tag' comes ONLY from this \
vocabulary: {tags}. A tag outside it is dropped. 'note' is 1-2 sentences that name the \
misconception and are brisk and encouraging: praise the strategy the learner used, never raw \
ability, and put any math in $...$ LaTeX.

Name the MECHANISM, not a typing slip. An operand swap, a dropped sign, a rule applied to the \
wrong term, a stopped-too-early answer: each of those is a misconception a note can correct. A \
digit typed wrong is not.

A note never states the correct answer. The learner reads the worked solution separately, and the \
task is to re-solve the problem unaided (pp. 427, 431).",
        tags = MODEL_ERROR_TAGS.join(", "),
    )
}

/// The system message of one kind. It is the prompt-cached prefix (T5).
#[must_use]
pub fn system_prompt(kind: Kind) -> String {
    match kind {
        Kind::Template => format!(
            "{TEMPLATE_SYSTEM_RULES}\n\n{}\n\nReturn the template via the {TOOL_TEMPLATE} tool.",
            template_system_additions()
        ),
        Kind::Teach => format!("{TEACH_SYSTEM}\n\nReturn the page via the {TOOL_TEACH} tool."),
        Kind::HintLadder => {
            format!("{HINT_SYSTEM}\n\nReturn the ladder via the {TOOL_HINT_LADDER} tool.")
        }
        Kind::Diagnosis => format!(
            "{}\n\nReturn the list via the {TOOL_DISTRACTORS} tool.",
            distractor_system()
        ),
    }
}

// --------------------------------------------------------------------------- //
// The tool schemas
// --------------------------------------------------------------------------- //

/// The `{"lit": n}` and nested-term grammar, stated in prose.
///
/// A term is recursive, and a recursive `$ref` is not read the same way by every
/// OpenAI-compatible endpoint, so the schema states the shape and the gate
/// decides it. The gate is the authority on every field here in any case: a
/// schema the provider honors still admits a constraint the core refuses.
const TERM_DESCRIPTION: &str = "A term: the parameter name as a bare string, {\"lit\": n} for an exact literal, or one of \
{\"add\": [term, ...]}, {\"sub\": [term, term]}, {\"mul\": [term, ...]}, {\"abs\": term}, \
{\"mod\": [term, term]}, {\"digit_sum\": term}.";

/// The distractor schema. The template tool and the distractor tool share it.
fn distractor_items() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["answer", "error_tag", "note"],
        "properties": {
            "answer": {
                "type": "string",
                "description":
                    "The wrong answer, as an expression over the declared parameters, so the \
    server computes it per instance.",
            },
            "error_tag": {"type": "string", "enum": MODEL_ERROR_TAGS},
            "note": {
                "type": "string",
                "description":
                    "1-2 sentences naming the misconception. It never states the correct answer.",
            },
        },
    })
}

/// The inclusive whole-number range of a rational domain's numerator and of its
/// denominator (`cadus_core::template::IntRange`).
fn int_range() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["low", "high"],
        "properties": {"low": {"type": "integer"}, "high": {"type": "integer"}},
    })
}

/// The parameter-domain schema: the four kinds of `cadus_core::template::Domain`.
fn params_schema() -> Value {
    json!({
        "type": "object",
        "description":
            "The placeholders and the values each one takes, by parameter name.",
        "additionalProperties": {
            "oneOf": [
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "low", "high"],
                    "properties": {
                        "kind": {"const": "int"},
                        "low": {"type": "integer"},
                        "high": {"type": "integer"},
                    },
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "values"],
                    "properties": {
                        "kind": {"const": "choice"},
                        "values": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": MAX_CHOICES,
                            "items": {"type": ["string", "integer"]},
                        },
                    },
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "num", "den"],
                    "properties": {
                        "kind": {"const": "rational"},
                        "num": int_range(),
                        "den": int_range(),
                    },
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "low", "high", "scale"],
                    "properties": {
                        "kind": {"const": "decimal"},
                        "low": {"type": "integer"},
                        "high": {"type": "integer"},
                        "scale": {"type": "integer", "minimum": 0, "maximum": MAX_DECIMAL_SCALE},
                    },
                },
            ],
        },
    })
}

/// The arguments schema of the template tool.
fn template_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "statement", "params", "constraints", "answer_expr",
            "solution_sketch", "hints", "distractors", "samples",
        ],
        "properties": {
            "statement": {
                "type": "string",
                "description":
                    "The problem statement with {name} placeholders for the values that vary, as \
    terminal-free prose with all math in $...$ KaTeX LaTeX. EVERY literal brace is doubled: write \
    $7^{{2}}$, because a single brace is a placeholder.",
            },
            "params": params_schema(),
            "constraints": {
                "type": "array",
                "description":
                    "The relations between parameters. Empty when the domains alone are right.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["op", "left", "right"],
                    "properties": {
                        "op": {"type": "string", "enum": CONSTRAINT_OPS},
                        "left": {"description": TERM_DESCRIPTION},
                        "right": {"description": TERM_DESCRIPTION},
                    },
                },
            },
            "answer_expr": {
                "type": "string",
                "description":
                    "The answer as an exact expression over the parameter names. The SERVER \
    computes every instance's answer from it, so it is exact over the whole domain.",
            },
            "solution_sketch": {
                "type": "string",
                "description":
                    "A 1-3 line method outline with the same {name} placeholders; math in $...$ \
    and literal braces doubled, exactly as in 'statement'.",
            },
            "hints": {
                "type": "array",
                "minItems": 1,
                "description":
                    "The hint ladder, widest rung first. No rung reveals the answer or the last \
    step.",
                "items": {"type": "string"},
            },
            "distractors": {
                "type": "array",
                "description":
                    "The wrong answers this knowledge point produces, with the mistake each one \
    names. Empty when you can name none.",
                "items": distractor_items(),
            },
            "samples": {
                "type": "array",
                "minItems": 1,
                "description":
                    "Worked instances that VERIFY your expression. For each, bind every parameter \
    and state the answer YOU compute by hand. The server evaluates answer_expr on the same bindings \
    and rejects the whole template if any one disagrees. COVERAGE IS CHECKED: the lowest AND the \
    highest value of every int parameter, every value of every choice parameter, and for every PAIR \
    of int parameters one crossed corner.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["params", "expected"],
                    "properties": {
                        "params": {
                            "type": "object",
                            "description": "A value for every declared parameter.",
                        },
                        "expected": {
                            "type": "string",
                            "description": "The answer for those values, computed by you.",
                        },
                    },
                },
            },
        },
    })
}

/// The arguments schema of one kind's tool, `additionalProperties: false`.
#[must_use]
pub fn tool_schema(kind: Kind) -> Value {
    match kind {
        Kind::Template => template_schema(),
        Kind::Teach => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["concept", "worked_example"],
            "properties": {
                "concept": {
                    "type": "string",
                    "description":
                        "1-2 sentences stating the method or the rule of this knowledge point \
        explicitly. Math in $...$ LaTeX.",
                },
                "worked_example": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["problem", "steps"],
                    "properties": {
                        "problem": {
                            "type": "string",
                            "description":
                                "A concrete example problem of this knowledge point's shape, with \
        DIFFERENT values from the exemplars.",
                        },
                        "steps": {
                            "type": "array",
                            "minItems": 1,
                            "description":
                                "The full solution, one step per entry, ending with the answer.",
                            "items": {"type": "string"},
                        },
                    },
                },
            },
        }),
        Kind::HintLadder => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["hints"],
            "properties": {
                "hints": {
                    "type": "array",
                    "minItems": 1,
                    "description":
                        "The rungs, widest first. No rung reveals the answer or the last step.",
                    "items": {"type": "string"},
                },
            },
        }),
        Kind::Diagnosis => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["distractors"],
            "properties": {
                "distractors": {
                    "type": "array",
                    "minItems": 1,
                    "items": distractor_items(),
                },
            },
        }),
    }
}

/// What the tool is for, in the model's words.
const fn tool_description(kind: Kind) -> &'static str {
    match kind {
        Kind::Template => {
            "Return the reusable STRUCTURE of this knowledge point's problems: the statement with \
placeholders, the values they range over, the relations between them, the answer as an \
expression, the hint ladder, the distractors, and the worked samples that verify it."
        }
        Kind::Teach => "Return the concept and one fully worked example for a knowledge point.",
        Kind::HintLadder => {
            "Return the ordered hint ladder of a knowledge point, widest rung first."
        }
        Kind::Diagnosis => {
            "Return the wrong answers a knowledge point produces, with the mistake each one names."
        }
    }
}

/// The forced tool of one kind.
#[must_use]
pub fn tool_spec(kind: Kind) -> ToolSpec {
    ToolSpec {
        name: kind.tool_name().to_owned(),
        description: tool_description(kind).to_owned(),
        parameters: tool_schema(kind),
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
