//! Rows v to 9 of the gate: the document itself, its parameters, and the
//! constraint shape.

use std::collections::{BTreeMap, BTreeSet};

use crate::curriculum::AnswerKind;

use super::text::{is_identifier, py_str};
use super::{FREE_SYMBOLS, GateSpec, RESERVED_NAMES, Rejection, TEMPLATABLE_KINDS};
use crate::template::constraint::{Constraint, Term, constraint_params, term_params};
use crate::template::document::{TEMPLATE_VERSION, TemplateDoc};
use crate::template::domain::{
    Domain, DomainError, MAX_CHOICES, MAX_DECIMAL_SCALE, MAX_DOMAIN_SIZE, Value,
};

/// The document version, the answer kind, and the two required strings.
pub(super) fn check_document(doc: &TemplateDoc, spec: &GateSpec) -> Result<(), Rejection> {
    if doc.v != TEMPLATE_VERSION {
        return Err(Rejection::new(
            "template-version",
            format!(
                "template version {} is not the current version {TEMPLATE_VERSION}",
                doc.v
            ),
        ));
    }
    if let Some(contract) = &doc.answer_contract {
        contract
            .validate()
            .map_err(|reason| Rejection::new("answer-contract", reason.reason.to_owned()))?;
        if contract == &crate::answer::AnswerContract::None {
            return Err(Rejection::new(
                "answer-contract",
                "a template requires a deterministic answer contract".to_owned(),
            ));
        }
    }
    let contracted_multi_step =
        spec.answer_kind == AnswerKind::MultiStep && doc.answer_contract.is_some();
    if !TEMPLATABLE_KINDS.contains(&spec.answer_kind) && !contracted_multi_step {
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
pub(super) fn check_params(
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
        let symbolic_answer = spec.answer_kind == AnswerKind::Expression
            || matches!(
                doc.answer_contract,
                Some(
                    crate::answer::AnswerContract::PolynomialRelation
                        | crate::answer::AnswerContract::RelationSetup
                )
            );
        if symbolic_answer && FREE_SYMBOLS.contains(&name.as_str()) {
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
    if let Domain::Choice { values } = domain
        && values.len() > MAX_CHOICES
    {
        return Err(Rejection::new(
            "choice-domain",
            format!(
                "a choice domain of {} exceeds MAX_CHOICES ({MAX_CHOICES}); every choice must appear in a worked sample, so use an int domain or split the template",
                values.len()
            ),
        ));
    }
    domain
        .values(name)
        .map_err(|error| domain_rejection(domain, &error))
}

/// The rejection a domain the value walk refused earns, from the error it raised.
fn domain_rejection(domain: &Domain, error: &DomainError) -> Rejection {
    match domain {
        Domain::Int { low, high } => match error {
            DomainError::EmptyRange { .. } => {
                Rejection::new("int-domain", format!("int domain {low}..{high} is empty"))
            }
            _ => Rejection::new(
                "domain-size",
                format!("int domain {low}..{high} exceeds MAX_DOMAIN_SIZE"),
            ),
        },
        // The walk refuses a choice list for one reason only: it is empty. A
        // list past MAX_CHOICES never reaches the walk.
        Domain::Choice { .. } => Rejection::new(
            "choice-domain",
            "a choice domain needs a non-empty 'values' list".to_string(),
        ),
        Domain::Rational { num, den } => match error {
            DomainError::ZeroDenominator { .. } => Rejection::new(
                "rational-domain",
                format!(
                    "the denominator range {}..{} of a rational domain holds zero",
                    den.low, den.high
                ),
            ),
            DomainError::EmptyRange { .. } => Rejection::new(
                "rational-domain",
                format!(
                    "rational domain {}..{} over {}..{} is empty",
                    num.low, num.high, den.low, den.high
                ),
            ),
            _ => Rejection::new(
                "domain-size",
                format!(
                    "rational domain {}..{} over {}..{} exceeds MAX_DOMAIN_SIZE ({MAX_DOMAIN_SIZE})",
                    num.low, num.high, den.low, den.high
                ),
            ),
        },
        Domain::Decimal { low, high, scale } => match error {
            DomainError::DecimalScale { .. } => Rejection::new(
                "decimal-domain",
                format!(
                    "decimal domain scale {scale} exceeds MAX_DECIMAL_SCALE ({MAX_DECIMAL_SCALE})"
                ),
            ),
            DomainError::EmptyRange { .. } => Rejection::new(
                "decimal-domain",
                format!("decimal domain {low}..{high} at scale {scale} is empty"),
            ),
            _ => Rejection::new(
                "domain-size",
                format!("decimal domain {low}..{high} exceeds MAX_DOMAIN_SIZE ({MAX_DOMAIN_SIZE})"),
            ),
        },
    }
}

// --------------------------------------------------------------------------
// 2.0: the constraint language reads declared, whole-number parameters
// --------------------------------------------------------------------------

/// Every constraint names declared parameters, and whole-number ones where it must.
pub(super) fn check_constraint_shape(
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
