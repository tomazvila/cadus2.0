//! The 2.0 template document (A1, D-S4, C6).
//!
//! The document is the unit of content A1 names: the statement with holes, the
//! per-parameter domains, the constraints between parameters, the answer as an
//! evaluable expression, a solution sketch, a hint ladder, the optional
//! pre-authored distractors of A4, and the worked samples that verify the whole
//! thing.
//!
//! # What the body holds, and what the columns hold
//!
//! The body is `content_store.body` for `kind = 'template'`
//! (`migrations/0005_content.sql`). It holds the template document ONLY. It never
//! holds the knowledge-point id and it never holds the approval state, because
//! those are columns of the row, and a second copy of either would be a second
//! source of truth (C6).
//!
//! # The digest covers the whole body
//!
//! 1.0 hashes five keys of its payload and leaves `space_size` and the samples
//! out (`problem_templates.py:1040-1054`). 2.0's `content_store.digest` covers
//! the whole body, so the samples and the satisfying count are inside it (spec
//! section 8, trap 8). That is stricter and correct: a reviewer read them, so a
//! change to either asks for a new approval. Serde therefore round-trips the
//! document byte for byte, and every field keeps its wire spelling.
//!
//! # `deny_unknown_fields`
//!
//! Every struct of this module refuses an unknown key. An authoring model that
//! invents a field gets a rejection, not a silently dropped instruction.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::answer::ast::Ast;
use crate::curriculum::AnswerKind;
use crate::learner::problem_text_hash;

use super::constraint::{Constraint, constraint_params};
use super::domain::{Bindings, Params, Scalar, SpaceSize};
use super::draw::{DrawError, DrawPlan};
use super::eval::{Answer, EvalError, answer, parse_answer_expr};
use super::render::{RenderError, placeholders, render};

/// The version of the 2.0 template document.
///
/// A bump retires every stored document, the way 1.0's `TEMPLATE_VERSION` does
/// (`problem_templates.py:151`), because the digest covers the version.
pub const TEMPLATE_VERSION: u32 = 1;

/// One worked sample: a bound tuple and the answer a human wrote for it.
///
/// The samples are the verification oracle of the gate (A2). They stay in the
/// body, so the digest a reviewer approved covers them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    /// The bound tuple, by parameter name.
    pub params: std::collections::BTreeMap<String, Scalar>,
    /// The answer the sample claims.
    pub expected: Scalar,
}

impl Sample {
    /// The bindings the sample declares.
    #[must_use]
    pub fn bindings(&self) -> Bindings {
        self.params
            .iter()
            .map(|(name, scalar)| (name.clone(), scalar.value()))
            .collect()
    }
}

/// One pre-authored wrong answer and the mistake it names (A4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Distractor {
    /// The wrong answer, as an expression over the parameters.
    pub answer: String,
    /// The error tag the diagnosis reports.
    pub error_tag: String,
    /// The prose the learner reads. Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The 2.0 template document (`content_store.body`, `kind = 'template'`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateDoc {
    /// The document version. [`TEMPLATE_VERSION`] for a current document.
    pub v: u32,
    /// The policy captured with this item; absence preserves legacy semantics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_contract: Option<crate::answer::AnswerContract>,
    /// The topic whose exemplars the template mirrors.
    pub topic_id: String,
    /// The topic kind: numeric, expression, or contract-bearing multi-step.
    pub answer_kind: AnswerKind,
    /// The statement, with `{name}` holes and doubled literal braces.
    pub statement: String,
    /// The domain of every parameter, in name order.
    pub params: Params,
    /// The constraints between the parameters (A1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,
    /// The answer, as an expression over the parameter names.
    pub answer_expr: String,
    /// The worked solution the learner reads after an attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solution_sketch: Option<String>,
    /// The hint ladder. At least one rung, and no rung names the answer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    /// The pre-authored wrong answers (A4). Optional.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub distractors: Vec<Distractor>,
    /// The worked samples the gate verifies against.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<Sample>,
    /// The satisfying count, filled by the gate and never by a model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_size: Option<SpaceSize>,
}

impl TemplateDoc {
    /// Every parameter name the document declares, in name order.
    #[must_use]
    pub fn param_names(&self) -> BTreeSet<String> {
        self.params.keys().cloned().collect()
    }

    /// Every parameter name the constraints read, in name order.
    #[must_use]
    pub fn constraint_names(&self) -> BTreeSet<String> {
        constraint_params(&self.constraints)
    }

    /// Every placeholder name the statement writes, in name order.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::StrayBrace`] for a brace that is neither doubled
    /// nor part of a placeholder.
    pub fn statement_names(&self) -> Result<BTreeSet<String>, RenderError> {
        placeholders(&self.statement)
    }
}

/// One rendered instance: the statement, the answer, and how it was built.
///
/// The instance is what a pool row carries (D-S5). `instance_hash` is
/// [`problem_text_hash`] of the rendered statement and of nothing else: two
/// different templates of one knowledge point produce the same statement often
/// enough to matter, and the learner sees the statement (spec section 5.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    /// The reviewed policy carried to the pool and grade transaction.
    pub answer_contract: Option<crate::answer::AnswerContract>,
    /// The bound tuple that produced the instance.
    pub bindings: Bindings,
    /// The rendered statement, as the learner reads it.
    pub text: String,
    /// The expected answer, inside the M2 grammar.
    pub answer: String,
    /// The canonical form of the expected answer.
    pub canon: crate::answer::Canon,
    /// `problem_text_hash` of `text`.
    pub instance_hash: String,
}

