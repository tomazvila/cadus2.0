//! The curriculum lint (C5, spec section 5).
//!
//! [`lint_curriculum`] is the port of 1.0 `lint_curriculum`
//! (`cadus/graph.py:628-842`). It reports every rule of DATA_MODEL section 1 as a
//! [`Finding`], with the 1.0 code, the 1.0 message text, the 1.0 `fatal` flag,
//! and the 1.0 `context` payload.
//!
//! ## Two stages
//!
//! The parse stage comes from [`super::load::parse_curriculum`]: `yaml`,
//! `schema`, `weight_out_of_range`, `missing_course_dir` and `empty_course`. The
//! graph stage is here. A load tolerates every graph-stage code (parity trap 13);
//! the lint reports all of them, and the runner fails on any finding at all.
//!
//! ## No arena
//!
//! The lint builds its own topic table and its own adjacency instead of calling
//! [`super::arena::Curriculum::build`], because the arena refuses a duplicate
//! topic id (parity trap 14) while the lint must tolerate one, keep the first
//! definition (spec section 2), and go on reporting.
//!
//! ## Order
//!
//! Every rule keeps the 1.0 iteration order: load order where 1.0 walks its
//! topic dictionary, ascending id order where 1.0 calls `sorted()`, catalog file
//! order where 1.0 walks `catalog.courses`.

mod rules;

use std::collections::HashMap;
use std::path::Path;

use super::finding::Finding;
use super::graph::{self, Csr};
use super::load::{ParsedUnit, parse_curriculum};
use super::model::Topic;

/// Lint a curriculum tree and return every violation (spec section 5).
///
/// The function never panics on content. A tree with no `courses.yaml` yields
/// the single `empty` finding that 1.0 `Graph.load` reports for the same tree
/// (`cadus/graph.py:206`); 1.0 `lint_curriculum` raises `CurriculumNotFound`
/// there, which this signature has no room for.
///
/// An empty result means the curriculum is clean.
pub fn lint_curriculum(root: &Path) -> Vec<Finding> {
    let parsed = parse_curriculum(root);
    if parsed.error.is_some() {
        return vec![Finding::new("empty", "no curriculum found")];
    }
    let Some(catalog) = parsed.catalog else {
        return parsed.findings;
    };
    let mut lint = Lint::new(&parsed.units, parsed.findings);
    let cycle = lint.check_cycle();
    lint.check_topics();
    lint.check_answer_contracts();
    lint.check_core_ancestors();
    lint.check_modules();
    lint.check_mastery_floor_forms(&catalog);
    // A cycle or a duplicate id makes the derived graph ill-defined, so the
    // reachability rule is skipped entirely then (spec section 5, rule 16).
    if cycle.is_none() && !lint.has_duplicate_id() {
        lint.check_reachability(&catalog);
    }
    lint.findings
}

/// The state of one lint run: the topic table, the two prerequisite
/// directions, and the findings so far.
struct Lint<'a> {
    table: Table<'a>,
    /// Child -> parents, existing targets only, ascending and distinct.
    prereqs: Csr,
    /// Parent -> children, the transpose of `prereqs`.
    dependents: Csr,
    findings: Vec<Finding>,
}

impl<'a> Lint<'a> {
    /// Every explicit policy must accept a supported authored expected value.
    fn check_answer_contracts(&mut self) {
        for topic in &self.table.topics {
            let exemplars = topic
                .knowledge_points
                .iter()
                .flat_map(|kp| &kp.exemplars)
                .chain(topic.diagnostic_exemplar.iter());
            for exemplar in exemplars {
                if let Some(contract) = exemplar.answer_contract {
                    if contract == crate::answer::AnswerContract::None {
                        continue;
                    }
                    if let Err(reason) = exemplar.canonical_answer() {
                        self.findings.push(Finding::new(
                            "answer_contract",
                            format!("{}: {}: {}", topic.id, exemplar.problem, reason.reason),
                        ));
                    }
                }
            }
        }
    }

    /// Build the table and the adjacency, and report the duplicate ids and the
    /// missing references on the way.
    fn new(units: &'a [ParsedUnit], mut findings: Vec<Finding>) -> Self {
        let table = Table::from_units(units, &mut findings);
        let prereq_lists = references(&table, &mut findings);
        let count = table.topics.len();
        Self {
            prereqs: Csr::from_lists(&prereq_lists),
            dependents: Csr::from_lists(&graph::transpose(&prereq_lists, count)),
            table,
            findings,
        }
    }

    /// Whether the run reported a `duplicate_topic_id` finding.
    fn has_duplicate_id(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.code == "duplicate_topic_id")
    }
}

/// The node number of a table position.
fn node(position: usize) -> u32 {
    u32::try_from(position).unwrap_or(u32::MAX)
}

