//! The verification gate (A2, C6, V2).
//!
//! The gate is the machine half of C6. It reads one template document and either
//! returns the verified counts or refuses the document with a reason a human, and
//! an authoring model, can act on. A human reads the document after the gate
//! passes it; nothing reaches a learner that the gate refused.
//!
//! # Why the message text is the product
//!
//! The gate cannot prove a template correct. 1.0 states the proof of that at
//! `problem_templates.py:606-630`: every point the coverage rules force lies on a
//! domain boundary, so `a*b + (a-1)(12-a)(b-1)(12-b)` agrees with `a*b` on all of
//! them and is wrong on 69% of the interior; for ANY finite forced set the same
//! construction exists. Seven forced points were honored while 97 of 144
//! instances stayed wrong.
//!
//! So the gate does one thing well: it forces the author's own worked samples to
//! speak where a wrong expression most commonly still looks right — an operand
//! swap, a missing magnitude, a branch-blind operator, a domain that runs past
//! the topic. The rejection text is what makes the next attempt better, which is
//! why every message here is a literal and why the tests compare it byte for
//! byte (A2).
//!
//! # The 1.0 wording is kept
//!
//! Every message of the 28 rows of `docs/reference/serving-1.0-spec.md` section 4
//! is the 1.0 text, character for character, including the words `SymPy` and
//! `solution_expr`. 2.0 runs no computer-algebra system and spells the field
//! `solution_sketch`, so read those two words as the names of the 1.0 rule the
//! row ports. The reason to keep them is the ported fixture set: a reviewer who
//! knows the 1.0 rejections reads the same sentence here, and the 1.0 test file
//! ports as literals.
//!
//! # What 2.0 adds
//!
//! | Addition | Why |
//! |---|---|
//! | `answer_expr` parses inside the M2 grammar | V2, and the M5 grade path |
//! | every instance answer canonicalizes | V2; an answer the checker refuses grades nothing |
//! | every constraint holds on every sample | a sample outside the constraints verifies nothing |
//! | the constraints admit at least one tuple | a template with no instance must not be stored |
//! | `space_size` is the SATISFYING count | 1.0 stores the product and saturates it (spec trap 9) |
//! | one statement carries one answer | C4; the pool keys a row by the statement digest |
//! | a hint ladder of at least one rung, and no rung names the answer | Hard Rule 3 |
//! | the exemplar envelope reads exact rationals | 1.0 reads `float()` (spec trap 12) |
//! | every parameter the answer reads appears where a learner reads it | one statement must carry one answer (C4) |
//!
//! # The per-instance rules run twice
//!
//! Above [`super::domain::EXHAUSTIVE_SPACE_LIMIT`] the gate reads a SAMPLE of
//! the space, and the refill of D-O4 draws from the same space, so the refill
//! meets tuples the gate never saw. [`check_instance`] is the per-instance half
//! of the gate, and the refill runs it on every instance before the instance
//! enters the pool.
//!
//! # No panic, on any document
//!
//! Every step returns a [`Rejection`]. The gate indexes nothing, unwraps nothing,
//! and divides by nothing (C4). A document that is not a document at all is a
//! rejection from [`gate_body`], not a crash.

mod body;
mod coverage;
mod document;
mod fields;
mod instance;
mod space;
mod text;

use num_traits::{One, Signed};

use crate::answer::{Canon, canonical_form};
use crate::curriculum::{AnswerKind, Exemplar};

use super::document::TemplateDoc;
use super::domain::SpaceSize;
use body::body_rejection;
use coverage::{axis_extremes, check_coverage};
use document::{check_constraint_shape, check_document, check_params};
use fields::{check_answer_names, check_dead_parameters, check_rendered_fields, compile};
use instance::check_instances;
use space::{build_walk, check_distractors, check_samples, check_space};

pub use instance::check_instance;
pub(crate) use text::{contains_token, py_list, py_str};

/// The count of DISTINCT satisfying tuples the walk collects above the limit.
///
/// 1.0 draws `GATE_SAMPLES = 200` (`problem_templates.py:183`). 2.0 raises the
/// count to the exhaustive limit, so the sampled branch and the walked branch
/// read the same number of instances and cost the same work. The 1.0 number was
/// chosen for a Python loop that calls SymPy once per instance; the 2.0 loop
/// evaluates an already-parsed tree.
///
/// The walk stops here, so the count it records is a floor of the satisfying
/// count and never a number above it (M4 review 2, findings 2 and 5).
pub const GATE_SAMPLES: u32 = 4_096;

/// The largest count of tuples the sampled walk draws.
///
/// The walk keeps drawing until it holds [`GATE_SAMPLES`] satisfying tuples or
/// it spends this budget, so one sparse draw never ends the walk (M4 review 1,
/// findings 7 and 12). The budget bounds the work of the gate: a document whose
/// constraints almost never hold costs this many draws and no more.
pub const GATE_DRAW_BUDGET: u32 = 262_144;

