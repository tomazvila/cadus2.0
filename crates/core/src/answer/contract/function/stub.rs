//! FLOW-STUB: a placeholder for `crate::answer::evalf` (lane B4a).
//!
//! Phase 2 of lane B4b deletes this file and uses the evaluator of lane B4a.
//! The placeholder finds no value at each point. Thus each `function` key is
//! refused, and no answer gets a verdict from this file.

use std::collections::{BTreeMap, BTreeSet};

use crate::answer::Ast;

// FLOW-STUB
/// The value of each variable at one sample point.
pub type Env = BTreeMap<String, f64>;

// FLOW-STUB
pub(super) fn eval(_tree: &Ast, _env: &Env) -> Option<f64> {
    None
}

// FLOW-STUB
pub(super) fn free_vars(_tree: &Ast) -> BTreeSet<String> {
    BTreeSet::new()
}
