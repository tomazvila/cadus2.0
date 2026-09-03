//! The graph-stage rules of the lint, one function per rule block of 1.0
//! `lint_curriculum` (`cadus/graph.py:628-842`), in the 1.0 order.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::super::finding::Finding;
use super::super::graph::{self, Csr};
use super::super::model::{Catalog, Course, Topic};
use super::{Lint, Table, node, py_list_repr, py_repr};

impl Lint<'_> {
    /// Rule 8, acyclicity: report one cycle and return its nodes.
    pub(super) fn check_cycle(&mut self) -> Option<Vec<u32>> {
        let cycle = graph::find_cycle(&self.prereqs, self.table.topics.len())?;
        let ids: Vec<&str> = cycle.iter().map(|&n| self.table.id(n as usize)).collect();
        // The arrow form closes the loop with the re-entered node.
        let arrow: Vec<&str> = ids.iter().chain(ids.first()).copied().collect();
        self.findings.push(
            Finding::new(
                "cycle",
                format!("prerequisite cycle: {}", arrow.join(" -> ")),
            )
            .with_context(ids.iter().map(|id| (*id).to_owned()).collect()),
        );
        Some(cycle)
    }

    /// Rules 9 to 12: the per-topic cardinality and the key prerequisites.
    pub(super) fn check_topics(&mut self) {
        let Self {
            table,
            prereqs,
            findings,
            ..
        } = self;
        for (position, topic) in table.topics.iter().enumerate() {
            check_cardinality(topic, findings);
            check_key_prerequisites(table, prereqs, position, topic, findings);
        }
    }

    /// Rule 13, the core-ancestor invariant (PEDAGOGY 1).
    ///
    /// 1.0 scans every topic's ancestor set for `tid`. The dependent closure of
    /// `tid` is the same relation read from the other end, and it costs one
    /// walk per non-core topic instead of one per topic.
    pub(super) fn check_core_ancestors(&mut self) {
        let Self {
            table,
            dependents,
            findings,
            ..
        } = self;
        let count = table.topics.len();
        for (position, topic) in table.sorted() {
            if topic.core {
                continue;
            }
            let mut core_dependents: Vec<&str> = graph::closure(dependents, node(position), count)
                .into_iter()
                .filter(|&other| table.topics.get(other as usize).is_some_and(|t| t.core))
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
    }

    /// Rule 14, module names: no empty name, and one course per module.
    pub(super) fn check_modules(&mut self) {
        let Self {
            table, findings, ..
        } = self;
        let mut module_courses: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for position in 0..table.topics.len() {
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
    }

    /// Rule 15: exactly one mastery-floor form per course.
    pub(super) fn check_mastery_floor_forms(&mut self, catalog: &Catalog) {
        for course in &catalog.courses {
            if let (false, Some(reference)) = (
                course.mastery_floor.is_empty(),
                course.mastery_floor_course.as_ref(),
            ) {
                self.findings.push(Finding::new(
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
    }

    /// Rule 16: every topic is reachable from the floor or the roots of its
    /// course.
    pub(super) fn check_reachability(&mut self, catalog: &Catalog) {
        let Self {
            table,
            prereqs,
            dependents,
            findings,
        } = self;
        let mut topics_by_course: HashMap<&str, Vec<usize>> = HashMap::new();
        for position in 0..table.topics.len() {
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
            let grounded = grounded_of(
                course,
                catalog,
                &course_by_id,
                &topics_by_course,
                table,
                prereqs,
            );
            let reachable = reach(dependents, grounded, table.topics.len());
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
}

/// Rules 9 to 11: at least one knowledge point, at least one exemplar per
/// knowledge point, and a diagnostic exemplar.
fn check_cardinality(topic: &Topic, findings: &mut Vec<Finding>) {
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
}

/// Rule 12: every key prerequisite is an ancestor or an `encompassings_extra`
/// target. A key prerequisite with no topic was already reported as
/// `missing_ref` and is skipped here.
fn check_key_prerequisites(
    table: &Table<'_>,
    prereqs: &Csr,
    position: usize,
    topic: &Topic,
    findings: &mut Vec<Finding>,
) {
    let wants_ancestors = topic
        .knowledge_points
        .iter()
        .any(|kp| !kp.key_prerequisites.is_empty());
    if !wants_ancestors {
        return;
    }
    let tid = topic.id.as_str();
    let ancestors = graph::closure(prereqs, node(position), table.topics.len());
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
            let is_ancestor = ancestors.binary_search(&node(target)).is_ok();
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

/// The grounded positions of one course: the floor, intersected with the known
/// topics, every topic of every course ordered at or below the named floor
/// course (parity trap 12), and the roots of the course.
fn grounded_of(
    course: &Course,
    catalog: &Catalog,
    course_by_id: &HashMap<&str, &Course>,
    topics_by_course: &HashMap<&str, Vec<usize>>,
    table: &Table<'_>,
    prereqs: &Csr,
) -> Vec<usize> {
    let mut grounded: Vec<usize> = course
        .mastery_floor
        .iter()
        .filter_map(|id| table.by_id.get(id.as_str()).copied())
        .collect();
    if let Some(named) = course.mastery_floor_course.as_ref()
        && let Some(reference) = course_by_id.get(named.as_str())
    {
        for other in &catalog.courses {
            if other.order <= reference.order
                && let Some(list) = topics_by_course.get(other.id.as_str())
            {
                grounded.extend_from_slice(list);
            }
        }
    }
    let course_topics: &[usize] = topics_by_course
        .get(course.id.as_str())
        .map_or(&[], Vec::as_slice);
    for &position in course_topics {
        if prereqs.neighbors(position).is_empty() {
            grounded.push(position);
        }
    }
    grounded
}

/// One multi-source walk down the dependent edges. It marks the grounded nodes
/// too, the same as the 1.0 `set(grounded)` seed.
fn reach(dependents: &Csr, grounded: Vec<usize>, count: usize) -> Vec<bool> {
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
    reachable
}
