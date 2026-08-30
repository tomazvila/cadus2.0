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

use std::collections::{BTreeMap, BTreeSet};

use num_traits::{One, Signed};

use crate::answer::ast::Ast;
use crate::answer::{Canon, Outcome, canonical_form, same_answer};
use crate::curriculum::{AnswerKind, Exemplar};

use super::constraint::{Constraint, Term, all_hold, constraint_params, holds, term_params};
use super::document::{Compiled, Instance, InstantiateError, TEMPLATE_VERSION, TemplateDoc};
use super::domain::{
    Bindings, Domain, MAX_CHOICES, MAX_DECIMAL_SCALE, MAX_DOMAIN_SIZE, MIN_SPACE_SIZE, SpaceSize,
    Value, walk_satisfying,
};
use super::eval::{EvalError, answer as evaluate_answer, parse_answer_expr};
use super::render::{render, scan};

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
pub const RESERVED_NAMES: [&str; 48] = [
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
    "ceiling",
    "cos",
    "e",
    "exp",
    "expand",
    "factor",
    "factorial",
    "false",
    "floor",
    "gcd",
    "lcm",
    "ln",
    "log",
    "max",
    "min",
    "nsimplify",
    "pi",
    "sign",
    "simplify",
    "sin",
    "sqrt",
    "tan",
    "together",
    "true",
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
    fn new(code: &'static str, message: String) -> Self {
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
    let compiled = compile(doc)?;
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

// --------------------------------------------------------------------------
// Rows v, 0, 0b, 1, 2 — the document itself
// --------------------------------------------------------------------------

/// The document version, the answer kind, and the two required strings.
fn check_document(doc: &TemplateDoc, spec: &GateSpec) -> Result<(), Rejection> {
    if doc.v != TEMPLATE_VERSION {
        return Err(Rejection::new(
            "template-version",
            format!(
                "template version {} is not the current version {TEMPLATE_VERSION}",
                doc.v
            ),
        ));
    }
    if !TEMPLATABLE_KINDS.contains(&spec.answer_kind) {
        return Err(Rejection::new(
            "answer-kind",
            format!(
                "answer kind {} is not symbolically decidable",
                spec.answer_kind
            ),
        ));
    }
    if doc.answer_kind != spec.answer_kind {
        return Err(Rejection::new(
            "answer-kind-mismatch",
            format!(
                "the document declares answer kind {} and the knowledge point declares {}",
                doc.answer_kind, spec.answer_kind
            ),
        ));
    }
    if doc.statement.trim().is_empty() {
        return Err(Rejection::new(
            "text-empty",
            "template text is missing or empty".to_string(),
        ));
    }
    if doc.answer_expr.trim().is_empty() {
        return Err(Rejection::new(
            "answer-expr-empty",
            "answer_expr is missing or empty".to_string(),
        ));
    }
    Ok(())
}

// --------------------------------------------------------------------------
// Rows 3 to 9 — the parameters and their domains
// --------------------------------------------------------------------------

/// The parameters, their names, and the value list of every domain.
fn check_params(
    doc: &TemplateDoc,
    spec: &GateSpec,
) -> Result<BTreeMap<String, Vec<Value>>, Rejection> {
    if doc.params.is_empty() {
        return Err(Rejection::new(
            "no-parameter",
            "a template needs at least one parameter".to_string(),
        ));
    }
    let mut values = BTreeMap::new();
    for (name, domain) in &doc.params {
        if !is_identifier(name) {
            return Err(Rejection::new(
                "parameter-name",
                format!("parameter name {} is not an identifier", py_str(name)),
            ));
        }
        if RESERVED_NAMES.contains(&name.as_str()) {
            return Err(Rejection::new(
                "parameter-collision",
                format!(
                    "parameter name {} collides with a SymPy function the answer expression may call",
                    py_str(name)
                ),
            ));
        }
        if spec.answer_kind == AnswerKind::Expression && FREE_SYMBOLS.contains(&name.as_str()) {
            return Err(Rejection::new(
                "unknown-collision",
                format!(
                    "parameter name {} collides with the unknown an expression answer is written in",
                    py_str(name)
                ),
            ));
        }
        values.insert(name.clone(), domain_values(name, domain)?);
    }
    Ok(values)
}

/// The values of one domain, or the rejection its shape earns.
fn domain_values(name: &str, domain: &Domain) -> Result<Vec<Value>, Rejection> {
    if let Domain::Choice { values } = domain {
        if values.is_empty() {
            return Err(Rejection::new(
                "choice-domain",
                "a choice domain needs a non-empty 'values' list".to_string(),
            ));
        }
        if values.len() > MAX_CHOICES {
            return Err(Rejection::new(
                "choice-domain",
                format!(
                    "a choice domain of {} exceeds MAX_CHOICES ({MAX_CHOICES}); every choice must appear in a worked sample, so use an int domain or split the template",
                    values.len()
                ),
            ));
        }
    }
    domain.values(name).map_err(|_| domain_rejection(domain))
}

/// The rejection a domain the value walk refused earns.
fn domain_rejection(domain: &Domain) -> Rejection {
    match domain {
        Domain::Int { low, high } => {
            if high < low {
                return Rejection::new("int-domain", format!("int domain {low}..{high} is empty"));
            }
            Rejection::new(
                "domain-size",
                format!("int domain {low}..{high} exceeds MAX_DOMAIN_SIZE"),
            )
        }
        Domain::Choice { values } => Rejection::new(
            "choice-domain",
            format!(
                "a choice domain of {} exceeds MAX_CHOICES ({MAX_CHOICES}); every choice must appear in a worked sample, so use an int domain or split the template",
                values.len()
            ),
        ),
        Domain::Rational { num, den } => {
            if den.low <= 0 && den.high >= 0 {
                return Rejection::new(
                    "rational-domain",
                    format!(
                        "the denominator range {}..{} of a rational domain holds zero",
                        den.low, den.high
                    ),
                );
            }
            if num.high < num.low || den.high < den.low {
                return Rejection::new(
                    "rational-domain",
                    format!(
                        "rational domain {}..{} over {}..{} is empty",
                        num.low, num.high, den.low, den.high
                    ),
                );
            }
            Rejection::new(
                "domain-size",
                format!(
                    "rational domain {}..{} over {}..{} exceeds MAX_DOMAIN_SIZE ({MAX_DOMAIN_SIZE})",
                    num.low, num.high, den.low, den.high
                ),
            )
        }
        Domain::Decimal { low, high, scale } => {
            if *scale > MAX_DECIMAL_SCALE {
                return Rejection::new(
                    "decimal-domain",
                    format!(
                        "decimal domain scale {scale} exceeds MAX_DECIMAL_SCALE ({MAX_DECIMAL_SCALE})"
                    ),
                );
            }
            if high < low {
                return Rejection::new(
                    "decimal-domain",
                    format!("decimal domain {low}..{high} at scale {scale} is empty"),
                );
            }
            Rejection::new(
                "domain-size",
                format!("decimal domain {low}..{high} exceeds MAX_DOMAIN_SIZE ({MAX_DOMAIN_SIZE})"),
            )
        }
    }
}

// --------------------------------------------------------------------------
// 2.0: the constraint language reads declared, whole-number parameters
// --------------------------------------------------------------------------

/// Every constraint names declared parameters, and whole-number ones where it must.
fn check_constraint_shape(
    doc: &TemplateDoc,
    values: &BTreeMap<String, Vec<Value>>,
) -> Result<(), Rejection> {
    for name in constraint_params(&doc.constraints) {
        if !doc.params.contains_key(&name) {
            return Err(Rejection::new(
                "constraint-parameter",
                format!(
                    "a constraint term names undeclared parameter {}",
                    py_str(&name)
                ),
            ));
        }
    }
    for constraint in &doc.constraints {
        for name in whole_number_params(constraint) {
            let whole = values
                .get(&name)
                .is_some_and(|list| list.iter().all(|value| value.as_integer().is_some()));
            if !whole {
                return Err(Rejection::new(
                    "constraint-whole",
                    format!(
                        "the {} constraint reads whole numbers, and parameter {} draws values that are not whole",
                        constraint.op.as_str(),
                        py_str(&name)
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Every parameter one constraint reads in a whole-number position.
fn whole_number_params(constraint: &Constraint) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if constraint.op.needs_whole_numbers() {
        names.extend(term_params(&constraint.left));
        names.extend(term_params(&constraint.right));
    }
    collect_whole_terms(&constraint.left, &mut names);
    collect_whole_terms(&constraint.right, &mut names);
    names
}

/// Walk a term and collect the names `mod` and `digit_sum` read.
fn collect_whole_terms(term: &Term, names: &mut BTreeSet<String>) {
    match term {
        Term::Mod(left, right) => {
            names.extend(term_params(left));
            names.extend(term_params(right));
        }
        Term::DigitSum(inner) => names.extend(term_params(inner)),
        Term::Add(items) | Term::Mul(items) => {
            for item in items {
                collect_whole_terms(item, names);
            }
        }
        Term::Sub(left, right) => {
            collect_whole_terms(left, names);
            collect_whole_terms(right, names);
        }
        Term::Abs(inner) => collect_whole_terms(inner, names),
        Term::Param(_) | Term::Lit(_) => {}
    }
}

// --------------------------------------------------------------------------
// Rows 10 to 12, and the 2.0 hint ladder
// --------------------------------------------------------------------------

/// Every rendered field names declared parameters and doubles its literal braces.
fn check_rendered_fields(doc: &TemplateDoc) -> Result<(), Rejection> {
    check_field(&doc.statement, doc, "text", "text")?;
    if let Some(sketch) = &doc.solution_sketch {
        check_field(sketch, doc, "text", "text")?;
    }
    if doc.hints.is_empty() {
        return Err(Rejection::new(
            "hint-missing",
            "a template needs at least one hint rung, and a hint may never give the answer away"
                .to_string(),
        ));
    }
    for (index, hint) in doc.hints.iter().enumerate() {
        let what = format!("hint {index}");
        check_field(hint, doc, &what, "hint")?;
    }
    for (index, distractor) in doc.distractors.iter().enumerate() {
        if let Some(note) = &distractor.note {
            let what = format!("distractor {index}");
            check_field(note, doc, &what, "distractor")?;
        }
    }
    Ok(())
}

/// One rendered field: no undeclared hole, and no undoubled brace.
///
/// 1.0 writes `text` in both messages, for the statement and for the solution
/// sketch alike (`problem_templates.py:513-522`). 2.0 keeps that word for those
/// two fields and names the hint or the distractor for the fields 2.0 adds.
fn check_field(field: &str, doc: &TemplateDoc, what: &str, code: &str) -> Result<(), Rejection> {
    let (names, stray) = scan(field);
    let undeclared: Vec<String> = names
        .into_iter()
        .filter(|name| !doc.params.contains_key(name))
        .collect();
    if !undeclared.is_empty() {
        return Err(Rejection::new(
            if code == "text" {
                "undeclared-parameter"
            } else if code == "hint" {
                "hint-placeholder"
            } else {
                "distractor-placeholder"
            },
            format!("{what} uses undeclared parameters {}", py_list(&undeclared)),
        ));
    }
    if let Some(brace) = stray {
        return Err(Rejection::new(
            if code == "text" {
                "unescaped-brace"
            } else if code == "hint" {
                "hint-placeholder"
            } else {
                "distractor-placeholder"
            },
            format!(
                "{what} has an unescaped brace at index {} ({}) — literal LaTeX braces must be doubled",
                brace.index,
                py_str(&brace.snippet)
            ),
        ));
    }
    Ok(())
}

// --------------------------------------------------------------------------
// 2.0: the answer expression is inside the grammar (V2)
// --------------------------------------------------------------------------

/// Parse `answer_expr` once, and refuse a source outside the grammar.
fn compile<'doc>(doc: &'doc TemplateDoc) -> Result<Compiled<'doc>, Rejection> {
    Compiled::new(doc).map_err(|err| match err {
        InstantiateError::Eval(EvalError::Grammar(reason)) => {
            if reason.reason == EXPONENT_REASON {
                Rejection::new(
                    "grammar",
                    format!(
                        "answer_expr {} exceeds the evaluation bound ({MAX_EXPONENT} is the largest exponent the grammar reads)",
                        py_str(&doc.answer_expr)
                    ),
                )
            } else {
                Rejection::new(
                    "grammar",
                    format!(
                        "answer_expr {} is outside the decidable grammar: {}",
                        py_str(&doc.answer_expr),
                        reason.reason
                    ),
                )
            }
        }
        other => Rejection::new("grammar", format!("answer_expr does not compile: {other}")),
    })
}

// --------------------------------------------------------------------------
// Rows 13 and 14 — dead parameters, and the names the answer reaches for
// --------------------------------------------------------------------------

/// Every declared parameter does work somewhere a learner sees.
///
/// 1.0 counts the statement, the solution sketch, and the answer expression
/// (`problem_templates.py:894-902`). 2.0 counts the hints and the distractor
/// notes too, because both are rendered and both are read. A parameter that
/// appears only in a constraint does not count: a constraint narrows the space
/// and shows the learner nothing.
fn check_dead_parameters(doc: &TemplateDoc, ast: &Ast) -> Result<(), Rejection> {
    let mut rendered = scan(&doc.statement).0;
    if let Some(sketch) = &doc.solution_sketch {
        rendered.extend(scan(sketch).0);
    }
    for hint in &doc.hints {
        rendered.extend(scan(hint).0);
    }
    for distractor in &doc.distractors {
        if let Some(note) = &distractor.note {
            rendered.extend(scan(note).0);
        }
    }
    let answer_names = ast_names(ast);
    let dead: Vec<String> = doc
        .params
        .keys()
        .filter(|name| !rendered.contains(*name) && !answer_names.contains(*name))
        .cloned()
        .collect();
    if !dead.is_empty() {
        return Err(Rejection::new(
            "dead-parameter",
            format!("parameters {} are declared but never used", py_list(&dead)),
        ));
    }
    check_hidden_parameters(doc, &rendered, &answer_names)
}

/// Every parameter the answer reads appears where a learner reads it.
///
/// A parameter that changes the answer and shows nowhere splits one printed
/// problem into several different correct answers: the pool keys an instance by
/// the statement digest, so it keeps one tuple of the many and serves the answer
/// of that one (M4 review 1, finding 4).
///
/// The rule reads NAMES, and it is one half of the C4 rule "two tuples that
/// render one statement must compute one answer". It is the half a document
/// answers before any tuple is walked, and it is strictly weaker than the rule
/// itself: two SHOWN parameters collide too when the statement writes them next
/// to each other (M4 review 2, finding 1). The other half is
/// [`check_one_answer_per_statement`], which groups the walked instances by
/// their digest.
fn check_hidden_parameters(
    doc: &TemplateDoc,
    rendered: &BTreeSet<String>,
    answer_names: &BTreeSet<String>,
) -> Result<(), Rejection> {
    for name in doc.params.keys() {
        if answer_names.contains(name) && !rendered.contains(name) {
            return Err(Rejection::new(
                "hidden-parameter",
                format!(
                    "parameter {} changes the answer but never appears in the statement",
                    py_str(name)
                ),
            ));
        }
    }
    Ok(())
}

/// The answer expression reaches for nothing outside the parameters.
fn check_answer_names(doc: &TemplateDoc, ast: &Ast, spec: &GateSpec) -> Result<(), Rejection> {
    let unknown: Vec<String> = ast_names(ast)
        .into_iter()
        .filter(|name| !doc.params.contains_key(name))
        .filter(|name| !RESERVED_NAMES.contains(&name.as_str()))
        .filter(|name| {
            spec.answer_kind != AnswerKind::Expression || !FREE_SYMBOLS.contains(&name.as_str())
        })
        .collect();
    if !unknown.is_empty() {
        return Err(Rejection::new(
            "unknown-names",
            format!("answer_expr references unknown names {}", py_list(&unknown)),
        ));
    }
    Ok(())
}

/// Every variable and function name the parsed answer expression writes.
fn ast_names(ast: &Ast) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_ast_names(ast, &mut names);
    names
}

/// Walk the tree and collect its names.
fn collect_ast_names(ast: &Ast, names: &mut BTreeSet<String>) {
    match ast {
        Ast::Var(name) => {
            names.insert(name.clone());
        }
        Ast::Func(name, args) => {
            names.insert(name.clone());
            for arg in args {
                collect_ast_names(arg, names);
            }
        }
        Ast::Neg(inner) | Ast::Sqrt(inner) | Ast::Pow(inner, _) => collect_ast_names(inner, names),
        Ast::Add(items)
        | Ast::Mul(items)
        | Ast::Tuple(items)
        | Ast::Set(items)
        | Ast::List(items) => {
            for item in items {
                collect_ast_names(item, names);
            }
        }
        Ast::Div(left, right) => {
            collect_ast_names(left, names);
            collect_ast_names(right, names);
        }
        Ast::Interval { lo, hi, .. } => {
            collect_ast_names(lo, names);
            collect_ast_names(hi, names);
        }
        Ast::Ineq { var, bound, .. } => {
            names.insert(var.clone());
            collect_ast_names(bound, names);
        }
        Ast::Assign { var, value } => {
            names.insert(var.clone());
            collect_ast_names(value, names);
        }
        Ast::Chain { lo, var, hi, .. } => {
            names.insert(var.clone());
            collect_ast_names(lo, names);
            collect_ast_names(hi, names);
        }
        Ast::Integer(_) | Ast::Decimal { .. } | Ast::Fraction { .. } | Ast::Mixed { .. } => {}
        Ast::Const(constant) => {
            names.insert(constant.name().to_string());
        }
    }
}

// --------------------------------------------------------------------------
// Row 15 and the 2.0 satisfying count
// --------------------------------------------------------------------------

/// The tuples the gate reads, and the count it stores.
struct Walk {
    /// The satisfying count: exact below the limit, the found count above it.
    space: SpaceSize,
    /// The distinct tuples the instance check reads.
    tuples: Vec<Bindings>,
    /// True when `tuples` is every satisfying tuple.
    exhaustive: bool,
    /// The count of tuples the sampled walk drew, and `None` below the limit.
    drawn: Option<u32>,
    /// What the walk did not do, and why.
    notes: Vec<String>,
}

/// Count the satisfying tuples and collect the ones the gate reads.
///
/// The count and the tuples come from ONE call, so the number the body records
/// is the count of distinct instances the walk actually found (M4 review 2,
/// findings 2 and 5).
fn build_walk(doc: &TemplateDoc) -> Result<Walk, Rejection> {
    let walked = walk_satisfying(&doc.params, &doc.constraints).map_err(|err| {
        Rejection::new(
            "domain-size",
            format!("the declared domains do not count: {err}"),
        )
    })?;
    let mut notes = Vec::new();
    if let Some(drawn) = walked.drawn {
        let found = walked.tuples.len();
        if u32::try_from(found).unwrap_or(u32::MAX) < GATE_SAMPLES {
            notes.push(format!(
                "the sampled walk found {found} satisfying tuple(s) in {drawn} draw(s), and the instance check read those {found}"
            ));
        }
    }
    Ok(Walk {
        space: walked.space,
        tuples: walked.tuples,
        exhaustive: walked.exhaustive,
        drawn: walked.drawn,
        notes,
    })
}

/// The satisfying set is not empty, the stored count is the true one, and the
/// space clears the distinct-problem floor.
fn check_space(doc: &TemplateDoc, walk: &Walk) -> Result<(), Rejection> {
    if walk.tuples.is_empty() {
        // The sampled branch names what it did, because it read a sample and not
        // the whole space (M4 review 1, finding 12).
        let message = match walk.drawn {
            None => "the constraints refuse every tuple of the declared domains, so the template has no instance to serve".to_string(),
            Some(drawn) => format!(
                "the sampled walk drew {drawn} tuple(s) of the declared domains and 0 satisfied the constraints, so the template has no instance to serve"
            ),
        };
        return Err(Rejection::new("no-satisfying-tuple", message));
    }
    if let Some(stated) = doc.space_size
        && (stated.count() != walk.space.count() || stated.is_exact() != walk.space.is_exact())
    {
        return Err(Rejection::new(
            "space-size",
            format!(
                "space_size states {} and the gate counts {} — the gate fills space_size, not the author",
                stated.count(),
                walk.space.count()
            ),
        ));
    }
    let count = walk.space.count();
    if count < MIN_SPACE_SIZE {
        return Err(Rejection::new(
            "space-floor",
            format!(
                "the declared domains produce only {count} distinct problem(s); at least {MIN_SPACE_SIZE} are needed for randomized values and for avoidance of a recently-served problem to mean anything (Hard Rule 4)"
            ),
        ));
    }
    Ok(())
}

// --------------------------------------------------------------------------
// Rows 16 to 19b, and the 2.0 constraint check on the samples
// --------------------------------------------------------------------------

/// Every worked sample agrees with what the server computes.
fn check_samples(
    doc: &TemplateDoc,
    compiled: &Compiled<'_>,
    spec: &GateSpec,
) -> Result<Vec<Bindings>, Rejection> {
    if doc.samples.is_empty() {
        return Err(Rejection::new(
            "no-samples",
            "a template needs worked samples to verify it".to_string(),
        ));
    }
    let declared: Vec<String> = doc.params.keys().cloned().collect();
    let mut validated = Vec::with_capacity(doc.samples.len());
    for sample in &doc.samples {
        let bound: Vec<String> = sample.params.keys().cloned().collect();
        if bound != declared {
            return Err(Rejection::new(
                "sample-binding",
                format!(
                    "sample binds {}, template declares {}",
                    py_list(&bound),
                    py_list(&declared)
                ),
            ));
        }
        let bindings = sample.bindings();
        let computed = evaluate_answer(compiled.answer_ast(), &bindings).map_err(|err| {
            Rejection::new(
                "sample-eval",
                format!(
                    "answer_expr failed on sample {}: {err}",
                    py_bindings(&bindings)
                ),
            )
        })?;
        let claimed = sample.expected.text();
        let agrees = matches!(
            crate::answer::check(&computed.text, &claimed, spec.answer_kind),
            Outcome::Decided(verdict) if verdict.correct
        );
        if !agrees {
            return Err(Rejection::new(
                "sample-agreement",
                format!(
                    "answer_expr gives {} for {} but the sample claims {} — the expression does not compute the stated answer",
                    py_str(&computed.text),
                    py_bindings(&bindings),
                    py_str(&claimed)
                ),
            ));
        }
        validated.push(bindings);
    }
    Ok(validated)
}

/// Every distractor parses, carries a tag, and is not the right answer.
fn check_distractors(doc: &TemplateDoc, compiled: &Compiled<'_>) -> Result<(), Rejection> {
    for (index, distractor) in doc.distractors.iter().enumerate() {
        if distractor.error_tag.trim().is_empty() {
            return Err(Rejection::new(
                "distractor",
                format!("distractor {index} carries no error_tag"),
            ));
        }
        let wrong = parse_answer_expr(&distractor.answer).map_err(|reason| {
            Rejection::new(
                "distractor",
                format!(
                    "distractor {index} answers {}, which is outside the decidable grammar: {}",
                    py_str(&distractor.answer),
                    reason.reason
                ),
            )
        })?;
        for sample in &doc.samples {
            let bindings = sample.bindings();
            let (Ok(wrong_value), Ok(right_value)) = (
                evaluate_answer(&wrong, &bindings),
                evaluate_answer(compiled.answer_ast(), &bindings),
            ) else {
                continue;
            };
            if same_answer(&right_value.canon, &wrong_value.canon) {
                return Err(Rejection::new(
                    "distractor",
                    format!(
                        "distractor {index} answers {} for {}, which is the right answer — a distractor names a mistake",
                        py_str(&wrong_value.text),
                        py_bindings(&bindings)
                    ),
                ));
            }
        }
    }
    Ok(())
}

// --------------------------------------------------------------------------
// Rows 20 to 23 — the samples lie inside the space and exercise it
// --------------------------------------------------------------------------

/// The two ends of one ordered axis, and the declared ends when they differ.
#[derive(Debug, Clone)]
struct AxisEnds {
    /// The low end the coverage rule asks a worked sample for.
    low: Value,
    /// The high end the coverage rule asks a worked sample for.
    high: Value,
    /// The declared ends, when a constraint names the axis.
    ///
    /// A sample at a declared end covers that end too: the walk above the
    /// exhaustive limit is a sample and it misses a reachable end, so a correct
    /// worked sample is never asked to move inward (M4 review 2, finding 10).
    declared: Option<(Value, Value)>,
}

/// The lowest and the highest value of every ordered axis.
///
/// The rule splits on the constraints, because both failures of 2.0 came from
/// reading ONE set for both cases (M4 review 1, finding 16; M4 review 2, finding
/// 10):
///
/// - An axis NO constraint names reads its DECLARED ends. Every declared value
///   of such an axis lies in a satisfying tuple, so both ends are reachable, and
///   the sampled walk's own minimum is an artifact of [`GATE_SEED`] that no
///   author can read off the document.
/// - An axis a constraint names reads the ends of the satisfying set. A declared
///   end the constraints forbid would ask the author for a worked sample the
///   sample-constraint rule then refuses, and no author input clears both. The
///   declared ends travel with the axis, so a worked sample AT one of them
///   covers that end as well.
fn axis_extremes(
    doc: &TemplateDoc,
    values: &BTreeMap<String, Vec<Value>>,
    walk: &Walk,
) -> BTreeMap<String, AxisEnds> {
    let constrained = constraint_params(&doc.constraints);
    let mut extremes = BTreeMap::new();
    for (name, domain) in &doc.params {
        if !matches!(
            domain,
            Domain::Int { .. } | Domain::Rational { .. } | Domain::Decimal { .. }
        ) {
            continue;
        }
        let declared = values.get(name).and_then(|list| {
            let (Some(low), Some(high)) = (list.iter().min(), list.iter().max()) else {
                return None;
            };
            Some((low.clone(), high.clone()))
        });
        if !constrained.contains(name) {
            if let Some((low, high)) = declared {
                extremes.insert(
                    name.clone(),
                    AxisEnds {
                        low,
                        high,
                        declared: None,
                    },
                );
            }
            continue;
        }
        let seen: Vec<Value> = walk
            .tuples
            .iter()
            .filter_map(|tuple| tuple.get(name).cloned())
            .collect();
        let (Some(low), Some(high)) = (seen.iter().min(), seen.iter().max()) else {
            continue;
        };
        extremes.insert(
            name.clone(),
            AxisEnds {
                low: low.clone(),
                high: high.clone(),
                declared,
            },
        );
    }
    extremes
}

/// The samples lie inside their own domains and exercise every axis.
fn check_coverage(
    doc: &TemplateDoc,
    values: &BTreeMap<String, Vec<Value>>,
    extremes: &BTreeMap<String, AxisEnds>,
    samples: &[Bindings],
    walk: &Walk,
    notes: &mut Vec<String>,
) -> Result<(), Rejection> {
    for (index, sample) in doc.samples.iter().enumerate() {
        for (name, scalar) in &sample.params {
            let inside = values
                .get(name)
                .is_some_and(|list| list.contains(&scalar.value()));
            if !inside {
                return Err(Rejection::new(
                    "sample-domain",
                    format!(
                        "sample {index} binds {name}={}, which its own domain cannot produce — a sample outside the domain verifies nothing",
                        scalar_repr(scalar)
                    ),
                ));
            }
        }
    }
    for (index, bindings) in samples.iter().enumerate() {
        for constraint in &doc.constraints {
            if !holds(constraint, bindings).map_err(constraint_rejection)? {
                return Err(Rejection::new(
                    "sample-constraint",
                    format!(
                        "sample {index} binds {}, which the {} constraint refuses — a sample outside the constraints verifies nothing",
                        py_bindings(bindings),
                        constraint.op.as_str()
                    ),
                ));
            }
        }
    }
    for (name, domain) in &doc.params {
        let seen: BTreeSet<Value> = samples
            .iter()
            .filter_map(|bindings| bindings.get(name).cloned())
            .collect();
        if matches!(domain, Domain::Choice { .. }) {
            // The rule reads the choices of the SATISFYING set, and never the
            // declared list: a declared choice the constraints forbid asks for a
            // worked sample the sample-constraint rule then refuses, and no
            // author input clears both (M4 review 2, findings 3 and 9).
            let reachable: BTreeSet<Value> = walk
                .tuples
                .iter()
                .filter_map(|tuple| tuple.get(name).cloned())
                .collect();
            let missing: Vec<String> = reachable
                .iter()
                .filter(|value| !seen.contains(*value))
                .map(Value::canonical_string)
                .collect();
            if !missing.is_empty() {
                return Err(Rejection::new(
                    "choice-coverage",
                    format!(
                        "no worked sample uses {name}={} — every choice must appear in a sample, or the expression is unverified for it",
                        py_list(&missing)
                    ),
                ));
            }
            continue;
        }
        let Some(ends) = extremes.get(name) else {
            continue;
        };
        let declared_low = ends.declared.as_ref().map(|(low, _)| low);
        let declared_high = ends.declared.as_ref().map(|(_, high)| high);
        for (edge, declared_edge, which) in [
            (&ends.low, declared_low, "low"),
            (&ends.high, declared_high, "high"),
        ] {
            let covered =
                seen.contains(edge) || declared_edge.is_some_and(|value| seen.contains(value));
            if !covered {
                return Err(Rejection::new(
                    "edge-coverage",
                    format!(
                        "no worked sample uses the {which} end of {name} ({}) — the edges are where an expression stops being right",
                        edge.canonical_string()
                    ),
                ));
            }
        }
    }
    check_crossed_corners(doc, extremes, samples, walk, notes)
}

/// Every pair of ordered axes needs a sample at OPPOSITE ends.
///
/// Per-axis edges are satisfied by `(low, low)` and `(high, high)`, and that
/// diagonal is exactly where a swapped-operand expression agrees with the right
/// one. 1.0 records the live case at `:663-680`: `a**b` authored as `b**a` passed
/// on 50 of 50 seeds and then graded a correct learner wrong on 18 of 30 served
/// problems.
fn check_crossed_corners(
    doc: &TemplateDoc,
    extremes: &BTreeMap<String, AxisEnds>,
    samples: &[Bindings],
    walk: &Walk,
    notes: &mut Vec<String>,
) -> Result<(), Rejection> {
    let names: Vec<&String> = extremes.keys().collect();
    for (position, left) in names.iter().enumerate() {
        for right in names.iter().skip(position + 1) {
            let (Some(ends_l), Some(ends_r)) = (extremes.get(*left), extremes.get(*right)) else {
                continue;
            };
            let (low_l, high_l) = (&ends_l.low, &ends_l.high);
            let (low_r, high_r) = (&ends_r.low, &ends_r.high);
            if low_l == high_l || low_r == high_r {
                continue;
            }
            // The rule asks for a sample at one of two corners, so it applies
            // only when the constraints admit BOTH of them. A band constraint
            // admits neither, and the rule and the sample-constraint rule then
            // deadlock: the gate names two tuples that it refuses (M4 review 1,
            // finding 22).
            let mut unreachable = Vec::new();
            for (bound_l, bound_r) in [(low_l, high_r), (high_l, low_r)] {
                if !corner_holds(doc, walk, left, bound_l, right, bound_r) {
                    unreachable.push(format!(
                        "{left}={} with {right}={}",
                        bound_l.canonical_string(),
                        bound_r.canonical_string()
                    ));
                }
            }
            if !unreachable.is_empty() {
                notes.push(format!(
                    "the crossed-corner rule is skipped for {left} and {right}: the constraints admit no tuple at {}",
                    unreachable.join(", nor at ")
                ));
                continue;
            }
            let crossed = samples.iter().any(|bindings| {
                let (Some(bound_l), Some(bound_r)) = (bindings.get(*left), bindings.get(*right))
                else {
                    return false;
                };
                (bound_l == low_l && bound_r == high_r) || (bound_l == high_l && bound_r == low_r)
            });
            if !crossed {
                return Err(Rejection::new(
                    "crossed-corner",
                    format!(
                        "no worked sample crosses {left} and {right} — one of them at its low end WITH the other at its high end ({left}={} with {right}={}, or {left}={} with {right}={}). Matching corners are exactly where a swapped-operand expression looks right",
                        low_l.canonical_string(),
                        high_r.canonical_string(),
                        high_l.canonical_string(),
                        low_r.canonical_string()
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Whether a satisfying tuple binds `left` and `right` to the corner values.
///
/// The probe takes every tuple of the walk, overwrites the two axes with the
/// corner values, and asks the constraints. The other axes therefore carry
/// values that hold together, which is what makes the answer a statement about
/// the constraints and not about one guessed tuple.
fn corner_holds(
    doc: &TemplateDoc,
    walk: &Walk,
    left: &str,
    left_value: &Value,
    right: &str,
    right_value: &Value,
) -> bool {
    for tuple in &walk.tuples {
        let mut probe = tuple.clone();
        probe.insert(left.to_string(), left_value.clone());
        probe.insert(right.to_string(), right_value.clone());
        if all_hold(&doc.constraints, &probe).unwrap_or(false) {
            return true;
        }
    }
    false
}

// --------------------------------------------------------------------------
// Rows 24 to 28, and the 2.0 canonical round trip and hint rule
// --------------------------------------------------------------------------

/// One rendered statement, and the answers the walk computed for it.
struct Statement {
    /// The rendered text, as the learner reads it.
    text: String,
    /// The count of walked tuples that render this text.
    tuples: u64,
    /// The distinct canonical answers those tuples computed.
    ///
    /// Equality of two answers is equality of two [`Canon`] values, so a set of
    /// more than one element is a statement with more than one right answer.
    answers: BTreeSet<Canon>,
}

/// Every instance renders, solves, canonicalizes, and looks like the topic, and
/// one statement carries one answer.
fn check_instances(
    doc: &TemplateDoc,
    compiled: &Compiled<'_>,
    spec: &GateSpec,
    walk: &Walk,
) -> Result<(), Rejection> {
    let envelope = exemplar_envelope(spec.exemplars);
    let mut order: Vec<String> = Vec::new();
    let mut statements: BTreeMap<String, Statement> = BTreeMap::new();
    for bindings in &walk.tuples {
        let instance = instantiate(compiled, bindings)?;
        check_one_instance(doc, spec, envelope.as_ref(), &instance)?;
        match statements.get_mut(&instance.instance_hash) {
            Some(statement) => {
                statement.tuples = statement.tuples.saturating_add(1);
                statement.answers.insert(instance.canon);
            }
            None => {
                order.push(instance.instance_hash.clone());
                let mut answers = BTreeSet::new();
                answers.insert(instance.canon);
                statements.insert(
                    instance.instance_hash,
                    Statement {
                        text: instance.text,
                        tuples: 1,
                        answers,
                    },
                );
            }
        }
    }
    check_one_answer_per_statement(&order, &statements)
}

/// Two tuples that render ONE statement must compute ONE answer (C4).
///
/// The pool keys an instance by the digest of its statement
/// (`serving_pool.instance_hash`), so a statement two tuples render with two
/// different answers reaches a learner as ONE printed problem whose stored
/// answer is the one of whichever tuple the fill met first. A learner who reads
/// the other one answers correctly and the grade path records a failure.
///
/// The hidden-parameter rule is a proxy for this rule and a strictly weaker one:
/// it refuses a parameter the statement never shows, and two SHOWN parameters
/// collide as well when the statement writes them next to each other, for
/// example `${a}{b}$` with `a = 1, b = 12` and `a = 11, b = 2` (M4 review 2,
/// finding 1). The gate holds the rendered text and the canonical answer of
/// every walked tuple already, so the rule is a grouping of what it read.
///
/// The rule runs on the sampled walk too. Above the exhaustive limit it reads
/// the tuples the walk drew, so it refuses a collision the sample carries and it
/// says nothing about a collision the sample missed; `TemplateSource::fill`
/// refuses that one per batch.
fn check_one_answer_per_statement(
    order: &[String],
    statements: &BTreeMap<String, Statement>,
) -> Result<(), Rejection> {
    for digest in order {
        let Some(statement) = statements.get(digest) else {
            continue;
        };
        if statement.answers.len() > 1 {
            let count = statement.tuples;
            return Err(Rejection::new(
                "statement-collision",
                format!(
                    "statement {} renders from {count} tuples with different answers",
                    py_str(&statement.text)
                ),
            ));
        }
    }
    Ok(())
}

/// Verify ONE instance against the per-instance rules of the gate.
///
/// The refill of D-O4 calls this on every instance it builds, because the gate
/// reads a sample of a large space and the refill draws from the same space: an
/// instance the gate never saw must meet the same rules before it enters the
/// pool (M4 review 1, findings 1, 2, and 15). The rules are the ones that read
/// the instance alone — the exemplar envelope, the non-answer tokens, the free
/// symbols, the decimal trailing-zero run, the hint give-away, and the canonical
/// round trip.
///
/// # Errors
///
/// Returns [`Rejection`] with the literal message of the rule that refused the
/// instance.
pub fn check_instance(
    doc: &TemplateDoc,
    spec: &GateSpec<'_>,
    instance: &Instance,
) -> Result<(), Rejection> {
    let envelope = exemplar_envelope(spec.exemplars);
    check_one_instance(doc, spec, envelope.as_ref(), instance)
}

/// The per-instance rules, with the envelope already read.
fn check_one_instance(
    doc: &TemplateDoc,
    spec: &GateSpec,
    envelope: Option<&Envelope>,
    instance: &Instance,
) -> Result<(), Rejection> {
    let bindings = &instance.bindings;
    if !scan(&instance.text).0.is_empty() {
        return Err(Rejection::new(
            "placeholder-left",
            "a rendered problem still contains a placeholder".to_string(),
        ));
    }
    if instance.answer.trim().is_empty() {
        return Err(Rejection::new(
            "empty-answer",
            format!(
                "instantiation for {} produced no answer",
                py_bindings(bindings)
            ),
        ));
    }
    let tokens = identifier_tokens(&instance.answer);
    let poisoned: Vec<String> = tokens
        .iter()
        .filter(|token| NON_ANSWERS.contains(&token.as_str()))
        .cloned()
        .collect();
    if !poisoned.is_empty() {
        return Err(Rejection::new(
            "not-a-number",
            format!(
                "instance {} answers {}, which is not a number ({})",
                py_bindings(bindings),
                py_str(&instance.answer),
                py_list(&poisoned)
            ),
        ));
    }
    if trailing_zero_run(&instance.answer) {
        return Err(Rejection::new(
            "decimal-answer",
            format!(
                "instance {} answers {}, which is a decimal with a trailing zero run — 2.0 answers hold exact values only (D6)",
                py_bindings(bindings),
                py_str(&instance.answer)
            ),
        ));
    }
    if spec.answer_kind == AnswerKind::Numeric {
        let leftover: Vec<String> = tokens
            .into_iter()
            .filter(|token| !RESERVED_NAMES.contains(&token.as_str()))
            .collect();
        if !leftover.is_empty() {
            return Err(Rejection::new(
                "free-symbol",
                format!(
                    "numeric answer {} for {} still contains {} — a parameter is undeclared",
                    py_str(&instance.answer),
                    py_bindings(bindings),
                    py_list(&leftover)
                ),
            ));
        }
        if let Some(envelope) = envelope {
            check_envelope(instance, envelope)?;
        }
    }
    check_canonical(instance)?;
    check_hints(doc, instance)
}

/// The answer string reads back as the canonical form the instance carries (V2).
fn check_canonical(instance: &Instance) -> Result<(), Rejection> {
    let read = canonical_form(&instance.answer).map_err(|reason| {
        Rejection::new(
            "undecidable-answer",
            format!(
                "instance {} answers {}, which the answer checker cannot decide: {} — every instance answer must canonicalize (V2)",
                py_bindings(&instance.bindings),
                py_str(&instance.answer),
                reason.reason
            ),
        )
    })?;
    if read != instance.canon {
        return Err(Rejection::new(
            "canonical-mismatch",
            format!(
                "instance {} answers {}, which does not read back as the canonical form the instance carries — the answer and its canonical form must agree (V2)",
                py_bindings(&instance.bindings),
                py_str(&instance.answer)
            ),
        ));
    }
    Ok(())
}

/// Render and solve one tuple, or say which step refused it.
fn instantiate(compiled: &Compiled<'_>, bindings: &Bindings) -> Result<Instance, Rejection> {
    compiled
        .instantiate(bindings.clone())
        .map_err(|err| match err {
            InstantiateError::Eval(EvalError::NotCanonical { text, reason }) => Rejection::new(
                "undecidable-answer",
                format!(
                    "instance {} answers {}, which the answer checker cannot decide: {} — every instance answer must canonicalize (V2)",
                    py_bindings(bindings),
                    py_str(&text),
                    reason.reason
                ),
            ),
            other => Rejection::new(
                "instantiation",
                format!(
                    "instantiation failed for {}: {other}",
                    py_bindings(bindings)
                ),
            ),
        })
}

/// The instance answer looks like the knowledge point's authored answers.
fn check_envelope(instance: &Instance, envelope: &Envelope) -> Result<(), Rejection> {
    let sign_refusal = || {
        Rejection::new(
            "envelope-sign",
            format!(
                "instance {} answers {}, but every authored answer for this knowledge point is non-negative — narrow the domains so no instance goes below zero",
                py_bindings(&instance.bindings),
                py_str(&instance.answer)
            ),
        )
    };
    let whole_refusal = || {
        Rejection::new(
            "envelope-integral",
            format!(
                "instance {} answers {}, but every authored answer for this knowledge point is a whole number",
                py_bindings(&instance.bindings),
                py_str(&instance.answer)
            ),
        )
    };
    match &instance.canon {
        Canon::Rational(value) => {
            if envelope.non_negative && value.numer().is_negative() {
                return Err(sign_refusal());
            }
            if envelope.integral && !value.denom().is_one() {
                return Err(whole_refusal());
            }
            Ok(())
        }
        // A surd or a formula is not a plain number, so only the integrality rule
        // can speak. 1.0 decides the same case the same way (`:775-781`).
        _ => {
            if envelope.integral {
                return Err(whole_refusal());
            }
            Ok(())
        }
    }
}

/// No hint rung names the answer of the instance it is shown with.
///
/// Hard Rule 3: a hint is Socratic and never the final step
/// (`docs/WEB_SERVICE.md:22-32`). The rule reads the RENDERED rung, because a
/// rung that writes `${a}` gives nothing away until an instance binds it.
///
/// A token the statement already shows is not given away: the learner is reading
/// it in the problem. Without that exemption the rule refuses a whole template
/// because one degenerate instance answers a digit the statement carries — `a` of
/// 1 in `Compute ${a}^{{2}}$.` answers `1`, and every rung that names `${a}` then
/// looks like a give-away.
fn check_hints(doc: &TemplateDoc, instance: &Instance) -> Result<(), Rejection> {
    if contains_token(&instance.text, &instance.answer) {
        return Ok(());
    }
    for (index, hint) in doc.hints.iter().enumerate() {
        let Ok(rendered) = render(hint, &instance.bindings) else {
            continue;
        };
        if contains_token(&rendered, &instance.answer) {
            return Err(Rejection::new(
                "hint-answer",
                format!(
                    "hint {index} reads {} for {}, which names the answer {} — a hint is a question, never the final step (Hard Rule 3)",
                    py_str(&rendered),
                    py_bindings(&instance.bindings),
                    py_str(&instance.answer)
                ),
            ));
        }
    }
    Ok(())
}

// --------------------------------------------------------------------------
// The body read, for the rows the typed read owns
// --------------------------------------------------------------------------

/// The 1.0 rejection a body that does not read as a document earns.
fn body_rejection(body: &str, err: &serde_json::Error) -> Rejection {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Rejection::new("body", format!("the template body is not JSON: {err}"));
    };
    if let Some(rejection) = params_rejection(&value) {
        return rejection;
    }
    if let Some(sketch) = value.get("solution_sketch")
        && !sketch.is_string()
        && !sketch.is_null()
    {
        return Rejection::new(
            "body",
            "solution_expr must be a string when present".to_string(),
        );
    }
    if let Some(samples) = value.get("samples").and_then(serde_json::Value::as_array) {
        for sample in samples {
            let Some(object) = sample.as_object() else {
                return Rejection::new("body", "a sample was not an object".to_string());
            };
            let params_ok = object
                .get("params")
                .is_some_and(serde_json::Value::is_object);
            let expected_ok = object
                .get("expected")
                .is_some_and(|value| value.is_string() || value.is_i64() || value.is_u64());
            if !params_ok || !expected_ok {
                return Rejection::new(
                    "body",
                    "a sample needs 'params' and a scalar 'expected'".to_string(),
                );
            }
        }
    }
    Rejection::new("body", format!("the template body does not read: {err}"))
}

/// The 1.0 rejection a malformed parameter domain earns.
fn params_rejection(value: &serde_json::Value) -> Option<Rejection> {
    let params = value.get("params")?.as_object()?;
    for domain in params.values() {
        let Some(object) = domain.as_object() else {
            return Some(Rejection::new(
                "body",
                "a parameter domain was not an object".to_string(),
            ));
        };
        match object.get("kind").and_then(serde_json::Value::as_str) {
            Some("int") => {
                let whole = |key: &str| {
                    object
                        .get(key)
                        .is_some_and(|end| end.is_i64() || end.is_u64())
                };
                if !whole("low") || !whole("high") {
                    return Some(Rejection::new(
                        "body",
                        "an int domain needs integer 'low' and 'high'".to_string(),
                    ));
                }
            }
            Some("choice") => {
                let Some(values) = object.get("values").and_then(serde_json::Value::as_array)
                else {
                    return Some(Rejection::new(
                        "body",
                        "a choice domain needs a non-empty 'values' list".to_string(),
                    ));
                };
                if values.is_empty() {
                    return Some(Rejection::new(
                        "body",
                        "a choice domain needs a non-empty 'values' list".to_string(),
                    ));
                }
                if !values
                    .iter()
                    .all(|entry| entry.is_string() || entry.is_i64() || entry.is_u64())
                {
                    return Some(Rejection::new(
                        "body",
                        "choice values must be strings or integers".to_string(),
                    ));
                }
            }
            Some("rational" | "decimal") => {}
            other => {
                let written = other.map_or_else(|| "None".to_string(), py_str);
                return Some(Rejection::new(
                    "body",
                    format!("unknown domain kind {written}"),
                ));
            }
        }
    }
    None
}

// --------------------------------------------------------------------------
// Small readers and writers
// --------------------------------------------------------------------------

/// The rejection an undecidable constraint earns.
fn constraint_rejection(err: super::constraint::ConstraintError) -> Rejection {
    Rejection::new("constraint-parameter", err.to_string())
}

/// Whether the name is a Python identifier (1.0 `str.isidentifier`).
///
/// The gate reads ASCII names. Python admits a wider set; a template whose
/// parameter is written in another script is refused here, and that is the
/// stricter reading.
fn is_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Every `[A-Za-z_][A-Za-z0-9_]*` run of a string, in name order.
fn identifier_tokens(text: &str) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    let mut current = String::new();
    for character in text.chars() {
        let starts = character.is_ascii_alphabetic() || character == '_';
        let continues = starts || character.is_ascii_digit();
        if current.is_empty() && starts {
            current.push(character);
            continue;
        }
        if !current.is_empty() && continues {
            current.push(character);
            continue;
        }
        if !current.is_empty() {
            tokens.insert(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.insert(current);
    }
    tokens
}

/// Whether the answer stands in the text as a token of its own.
///
/// A letter, a digit, or an underscore beside the run means the run is part of a
/// longer word or number, so `144` does not stand inside `1442`.
///
/// A point or a slash breaks the run only when a digit stands on its far side.
/// That one rule keeps three readings right at once: `16` stands at the end of
/// `is 16.`, because the point ends a sentence; `2` does not stand inside `1/2`,
/// because the slash carries the numerator; and `0` does not stand inside `0.5`,
/// because the point carries the decimal.
pub(crate) fn contains_token(text: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let characters: Vec<char> = text.chars().collect();
    let needle: Vec<char> = token.chars().collect();
    let last = characters.len().saturating_sub(needle.len());
    for start in 0..=last {
        let Some(window) = characters.get(start..start + needle.len()) else {
            continue;
        };
        if window != needle.as_slice() {
            continue;
        }
        let before = start.checked_sub(1);
        let after = start + needle.len();
        let free_before = before.is_none_or(|index| {
            let outer = index.checked_sub(1).and_then(|far| characters.get(far));
            free_side(characters.get(index), outer)
        });
        let free_after = free_side(characters.get(after), characters.get(after + 1));
        if free_before && free_after {
            return true;
        }
    }
    false
}

/// Whether one side of a run leaves the run standing on its own.
///
/// `near` is the character beside the run and `far` is the one beyond it.
fn free_side(near: Option<&char>, far: Option<&char>) -> bool {
    let Some(near) = near else {
        return true;
    };
    if near.is_ascii_alphanumeric() || *near == '_' {
        return false;
    }
    if *near == '.' || *near == '/' {
        return !far.is_some_and(char::is_ascii_digit);
    }
    true
}

/// Whether the text is a decimal that ends in a zero run (spec trap 3).
fn trailing_zero_run(text: &str) -> bool {
    let Some((_, fraction)) = text.split_once('.') else {
        return false;
    };
    !fraction.is_empty()
        && fraction.chars().all(|character| character.is_ascii_digit())
        && fraction.ends_with('0')
}

/// Write one string the way Python's `repr` writes it.
///
/// Every message the gate quotes carries a name, a snippet, or an answer, and
/// none of them holds a quote character, so the quote-switching rule of Python's
/// `repr` never applies: the writer always uses single quotes and escapes a
/// backslash and a quote.
pub(crate) fn py_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('\'');
    out
}

/// Write a name list the way Python writes a sorted list of strings.
fn py_list(items: &[String]) -> String {
    let written: Vec<String> = items.iter().map(|item| py_str(item)).collect();
    format!("[{}]", written.join(", "))
}

/// Write a bound tuple the way Python writes a dictionary.
///
/// A number writes its digits and a text writes its `repr`, so the message reads
/// `{'a': 59, 'b': 63}` for the tuple 1.0 refuses in the specification's live
/// run (section 4).
fn py_bindings(bindings: &Bindings) -> String {
    let written: Vec<String> = bindings
        .iter()
        .map(|(name, value)| match value {
            Value::Num(_) | Value::Spelled { .. } => {
                format!("{}: {}", py_str(name), value.canonical_string())
            }
            Value::Text(text) => format!("{}: {}", py_str(name), py_str(text)),
        })
        .collect();
    format!("{{{}}}", written.join(", "))
}

/// Write one sample scalar the way Python's `repr` writes it.
fn scalar_repr(scalar: &super::domain::Scalar) -> String {
    match scalar {
        super::domain::Scalar::Int(value) => value.to_string(),
        super::domain::Scalar::Text(text) => py_str(text),
    }
}
