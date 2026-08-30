//! Parameterized problem templates: the unit of content (A1, A5, D6, V2, T1).
//!
//! A1 inverts the 1.0 architecture. 1.0 asked a model for a problem on every
//! serve, so no two problems repeated and nothing was cacheable. 2.0 authors a
//! template once, verifies it by machine, has a human approve it (C6), and then
//! instantiates it locally forever. This module is the local instantiation: the
//! document, the constraint language, the renderer, the exact evaluator, the
//! domains, and the seeded draws.
//!
//! **No model call runs here, at authoring time or at serve time (T1).** No float
//! enters any value (D6). No step panics on any document (C4).
//!
//! # The eight pieces
//!
//! | Module | What it owns |
//! |---|---|
//! | [`document`] | the template document, its serde shape, and [`Compiled`] |
//! | [`constraint`] | the inter-parameter constraint language and its evaluator |
//! | [`domain`] | the parameter domains, the values, and `space_size` |
//! | [`render`] | the statement scanner over the `{name}` grammar |
//! | [`eval`] | the exact evaluator of `answer_expr` and the answer writer |
//! | [`draw`] | the seeded draws and the candidate stream |
//! | [`gate`] | the verification gate: every check, with the reason it writes |
//! | [`distractor`] | the `diagnosis` document of A4: its gate, and the match the grade path runs |
//!
//! # The path of one instance
//!
//! ```text
//! TemplateDoc ──Compiled::new──▶ answer_expr parsed once (M2 Ast) + DrawPlan
//!                                          │
//!                    draw ──▶ Bindings (every constraint holds)
//!                                          │
//!         render ──▶ statement     evaluate ──▶ exact value ──▶ answer string
//!                                          │
//!                          canonical_form(answer) must decide (V2)
//!                                          │
//!                    problem_text_hash(statement) ──▶ instance_hash (A5)
//! ```
//!
//! # What the module refuses
//!
//! - An `answer_expr` outside the M2 grammar plus [`eval::EVAL_FUNCTIONS`] (V2).
//! - An answer string the M2 canonicalizer cannot decide (V2, and the L2 grade
//!   path of M5 depends on it).
//! - A statement with an undoubled literal brace, or with a hole no parameter
//!   binds. 1.0 buys the second property with `format_map`'s `KeyError`; the
//!   scanner of [`render`] buys it with a rule.
//! - A drawn tuple a constraint refuses. The draw reports that it found none; it
//!   never returns the violating tuple.

pub mod constraint;
pub mod distractor;
pub mod document;
pub mod domain;
pub mod draw;
pub mod eval;
pub mod gate;
pub mod render;

pub use constraint::{
    Cmp, Constraint, ConstraintError, Term, all_hold, constraint_params, eval_term, holds,
    term_params,
};
pub use distractor::{
    DIAGNOSIS_VERSION, DiagnosisDoc, KIND_DIAGNOSIS, Preauthored, gate_diagnosis,
    gate_diagnosis_body, keep_known_tags, match_answer, read_distractors, to_diagnosis_body,
};
pub use document::{
    Compiled, Distractor, Instance, InstantiateError, Sample, TEMPLATE_VERSION, TemplateDoc,
    from_body, to_body,
};
pub use domain::{
    Bindings, Domain, DomainError, EXHAUSTIVE_SPACE_LIMIT, IntRange, MAX_CHOICES,
    MAX_DECIMAL_SCALE, MAX_DOMAIN_SIZE, MIN_SPACE_SIZE, Params, SatisfyingWalk, Scalar, SpaceSize,
    Value, declared_space, enumerate, literal_to_rational, space_size, walk_satisfying,
};
pub use draw::{
    DrawError, DrawPlan, MAX_REJECTIONS, RESAMPLE_ATTEMPTS, below, candidates, draw_bindings,
    draw_satisfying, rng_from_seed, shuffle,
};
pub use eval::{
    Answer, EVAL_FUNCTIONS, EXTRA_FUNCTIONS, EvalError, MAX_FACTORIAL, MAX_VALUE_BITS, answer,
    evaluate, parse_answer_expr, write,
};
pub use gate::{
    Envelope, FREE_SYMBOLS, GATE_DRAW_BUDGET, GATE_SAMPLES, GATE_SEED, GateSpec, MAX_EXPONENT,
    NON_ANSWERS, RESERVED_NAMES, Rejection, TEMPLATABLE_KINDS, Verified, check_instance,
    exemplar_envelope, gate, gate_body, with_space_size,
};
pub use render::{RenderError, SNIPPET_CHARS, StrayBrace, placeholders, render, scan, stray_brace};