/// The seed of the sampled branch.
///
/// The seed is a constant, so the whole gate is a function of the document and
/// the topic alone. A reviewer re-runs it and gets the same verdict, which is
/// what makes an approval mean something (C6).
pub const GATE_SEED: u64 = 0;

/// The largest exponent the M2 grammar reads (1.0 `_MAX_EXPONENT`).
///
/// The number is pinned here as a literal because the gate quotes it in the
/// rejection an exponent bomb earns. `crates/core/src/answer/parse.rs` holds the
/// same number as the bound it enforces.
pub const MAX_EXPONENT: i64 = 1_000;

/// The answer kinds a template may target (1.0 `TEMPLATABLE_KINDS`).
///
/// Exactly the two kinds the checker decides, so an instantiated problem is
/// gradable without a model (V2, T1).
pub const TEMPLATABLE_KINDS: [AnswerKind; 2] = [AnswerKind::Numeric, AnswerKind::Expression];

/// The names a parameter may not take (1.0 `_ALLOWED_NAMES`, plus the 2.0 set).
///
/// A parameter of one of these names shadows a function or a constant the answer
/// expression may call. 1.0 refuses them because it substitutes the value into
/// the source text; 2.0 substitutes into a tree, and the collision is still real:
/// `pi` and `e` parse as constants and never bind, and `min`, `max`, `gcd`, and
/// the rest are the evaluation-only functions of
/// [`super::eval::EVAL_FUNCTIONS`].
///
/// The 1.0 list is kept whole, so a name the 1.0 gate refused stays refused, and
/// the 2.0 names `e`, `min`, and `max` are added to it.
pub const RESERVED_NAMES: [&str; 61] = [
    "Abs",
    "And",
    "E",
    "Eq",
    "False",
    "Float",
    "Ge",
    "Gt",
    "ITE",
    "Integer",
    "Le",
    "Lt",
    "Max",
    "Min",
    "Ne",
    "Not",
    "Or",
    "Piecewise",
    "Rational",
    "S",
    "True",
    "abs",
    "binomial",
    "cancel",
    "divisibilitylabel",
    "equalitylabel",
    "ceiling",
    "cos",
    "e",
    "exp",
    "expand",
    "excludepoint",
    "factor",
    "factorial",
    "factorlist",
    "false",
    "firstmultiples",
    "floor",
    "gcd",
    "lcm",
    "lowerbound",
    "ln",
    "log",
    "max",
    "min",
    "multipart",
    "nsimplify",
    "pi",
    "primeclass",
    "primefactors",
    "quotientremainder",
    "repeatedfactors",
    "sign",
    "signcase",
    "simplify",
    "sin",
    "sqrt",
    "tan",
    "together",
    "true",
    "upperbound",
];

/// The conventional unknowns an `expression` answer is written in (1.0 `_FREE_SYMBOLS`).
///
/// `2*x + 1` has to mention `x`, and `x` is not a parameter: it is the unknown.
/// A parameter of that name is a collision for an `expression` answer and is not
/// a collision for a `numeric` one, where there is no free symbol to shadow.
pub const FREE_SYMBOLS: [&str; 11] = ["k", "n", "r", "t", "theta", "u", "v", "w", "x", "y", "z"];

/// The tokens an answer may never carry (1.0 `_NON_ANSWERS`).
///
/// `sqrt(-4)` answers `2*I` in 1.0 and `1/0` answers `zoo`. Both are strings the
/// server would hand a learner as the expected answer, against which every
/// attempt is wrong. 2.0 refuses both cases in the evaluator, so this row is the
/// belt beside that brace: it reads the answer string the writer produced.
pub const NON_ANSWERS: [&str; 9] = [
    "AccumBounds",
    "False",
    "I",
    "True",
    "false",
    "nan",
    "oo",
    "true",
    "zoo",
];

/// The refusal reason the M2 parser writes for an exponent past the bound.
///
/// The gate matches it, so an exponent bomb earns the 1.0 wording
/// `exceeds the evaluation bound` instead of the generic grammar refusal.
const EXPONENT_REASON: &str = "an exponent outside the evaluation bound";

/// What the gate needs about the knowledge point the template serves.
///
/// The topic owns the answer kind and the authored exemplars; the document owns
/// everything else. 1.0 reads the same two fields off `ProblemSpec`
/// (`problem_templates.py:839-940`).
#[derive(Debug, Clone, Copy)]
pub struct GateSpec<'kp> {
    /// The answer kind of the topic.
    pub answer_kind: AnswerKind,
    /// The authored exemplars of the knowledge point.
    pub exemplars: &'kp [Exemplar],
}

/// What every authored answer of a knowledge point looks like (1.0 `_exemplar_envelope`).
///
/// The envelope is what catches the template that computes correctly and is not
/// this knowledge point's problem. 1.0 records the live case at `:751-757`: asked
/// for subtraction with borrowing, the deployed model authored
/// `Compute ${a} - {b}$` over two independent 10..99 ranges, and half of the
/// instances answered negative.
///
/// 2.0 reads every exemplar answer through [`canonical_form`] and decides on the
/// exact rational. 1.0 reads it through `float()`, which is a float in a
/// correctness decision (spec trap 12, D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Envelope {
    /// Every authored answer is zero or above.
    pub non_negative: bool,
    /// Every authored answer is a whole number.
    pub integral: bool,
}

