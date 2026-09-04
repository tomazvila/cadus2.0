//! The helpers of the `template_domain_edges` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

pub use cadus_core::curriculum::{AnswerKind, Exemplar};
pub use cadus_core::template::{
    Cmp, Compiled, Constraint, DiagnosisDoc, Distractor, Domain, DomainError, DrawError, DrawPlan,
    GateSpec, InstantiateError, IntRange, Params, RenderError, Scalar, StrayBrace, Term, Value,
    below, candidates, declared_space, draw_bindings, draw_satisfying, enumerate, from_body,
    gate_diagnosis, gate_diagnosis_body, keep_known_tags, literal_to_rational, placeholders,
    render, rng_from_seed, space_size, stray_brace, walk_satisfying,
};
pub use num_bigint::BigInt;
pub use num_rational::BigRational;
pub use serde_json::json;

/// An int domain.
pub fn int(low: i64, high: i64) -> Domain {
    Domain::Int { low, high }
}

/// A parameter set.
pub fn params(entries: &[(&str, Domain)]) -> Params {
    entries
        .iter()
        .map(|(name, domain)| ((*name).to_string(), domain.clone()))
        .collect()
}

/// The constraint that reads the text choice `op` as a number.
pub fn text_as_number() -> Constraint {
    Constraint {
        op: Cmp::Eq,
        left: Term::Param("op".to_string()),
        right: Term::Lit(BigRational::from_integer(BigInt::from(1))),
    }
}

/// The choice of two operator texts.
pub fn operators() -> Domain {
    Domain::Choice {
        values: vec![Scalar::Text("+".to_string()), Scalar::Text("-".to_string())],
    }
}

/// The error a text choice under a numeric constraint raises.
pub fn not_numeric() -> DomainError {
    DomainError::Constraint(cadus_core::template::ConstraintError::NotNumeric {
        name: "op".to_string(),
        text: "+".to_string(),
    })
}

/// The whole number `value`.
pub fn num(value: i64) -> Value {
    Value::Num(BigRational::from_integer(BigInt::from(value)))
}
