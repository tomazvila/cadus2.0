//! Rows 10 to 14 of the gate: the rendered fields, the answer expression, and
//! the names it reaches for.

use std::collections::{BTreeMap, BTreeSet};

use crate::answer::ast::Ast;
use crate::curriculum::AnswerKind;

use super::text::{py_list, py_str};
use super::{EXPONENT_REASON, FREE_SYMBOLS, GateSpec, MAX_EXPONENT, RESERVED_NAMES, Rejection};
use crate::template::document::{Compiled, TemplateDoc};
use crate::template::domain::Value;
use crate::template::draw::DrawPlan;
use crate::template::eval::parse_answer_expr;
use crate::template::render::scan;

/// Every rendered field names declared parameters and doubles its literal braces.
pub(super) fn check_rendered_fields(doc: &TemplateDoc) -> Result<(), Rejection> {
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
///
/// `values` holds the value list of every domain, which [`super::check_params`]
/// read already, so the plan comes from those lists.
pub(super) fn compile<'doc>(
    doc: &'doc TemplateDoc,
    values: &BTreeMap<String, Vec<Value>>,
) -> Result<Compiled<'doc>, Rejection> {
    let answer_ast = parse_answer_expr(&doc.answer_expr).map_err(|reason| {
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
    })?;
    Ok(Compiled::from_parts(
        doc,
        answer_ast,
        DrawPlan::from_columns(values.clone()),
    ))
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
pub(super) fn check_dead_parameters(doc: &TemplateDoc, ast: &Ast) -> Result<(), Rejection> {
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
pub(super) fn check_answer_names(
    doc: &TemplateDoc,
    ast: &Ast,
    spec: &GateSpec,
) -> Result<(), Rejection> {
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
