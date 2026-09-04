//! The helpers of the `template` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

pub use cadus_core::learner::problem_text_hash;
pub use cadus_core::template::{Bindings, eval::EvalError, render::RenderError};
pub use cadus_core::template::{
    Cmp, Compiled, Constraint, Domain, DrawPlan, EXHAUSTIVE_SPACE_LIMIT, Instance, MAX_CHOICES,
    MAX_DOMAIN_SIZE, MIN_SPACE_SIZE, RESAMPLE_ATTEMPTS, Scalar, SpaceSize, TEMPLATE_VERSION,
    TemplateDoc, Term, Value, below, from_body, holds, render, rng_from_seed, space_size,
    stray_brace, to_body, walk_satisfying,
};
pub use std::collections::BTreeSet;

// --------------------------------------------------------------------------
// Fixtures
// --------------------------------------------------------------------------
/// The 1.0 perfect-squares template, in the 2.0 document shape.
///
/// 1.0 stores `{"v": 2, "text": "Compute ${a}^{{2}}$.", "answer_expr": "a**2",
/// "params": {"a": {"kind": "int", "low": 1, "high": 12}}, "space_size": 12}`
/// (`docs/reference/serving-1.0-spec.md`, section 2.1, the printed round trip).
pub fn perfect_squares_body() -> String {
    super::gate::body_with(&[])
}

/// The squares-of-negatives template: `a**2` over -12..-1 (M4 review 1, finding 18).
pub fn squares_of_negatives_body() -> String {
    super::gate::body_with(&[
        ("topic_id", r#""squares-of-negatives""#),
        (
            "params",
            r#"{"a": {"kind": "int", "low": -12, "high": -1}}"#,
        ),
        ("solution_sketch", ""),
        ("hints", r#"["What sign does a square carry?"]"#),
        ("samples", r#"[{"params": {"a": -1}, "expected": "1"}]"#),
    ])
}

pub fn doc_from(body: &str) -> TemplateDoc {
    from_body(body).expect("the fixture body reads")
}

pub fn bind(pairs: &[(&str, i64)]) -> Bindings {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), Value::Num(num_rational_from(*value))))
        .collect()
}

pub fn num_rational_from(value: i64) -> num_rational::BigRational {
    num_rational::BigRational::from(num_bigint::BigInt::from(value))
}

pub fn text_binding(name: &str, text: &str) -> (String, Value) {
    (name.to_string(), Value::Text(text.to_string()))
}

/// One evaluation case: the source, the tuple it binds, and the answer.
pub type EvalCase = (&'static str, &'static [(&'static str, i64)], &'static str);

/// The whole numbers of a bound tuple, as `i64`, in name order.
pub fn whole_values(bindings: &Bindings) -> Vec<i64> {
    bindings
        .values()
        .map(|value| {
            value
                .as_integer()
                .and_then(|number| i64::try_from(number).ok())
                .expect("the tuple binds whole numbers")
        })
        .collect()
}
