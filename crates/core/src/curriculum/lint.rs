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

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use super::finding::Finding;
use super::graph::{self, Csr};
use super::load::{Parsed, parse_curriculum};
use super::model::{Course, Topic};

/// Lint a curriculum tree and return every violation (spec section 5).
///
/// The function never panics on content. A tree with no `courses.yaml` yields
/// the single `empty` finding that 1.0 `Graph.load` reports for the same tree
/// (`cadus/graph.py:206`); 1.0 `lint_curriculum` raises `CurriculumNotFound`
/// there, which this signature has no room for.
///
/// An empty result means the curriculum is clean.
pub fn lint_curriculum(root: &Path) -> Vec<Finding> {
    let Parsed {
        catalog,
        units,
        mut findings,
        error,
    } = parse_curriculum(root);

    if error.is_some() {
        return vec![Finding::new("empty", "no curriculum found")];
    }
    let Some(catalog) = catalog else {
        return findings;
    };

    // -- topic table, in load order, first definition wins ----------------- //

    let mut table = Table::default();
    for parsed_unit in &units {
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
    let count = table.topics.len();

    // -- referenced ids exist ---------------------------------------------- //

    let mut prereq_lists: Vec<Vec<u32>> = vec![Vec::new(); count];
    for (position, topic) in table.topics.iter().enumerate() {
        let tid = topic.id.as_str();
        for edge in &topic.prerequisites {
            match table.by_id.get(edge.id.as_str()) {
                None => findings.push(
                    Finding::new(
                        "missing_ref",
                        format!(
                            "prerequisite {} of {} does not exist",
                            py_repr(edge.id.as_str()),
                            py_repr(tid)
                        ),
                    )
                    .with_topic(tid),
                ),
                Some(&parent) => {
                    if let (Some(list), Ok(parent)) =
                        (prereq_lists.get_mut(position), u32::try_from(parent))
                    {
                        list.push(parent);
                    }
                }
            }
        }
        for edge in &topic.encompassings_extra {
            if !table.by_id.contains_key(edge.id.as_str()) {
                findings.push(
                    Finding::new(
                        "missing_ref",
                        format!(
                            "encompassings_extra {} of {} does not exist",
                            py_repr(edge.id.as_str()),
                            py_repr(tid)
                        ),
                    )
                    .with_topic(tid),
                );
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
    }

    // 1.0 keeps the adjacency in a `set`, so a repeated edge counts once. The
    // sort makes the walk order the load order instead of the hash order
    // (parity trap 10).
    for list in &mut prereq_lists {
        list.sort_unstable();
        list.dedup();
    }
    let prereqs = Csr::from_lists(&prereq_lists);

    let mut dependent_lists: Vec<Vec<u32>> = vec![Vec::new(); count];
    for (position, list) in prereq_lists.iter().enumerate() {
        let Ok(child) = u32::try_from(position) else {
            continue;
        };
        for &parent in list {
            if let Some(slot) = dependent_lists.get_mut(parent as usize) {
                slot.push(child);
            }
        }
    }
    let dependents = Csr::from_lists(&dependent_lists);

    // -- acyclicity --------------------------------------------------------- //

    let cycle = graph::find_cycle(&prereqs, count);
    if let Some(nodes) = cycle.as_ref() {
        let ids: Vec<&str> = nodes.iter().map(|&n| table.id(n as usize)).collect();
        let mut arrow: Vec<&str> = ids.clone();
        if let Some(&first) = ids.first() {
            arrow.push(first);
        }
        findings.push(
            Finding::new(
                "cycle",
                format!("prerequisite cycle: {}", arrow.join(" -> ")),
            )
            .with_context(ids.iter().map(|id| (*id).to_owned()).collect()),
        );
    }

    // -- per-topic cardinality and key prerequisites ------------------------ //

    for (position, topic) in table.topics.iter().enumerate() {
        let tid = topic.id.as_str();
        if topic.knowledge_points.is_empty() {
            findings.push(
                Finding::new(
                    "no_kp",
                    format!("topic {} has no knowledge_points", py_repr(tid)),
                )
                .with_topic(tid),
            );
        }
        for kp in &topic.knowledge_points {
            if kp.exemplars.is_empty() {
                findings.push(
                    Finding::new(
                        "no_exemplar",
                        format!("KP {tid}.{} has no exemplars", kp.id),
                    )
                    .with_topic(tid),
                );
            }
        }
        if topic.diagnostic_exemplar.is_none() {
            findings.push(
                Finding::new(
                    "missing_diagnostic_exemplar",
                    format!("topic {} has no diagnostic_exemplar", py_repr(tid)),
                )
                .with_topic(tid),
            );
        }

        // Every key prerequisite is an ancestor or an `encompassings_extra`
        // target. A key prerequisite with no topic was already reported as
        // `missing_ref` and is skipped here (spec section 5, rule 12).
        let wants_ancestors = topic
            .knowledge_points
            .iter()
            .any(|kp| !kp.key_prerequisites.is_empty());
        if !wants_ancestors {
            continue;
        }
        let Ok(node) = u32::try_from(position) else {
            continue;
        };
        let ancestors = graph::closure(&prereqs, node, count);
        let extra_ids: BTreeSet<&str> = topic
            .encompassings_extra
            .iter()
            .map(|edge| edge.id.as_str())
            .collect();
        for kp in &topic.knowledge_points {
            for key_id in &kp.key_prerequisites {
                let key_id = key_id.as_str();
                let Some(&target) = table.by_id.get(key_id) else {
                    continue;
                };
                let is_ancestor = u32::try_from(target)
                    .is_ok_and(|target| ancestors.binary_search(&target).is_ok());
                if !is_ancestor && !extra_ids.contains(key_id) {
                    findings.push(
                        Finding::new(
                            "key_prereq_not_ancestor",
                            format!(
                                "key_prerequisite {} in {tid}.{} is neither an ancestor nor an \
                                 encompassings_extra target",
                                py_repr(key_id),
                                kp.id
                            ),
                        )
                        .with_topic(tid),
                    );
                }
            }
        }
    }

    // -- the core-ancestor invariant (PEDAGOGY 1) --------------------------- //
    //
    // 1.0 scans every topic's ancestor set for `tid`. The dependent closure of
    // `tid` is the same relation read from the other end, and it costs one walk
    // per non-core topic instead of one per topic.
    for &position in table.sorted_positions().iter() {
        let Some(topic) = table.topics.get(position) else {
            continue;
        };
        if topic.core {
            continue;
        }
        let Ok(node) = u32::try_from(position) else {
            continue;
        };
        let mut core_dependents: Vec<&str> = graph::closure(&dependents, node, count)
            .into_iter()
            .filter(|&other| {
                table
                    .topics
                    .get(other as usize)
                    .is_some_and(|topic| topic.core)
            })
            .map(|other| table.id(other as usize))
            .collect();
        core_dependents.sort_unstable();
        let Some(&example) = core_dependents.first() else {
            continue;
        };
        let extra = if core_dependents.len() > 1 {
            format!(" (+{} more)", core_dependents.len() - 1)
        } else {
            String::new()
        };
        findings.push(
            Finding::new(
                "noncore_ancestor_of_core",
                format!(
                    "non-core topic {} is a prerequisite (ancestor) of core topic {}{extra}",
                    py_repr(topic.id.as_str()),
                    py_repr(example)
                ),
            )
            .with_topic(topic.id.as_str())
            .with_context(core_dependents.iter().map(|id| (*id).to_owned()).collect()),
        );
    }

    // -- module names ------------------------------------------------------- //

    let mut module_courses: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for position in 0..count {
        let module = table.module(position);
        if module.trim().is_empty() {
            findings.push(
                Finding::new(
                    "module_inconsistent",
                    format!(
                        "topic {} has an empty module name",
                        py_repr(table.id(position))
                    ),
                )
                .with_topic(table.id(position)),
            );
        }
        module_courses
            .entry(module)
            .or_default()
            .insert(table.course(position));
    }
    for (module, courses) in &module_courses {
        if courses.len() > 1 {
            let names: Vec<&str> = courses.iter().copied().collect();
            findings.push(Finding::new(
                "module_inconsistent",
                format!(
                    "module {} spans multiple courses: {}",
                    py_repr(module),
                    py_list_repr(&names)
                ),
            ));
        }
    }

    // -- exactly one mastery-floor form per course -------------------------- //

    for course in &catalog.courses {
        if let (false, Some(reference)) = (
            course.mastery_floor.is_empty(),
            course.mastery_floor_course.as_ref(),
        ) {
            findings.push(Finding::new(
                "mastery_floor_ambiguous",
                format!(
                    "course {} sets both a mastery_floor list and mastery_floor_course {}; \
                     a course must use exactly one mastery-floor form",
                    py_repr(course.id.as_str()),
                    py_repr(reference.as_str())
                ),
            ));
        }
    }

    // -- every topic reachable from its course's floor or roots ------------- //
    //
    // A cycle or a duplicate id makes the derived graph ill-defined, so the rule
    // is skipped entirely then (spec section 5, rule 16).
    let duplicates = findings
        .iter()
        .any(|finding| finding.code == "duplicate_topic_id");
    if cycle.is_none() && !duplicates {
        let mut topics_by_course: HashMap<&str, Vec<usize>> = HashMap::new();
        for position in 0..count {
            topics_by_course
                .entry(table.course(position))
                .or_default()
                .push(position);
        }
        // 1.0 writes `{c.id: c for c in catalog.courses}`, so a repeated course
        // id keeps the LAST entry.
        let mut course_by_id: HashMap<&str, &Course> = HashMap::new();
        for course in &catalog.courses {
            course_by_id.insert(course.id.as_str(), course);
        }

        for course in &catalog.courses {
            let course_topics: &[usize] = topics_by_course
                .get(course.id.as_str())
                .map_or(&[], Vec::as_slice);

            // The floor, intersected with the known topics, plus the roots of
            // the course.
            let mut grounded: Vec<usize> = course
                .mastery_floor
                .iter()
                .filter_map(|id| table.by_id.get(id.as_str()).copied())
                .collect();
            if let Some(named) = course.mastery_floor_course.as_ref()
                && let Some(reference) = course_by_id.get(named.as_str())
            {
                // Parity trap 12: every course ordered at or below the named one.
                for other in &catalog.courses {
                    if other.order <= reference.order
                        && let Some(list) = topics_by_course.get(other.id.as_str())
                    {
                        grounded.extend_from_slice(list);
                    }
                }
            }
            for &position in course_topics {
                if prereqs.neighbors(position).is_empty() {
                    grounded.push(position);
                }
            }

            // One multi-source walk down the dependent edges. It marks the
            // grounded nodes too, the same as the 1.0 `set(grounded)` seed.
            let mut reachable = vec![false; count];
            let mut stack: Vec<usize> = Vec::new();
            for position in grounded {
                if let Some(slot) = reachable.get_mut(position)
                    && !*slot
                {
                    *slot = true;
                    stack.push(position);
                }
            }
            while let Some(position) = stack.pop() {
                for &next in dependents.neighbors(position) {
                    if let Some(slot) = reachable.get_mut(next as usize)
                        && !*slot
                    {
                        *slot = true;
                        stack.push(next as usize);
                    }
                }
            }

            let mut unreachable: Vec<&str> = course_topics
                .iter()
                .filter(|&&position| !reachable.get(position).copied().unwrap_or(true))
                .map(|&position| table.id(position))
                .collect();
            unreachable.sort_unstable();
            for tid in unreachable {
                findings.push(
                    Finding::new(
                        "unreachable_from_floor",
                        format!(
                            "topic {} is not reachable from course {}'s floor/roots",
                            py_repr(tid),
                            course.id
                        ),
                    )
                    .with_topic(tid),
                );
            }
        }
    }

    findings
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

    /// Every position, ordered by topic id. This is the 1.0 `sorted(topics)`
    /// walk; the ids are unique here, so the order is total.
    fn sorted_positions(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.topics.len()).collect();
        order.sort_by(|left, right| self.id(*left).cmp(self.id(*right)));
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
