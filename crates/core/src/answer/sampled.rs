//! Rung 5b of the answer check: a sampled upgrade for plain expressions (D6-expr).
//!
//! The exact canonical form has no rule for a trigonometric identity, a
//! polynomial GCD or a radical law. A pair such as `sec(x)^2` and `1/cos(x)^2`
//! reaches rung 5b as a decided miss. This rung samples the pair with the
//! `function` contract of D6-fn. It only upgrades. If the sampler agrees, the
//! verdict is `correct`. If the sampler disagrees or refuses, the decided miss
//! stands. The rung never returns an undecidable outcome.

use std::collections::BTreeMap;

use crate::curriculum::AnswerKind;

use super::ast::Ast;
use super::canon::Canon;
use super::check::Outcome;
use super::contract::function::{self, FunctionSpec};
use super::evalf::{any_node, free_vars};

/// The most variables that a `function` contract reads.
const MAX_VARS: usize = 4;

/// Whether the canonical form is a plain value, not a label or a collection.
fn is_plain(value: &Canon) -> bool {
    matches!(
        value,
        Canon::Rational(_)
            | Canon::Radical(_)
            | Canon::Poly(_)
            | Canon::Value { .. }
            | Canon::Func(..)
    )
}

/// Return `Some` correct outcome when the sampler proves the pair equal.
///
/// `None` means the rung does not apply, or the sampler does not agree. The
/// caller then keeps its decided miss.
pub(super) fn rung(
    expected: &str,
    learner: &str,
    kind: AnswerKind,
    trees: (&Ast, &Ast),
    values: (&Canon, &Canon),
) -> Option<Outcome> {
    let (expected_tree, learner_tree) = trees;
    if kind != AnswerKind::Expression || !is_plain(values.0) || !is_plain(values.1) {
        return None;
    }
    let mut names = free_vars(expected_tree);
    names.extend(free_vars(learner_tree));
    if names.is_empty() || names.len() > MAX_VARS {
        return None;
    }
    // A decimal literal stays exact (D6-dec): a rounded coefficient is not equal.
    let has_decimal = |tree: &Ast| any_node(tree, |node| matches!(node, Ast::Decimal { .. }));
    if has_decimal(expected_tree) || has_decimal(learner_tree) {
        return None;
    }
    let names: Vec<String> = names.into_iter().collect();
    let spec = FunctionSpec::new(&names, false, &BTreeMap::new()).ok()?;
    function::expected(&spec, expected).ok()?;
    match function::check(&spec, expected, learner) {
        Outcome::Decided(verdict) if verdict.correct => Some(Outcome::decided(true)),
        _ => None,
    }
}