/// A template document the gate refuses.
///
/// The message is the product (A2). The code names the row, so a counter and an
/// operator view can group refusals without reading prose.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct Rejection {
    /// The short name of the check that refused the document.
    pub code: &'static str,
    /// The reason, in the words the author reads.
    pub message: String,
}

impl Rejection {
    /// Build a rejection.
    pub(super) fn new(code: &'static str, message: String) -> Self {
        Self { code, message }
    }
}

/// What the gate learned about a document it accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// The count of tuples the constraints admit. The gate fills it, not a model.
    pub space: SpaceSize,
    /// The count of instances the gate rendered and solved.
    pub instances_checked: u64,
    /// True when the gate walked every tuple, false when it drew [`GATE_SAMPLES`].
    pub exhaustive: bool,
    /// The checks the gate skipped, and the reason for each one.
    ///
    /// The core writes no log (R3), so the caller of the gate writes these lines
    /// to its own log. A skipped check is never a silent one (M4 review 1,
    /// findings 12 and 22).
    pub notes: Vec<String>,
}

/// Verify one template document against the knowledge point it serves.
///
/// The order of the checks is the order of
/// `docs/reference/serving-1.0-spec.md` section 4, with the 2.0 additions written
/// into the places their inputs become available. The first failure returns.
///
/// # Errors
///
/// Returns [`Rejection`] with the literal message of the check that refused the
/// document.
pub fn gate(doc: &TemplateDoc, spec: &GateSpec) -> Result<Verified, Rejection> {
    check_document(doc, spec)?;
    let values = check_params(doc, spec)?;
    check_constraint_shape(doc, &values)?;
    check_rendered_fields(doc)?;
    let compiled = compile(doc, &values)?;
    check_dead_parameters(doc, compiled.answer_ast())?;
    check_answer_names(doc, compiled.answer_ast(), spec)?;
    let walk = build_walk(doc)?;
    check_space(doc, &walk)?;
    let samples = check_samples(doc, &compiled, spec)?;
    check_distractors(doc, &compiled)?;
    let extremes = axis_extremes(doc, &values, &walk);
    let mut notes = walk.notes.clone();
    check_coverage(doc, &values, &extremes, &samples, &walk, &mut notes)?;
    check_instances(doc, &compiled, spec, &walk)?;
    Ok(Verified {
        space: walk.space,
        instances_checked: u64::try_from(walk.tuples.len()).unwrap_or(u64::MAX),
        exhaustive: walk.exhaustive,
        notes,
    })
}

/// Read one template body and verify it.
///
/// The typed read is the 2.0 spelling of the 1.0 shape rows: an int domain whose
/// ends are not whole numbers, a choice value that is neither a string nor a
/// whole number, a `solution_sketch` that is not a string, and a sample that is
/// not an object never build a [`TemplateDoc`]. This function turns those reads
/// into the 1.0 rejections, so an authoring model gets the same sentence for the
/// same mistake.
///
/// # Errors
///
/// Returns [`Rejection`] for a body that does not read as a document and for
/// every check [`gate`] runs.
pub fn gate_body(body: &str, spec: &GateSpec) -> Result<(TemplateDoc, Verified), Rejection> {
    let doc = super::document::from_body(body).map_err(|err| body_rejection(body, &err))?;
    let verified = gate(&doc, spec)?;
    Ok((doc, verified))
}

/// Write the document the gate accepted, with the satisfying count filled in.
///
/// `space_size` is the gate's field and never the model's
/// (`problem_templates.py:309`). The authoring pipeline stores what this returns.
#[must_use]
pub fn with_space_size(doc: &TemplateDoc, verified: &Verified) -> TemplateDoc {
    let mut filled = doc.clone();
    filled.space_size = Some(verified.space);
    filled
}

/// Read the envelope of a knowledge point's authored answers.
///
/// Returns `None` when the knowledge point has no exemplar, and when one
/// exemplar answer is not an exact number: then there is no envelope to read, and
/// a guess would refuse good templates. 1.0 states the asymmetry that justifies
/// the rule at `:758-763` — a wrongly refused template costs one live generation,
/// and a wrongly accepted one is served for as long as the row lives.
#[must_use]
pub fn exemplar_envelope(exemplars: &[Exemplar]) -> Option<Envelope> {
    let mut non_negative = true;
    let mut integral = true;
    let mut seen = false;
    for exemplar in exemplars {
        let Ok(Canon::Rational(value)) = canonical_form(&exemplar.answer) else {
            return None;
        };
        seen = true;
        if value.numer().is_negative() {
            non_negative = false;
        }
        if !value.denom().is_one() {
            integral = false;
        }
    }
    seen.then_some(Envelope {
        non_negative,
        integral,
    })
}