/// The prerequisite nodes of every topic, ascending and distinct, with a
/// `missing_ref` finding for every reference that names no topic.
///
/// 1.0 keeps the adjacency in a `set`, so a repeated edge counts once. The sort
/// makes the walk order the load order instead of the hash order (parity trap
/// 10).
fn references(table: &Table<'_>, findings: &mut Vec<Finding>) -> Vec<Vec<u32>> {
    let mut lists: Vec<Vec<u32>> = Vec::with_capacity(table.topics.len());
    for topic in &table.topics {
        let tid = topic.id.as_str();
        let mut parents: Vec<u32> = Vec::new();
        for edge in &topic.prerequisites {
            match table.by_id.get(edge.id.as_str()) {
                Some(&parent) => parents.push(node(parent)),
                None => findings.push(missing_ref("prerequisite", edge.id.as_str(), tid)),
            }
        }
        for edge in &topic.encompassings_extra {
            if !table.by_id.contains_key(edge.id.as_str()) {
                findings.push(missing_ref("encompassings_extra", edge.id.as_str(), tid));
            }
        }
        for kp in &topic.knowledge_points {
            for key_id in &kp.key_prerequisites {
                if !table.by_id.contains_key(key_id.as_str()) {
                    findings.push(
                        Finding::new(
                            "missing_ref",
                            format!(
                                "key_prerequisite {} in {tid}.{} does not exist",
                                py_repr(key_id.as_str()),
                                kp.id
                            ),
                        )
                        .with_topic(tid),
                    );
                }
            }
        }
        parents.sort_unstable();
        parents.dedup();
        lists.push(parents);
    }
    lists
}

/// The `missing_ref` finding of an edge field: `<field> <id> of <topic> does
/// not exist`.
fn missing_ref(field: &str, id: &str, tid: &str) -> Finding {
    Finding::new(
        "missing_ref",
        format!("{field} {} of {} does not exist", py_repr(id), py_repr(tid)),
    )
    .with_topic(tid)
}

/// The topics of a tree in load order, with the denormalized course and module.
///
/// The table keeps the FIRST definition of a repeated topic id (spec section 2),
/// which is what 1.0 lint does before it goes on with the rest of the rules.
#[derive(Default)]
struct Table<'a> {
    topics: Vec<&'a Topic>,
    courses: Vec<&'a str>,
    modules: Vec<&'a str>,
    by_id: HashMap<&'a str, usize>,
}

impl<'a> Table<'a> {
    /// The topics of every unit in load order. A repeated id keeps the first
    /// definition and reports `duplicate_topic_id`.
    fn from_units(units: &'a [ParsedUnit], findings: &mut Vec<Finding>) -> Self {
        let mut table = Self::default();
        for parsed_unit in units {
            let unit = &parsed_unit.unit;
            for topic in &unit.topics {
                let id = topic.id.as_str();
                if table.by_id.contains_key(id) {
                    findings.push(
                        Finding::new(
                            "duplicate_topic_id",
                            format!("topic id {} defined more than once", py_repr(id)),
                        )
                        .with_topic(id),
                    );
                    continue;
                }
                table.by_id.insert(id, table.topics.len());
                table.topics.push(topic);
                // The course of a topic is the authored `course` field, not the
                // directory of the file (`cadus/graph.py:272`).
                table.courses.push(unit.course.as_str());
                table.modules.push(unit.module.as_str());
            }
        }
        table
    }

    /// The id of one topic, or `""` when the position is out of range.
    fn id(&self, position: usize) -> &'a str {
        self.topics
            .get(position)
            .map_or("", |topic| topic.id.as_str())
    }

    /// The course of one topic, or `""` when the position is out of range.
    fn course(&self, position: usize) -> &'a str {
        self.courses.get(position).copied().unwrap_or("")
    }

    /// The module of one topic, or `""` when the position is out of range.
    fn module(&self, position: usize) -> &'a str {
        self.modules.get(position).copied().unwrap_or("")
    }

    /// Every topic with its position, ordered by topic id. This is the 1.0
    /// `sorted(topics)` walk; the ids are unique here, so the order is total.
    fn sorted(&self) -> Vec<(usize, &'a Topic)> {
        let mut order: Vec<(usize, &'a Topic)> = self.topics.iter().copied().enumerate().collect();
        order.sort_by(|left, right| left.1.id.cmp(&right.1.id));
        order
    }
}

/// The Python `repr` of a string: single quotes unless the text holds one.
///
/// 1.0 writes every id into its messages with `{x!r}` (spec section 5), so the
/// port needs the same quoting. Curriculum ids are printable text, so the escape
/// table below covers the backslash, both quotes, and the C0 controls.
fn py_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if other == quote => {
                out.push('\\');
                out.push(other);
            }
            other if (other as u32) < 0x20 || (other as u32) == 0x7f => {
                out.push_str(&format!("\\x{:02x}", other as u32));
            }
            other => out.push(other),
        }
    }
    out.push(quote);
    out
}

/// The Python `repr` of a list of strings: `['a', 'b']`.
fn py_list_repr(items: &[&str]) -> String {
    let parts: Vec<String> = items.iter().map(|item| py_repr(item)).collect();
    format!("[{}]", parts.join(", "))
}
#[cfg(test)]
mod tests {
    use super::{py_list_repr, py_repr};

    /// The quote switches to a double quote when the text holds a single quote
    /// and no double quote, the same as Python.
    #[test]
    fn the_python_repr_switches_the_quote_and_escapes_the_rest() {
        assert_eq!(py_repr("plain"), "'plain'");
        assert_eq!(py_repr("it's"), "\"it's\"");
        assert_eq!(py_repr("both ' and \""), "'both \\' and \"'");
        assert_eq!(py_repr("a\\b\n\r\t"), "'a\\\\b\\n\\r\\t'");
        assert_eq!(py_repr("\u{1}\u{7f}"), "'\\x01\\x7f'");
        assert_eq!(py_list_repr(&["a", "b"]), "['a', 'b']");
    }
}
