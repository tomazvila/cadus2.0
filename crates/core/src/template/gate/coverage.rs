//! Rows 20 to 23 of the gate: the samples lie inside the space and exercise it.

use std::collections::{BTreeMap, BTreeSet};

use super::Rejection;
use super::space::Walk;
use super::text::{constraint_rejection, py_bindings, py_list, scalar_repr};
use crate::template::constraint::{all_hold, constraint_params, holds};
use crate::template::document::TemplateDoc;
use crate::template::domain::{Bindings, Domain, Value};

/// The two ends of one ordered axis, and the declared ends when they differ.
#[derive(Debug, Clone)]
pub(super) struct AxisEnds {
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
pub(super) fn axis_extremes(
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
pub(super) fn check_coverage(
    doc: &TemplateDoc,
    values: &BTreeMap<String, Vec<Value>>,
    extremes: &BTreeMap<String, AxisEnds>,
    samples: &[Bindings],
    walk: &Walk,
    notes: &mut Vec<String>,
) -> Result<(), Rejection> {
    check_samples_in_domain(doc, values)?;
    check_samples_hold(doc, samples)?;
    for (name, domain) in &doc.params {
        let seen: BTreeSet<Value> = samples
            .iter()
            .filter_map(|bindings| bindings.get(name).cloned())
            .collect();
        if matches!(domain, Domain::Choice { .. }) {
            check_choice_axis(name, &seen, walk)?;
            continue;
        }
        if let Some(ends) = extremes.get(name) {
            check_axis_ends(name, ends, &seen)?;
        }
    }
    check_crossed_corners(doc, extremes, samples, walk, notes)
}

/// Every sample binds each parameter to a value its own domain produces.
fn check_samples_in_domain(
    doc: &TemplateDoc,
    values: &BTreeMap<String, Vec<Value>>,
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
    Ok(())
}

/// Every constraint holds on every sample.
fn check_samples_hold(doc: &TemplateDoc, samples: &[Bindings]) -> Result<(), Rejection> {
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
    Ok(())
}

/// Every reachable choice of one axis appears in a worked sample.
///
/// The rule reads the choices of the SATISFYING set, and never the declared
/// list: a declared choice the constraints forbid asks for a worked sample the
/// sample-constraint rule then refuses, and no author input clears both (M4
/// review 2, findings 3 and 9).
fn check_choice_axis(name: &str, seen: &BTreeSet<Value>, walk: &Walk) -> Result<(), Rejection> {
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
    if missing.is_empty() {
        return Ok(());
    }
    Err(Rejection::new(
        "choice-coverage",
        format!(
            "no worked sample uses {name}={} — every choice must appear in a sample, or the expression is unverified for it",
            py_list(&missing)
        ),
    ))
}

/// A worked sample stands at each end of one ordered axis.
///
/// A sample at a declared end covers that end too (M4 review 2, finding 10).
fn check_axis_ends(name: &str, ends: &AxisEnds, seen: &BTreeSet<Value>) -> Result<(), Rejection> {
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
    Ok(())
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
    let axes: Vec<(&String, &AxisEnds)> = extremes
        .iter()
        .filter(|(_, ends)| ends.low != ends.high)
        .collect();
    for (position, (left, ends_l)) in axes.iter().enumerate() {
        for (right, ends_r) in axes.iter().skip(position + 1) {
            let pair = AxisPair {
                left,
                ends_l,
                right,
                ends_r,
            };
            check_corner_pair(doc, &pair, samples, walk, notes)?;
        }
    }
    Ok(())
}

/// Two ordered axes, with the ends of each.
struct AxisPair<'a> {
    /// The name of the first axis.
    left: &'a str,
    /// The ends of the first axis.
    ends_l: &'a AxisEnds,
    /// The name of the second axis.
    right: &'a str,
    /// The ends of the second axis.
    ends_r: &'a AxisEnds,
}

/// One pair of axes has a worked sample at one of its two crossed corners.
///
/// The rule asks for a sample at one of two corners, so it applies only when
/// the constraints admit BOTH of them. A band constraint admits neither, and
/// the rule and the sample-constraint rule then deadlock: the gate names two
/// tuples that it refuses (M4 review 1, finding 22).
fn check_corner_pair(
    doc: &TemplateDoc,
    pair: &AxisPair<'_>,
    samples: &[Bindings],
    walk: &Walk,
    notes: &mut Vec<String>,
) -> Result<(), Rejection> {
    let AxisPair {
        left,
        ends_l,
        right,
        ends_r,
    } = *pair;
    let (low_l, high_l) = (&ends_l.low, &ends_l.high);
    let (low_r, high_r) = (&ends_r.low, &ends_r.high);
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
        return Ok(());
    }
    let crossed = samples.iter().any(|bindings| {
        let (Some(bound_l), Some(bound_r)) = (bindings.get(left), bindings.get(right)) else {
            return false;
        };
        (bound_l == low_l && bound_r == high_r) || (bound_l == high_l && bound_r == low_r)
    });
    if crossed {
        return Ok(());
    }
    Err(Rejection::new(
        "crossed-corner",
        format!(
            "no worked sample crosses {left} and {right} — one of them at its low end WITH the other at its high end ({left}={} with {right}={}, or {left}={} with {right}={}). Matching corners are exactly where a swapped-operand expression looks right",
            low_l.canonical_string(),
            high_r.canonical_string(),
            high_l.canonical_string(),
            low_r.canonical_string()
        ),
    ))
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