/// An instance the instantiator refuses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InstantiateError {
    /// The statement did not render.
    #[error("{0}")]
    Render(#[from] RenderError),
    /// The answer did not evaluate, or it did not canonicalize.
    #[error("{0}")]
    Eval(#[from] EvalError),
    /// The draw found no tuple to instantiate.
    #[error("{0}")]
    Draw(#[from] DrawError),
}

/// A template document with its answer expression parsed once (spec 3.4).
///
/// The parse of `answer_expr` runs at authoring time. The serve path and the
/// refill job evaluate the tree this holds; neither reads the source again.
#[derive(Debug, Clone)]
pub struct Compiled<'doc> {
    /// The document.
    doc: &'doc TemplateDoc,
    /// The parsed answer expression.
    answer_ast: Ast,
    /// The materialized value list of every parameter.
    plan: DrawPlan,
}

impl<'doc> Compiled<'doc> {
    /// Parse the answer expression and materialize the domains.
    ///
    /// # Errors
    ///
    /// Returns [`Undecidable`] when `answer_expr` leaves the grammar (V2), and
    /// [`super::domain::DomainError`] when a domain is empty or past its bound.
    pub fn new(doc: &'doc TemplateDoc) -> Result<Self, InstantiateError> {
        let answer_ast = parse_answer_expr(&doc.answer_expr).map_err(EvalError::Grammar)?;
        let plan = DrawPlan::new(&doc.params).map_err(DrawError::Domain)?;
        Ok(Self {
            doc,
            answer_ast,
            plan,
        })
    }

    /// Build from a parsed answer expression and a materialized plan.
    ///
    /// The gate validated the domains already, so it builds the plan from the
    /// value lists it holds and skips a second read of the document.
    #[must_use]
    pub const fn from_parts(doc: &'doc TemplateDoc, answer_ast: Ast, plan: DrawPlan) -> Self {
        Self {
            doc,
            answer_ast,
            plan,
        }
    }

    /// The document this was compiled from.
    #[must_use]
    pub const fn doc(&self) -> &'doc TemplateDoc {
        self.doc
    }

    /// The parsed answer expression.
    #[must_use]
    pub const fn answer_ast(&self) -> &Ast {
        &self.answer_ast
    }

    /// The materialized value lists of the parameters.
    #[must_use]
    pub const fn plan(&self) -> &DrawPlan {
        &self.plan
    }

    /// Render and solve one bound tuple.
    ///
    /// The call runs the whole instantiation: render the statement, evaluate the
    /// answer, canonicalize it, and hash the statement. It is the unit of work
    /// the L1 benchmark measures.
    ///
    /// # Errors
    ///
    /// Returns [`InstantiateError`] when the statement holds a stray brace or an
    /// unbound placeholder, when the answer does not evaluate, and when the
    /// answer does not canonicalize (V2).
    pub fn instantiate(&self, bindings: Bindings) -> Result<Instance, InstantiateError> {
        let text = render(&self.doc.statement, &bindings)?;
        let Answer {
            text: answer,
            canon,
        } = answer(&self.answer_ast, &bindings)?;
        if let Some(contract) = self.doc.answer_contract {
            contract
                .validate_expected(&answer)
                .map_err(EvalError::Grammar)?;
        }
        let instance_hash = problem_text_hash(&text);
        Ok(Instance {
            answer_contract: self.doc.answer_contract,
            bindings,
            text,
            answer,
            canon,
            instance_hash,
        })
    }

    /// Draw one satisfying tuple and instantiate it.
    ///
    /// # Errors
    ///
    /// Returns [`InstantiateError`] for a constraint set no draw satisfied and
    /// for every case [`Compiled::instantiate`] refuses.
    pub fn draw(&self, rng: &mut rand_chacha::ChaCha8Rng) -> Result<Instance, InstantiateError> {
        let bindings = self.plan.draw_satisfying(&self.doc.constraints, rng)?;
        self.instantiate(bindings)
    }

    /// The candidate stream of one instantiation (spec section 3.1).
    ///
    /// # Errors
    ///
    /// Returns [`InstantiateError`] when a constraint cannot decide on a tuple.
    pub fn candidates(
        &self,
        rng: &mut rand_chacha::ChaCha8Rng,
    ) -> Result<Vec<Bindings>, InstantiateError> {
        Ok(super::draw::candidates(
            &self.doc.params,
            &self.doc.constraints,
            rng,
        )?)
    }
}

/// Read one template document from its JSON body.
///
/// # Errors
///
/// Returns the `serde_json` error of an unknown key, a missing key, a wrong
/// type, and a malformed constraint term.
pub fn from_body(body: &str) -> Result<TemplateDoc, serde_json::Error> {
    serde_json::from_str(body)
}

/// Write one template document as its JSON body.
///
/// # Errors
///
/// Returns the `serde_json` error of a value that does not serialize.
pub fn to_body(doc: &TemplateDoc) -> Result<String, serde_json::Error> {
    serde_json::to_string(doc)
}
