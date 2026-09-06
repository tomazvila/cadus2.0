//! The Markdown of the prerequisite and diagnostic coverage audit (unit f10).
//!
//! The report answers three questions per course, and it answers them with
//! counts an author acts on: which prerequisite edges lead nowhere, which topics
//! the placement cannot ask about, and which assumed-mastery topics carry no
//! evidence.
//!
//! The renderer states the shortage it finds and grants no exemption. A topic
//! the audit calls uncovered stays uncovered until an author writes the item.

use std::fmt::Write as _;

use cadus_core::readiness::{DiagnosticState, PrereqCoverage, TopicCoverage};

/// The largest number of example rows one list prints.
///
/// A report that prints 800 rows is a report nobody reads. The counts above each
/// list carry the whole number, and the list names the first rows in load order.
const LIST_LIMIT: usize = 25;

/// Render the coverage as Markdown.
#[must_use]
pub fn render_prereq_markdown(coverage: &PrereqCoverage, date: &str) -> String {
    let mut out = format!(
        "# Prerequisite and diagnostic coverage — {date}\n\n\
         The audit reads the curriculum alone: it asks whether a prerequisite edge points at \
         a topic a learner can practice, whether the placement can ask about a topic and grade \
         the answer, and what evidence stands behind each topic a course seeds as mastered.\n\n\
         The counts come from `cadus_core::readiness::PrereqCoverage`. The audit grants no \
         readiness: a topic with no diagnostic item stays a topic with no diagnostic item.\n\n"
    );
    out.push_str(&summary_table(coverage));
    for course in coverage.courses() {
        out.push_str(&course_section(coverage, &course));
    }
    out
}

/// The one table that names every course.
fn summary_table(coverage: &PrereqCoverage) -> String {
    let mut out = String::from(
        "## Every course\n\n\
         | course | topics | practicable | diagnostic decidable | undecidable | missing | \
         prerequisite edges | dangling | to unpracticable | assumed mastery | assumed with no \
         full evidence |\n|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for course in coverage.courses() {
        let counts = coverage.counts(&course);
        let _ = writeln!(
            out,
            "| {course} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            counts.topics,
            counts.practicable_topics,
            counts.diagnostic_decidable,
            counts.diagnostic_undecidable,
            counts.diagnostic_missing,
            counts.prerequisites,
            counts.dangling,
            counts.unpracticable_edges,
            counts.assumed,
            counts.assumed_without_evidence,
        );
    }
    out.push('\n');
    out
}

/// The section of one course.
fn course_section(coverage: &PrereqCoverage, course: &str) -> String {
    let rows = coverage.course(course);
    let counts = coverage.counts(course);
    let mut out = format!("## {course}\n\n");
    let _ = writeln!(
        out,
        "{} topics. {} hold at least one practicable knowledge point. {} authored prerequisite \
         edges, of which {} name a topic the tree does not hold and {} point at a topic with no \
         practice.\n",
        counts.topics,
        counts.practicable_topics,
        counts.prerequisites,
        counts.dangling,
        counts.unpracticable_edges
    );

    out.push_str("### Diagnostic coverage\n\n");
    let _ = writeln!(
        out,
        "Decidable {}, undecidable {}, missing {}. A topic with no decidable item never enters \
         the probe set, so the placement infers its state and never measures it.\n",
        counts.diagnostic_decidable, counts.diagnostic_undecidable, counts.diagnostic_missing
    );
    out.push_str(&topic_list(
        "Topics with no diagnostic item",
        &rows,
        |row| row.diagnostic == DiagnosticState::Missing,
    ));
    out.push_str(&topic_list(
        "Topics whose diagnostic answer the grammar refuses",
        &rows,
        |row| row.diagnostic == DiagnosticState::Undecidable,
    ));

    out.push_str("### Prerequisite edges that lead nowhere\n\n");
    out.push_str(&edge_list(&rows));

    out.push_str("### Assumed mastery\n\n");
    out.push_str(&floor_table(&rows, counts.assumed));
    out
}

/// One list of topic ids, with a heading and a limit.
fn topic_list(
    heading: &str,
    rows: &[&TopicCoverage],
    keep: impl Fn(&TopicCoverage) -> bool,
) -> String {
    let named: Vec<&str> = rows
        .iter()
        .filter(|row| keep(row))
        .map(|row| row.topic_id.as_str())
        .collect();
    let mut out = format!("**{heading}: {}.**\n\n", named.len());
    if named.is_empty() {
        out.push_str("None.\n\n");
        return out;
    }
    for id in named.iter().take(LIST_LIMIT) {
        let _ = writeln!(out, "- `{id}`");
    }
    if named.len() > LIST_LIMIT {
        let _ = writeln!(out, "- … and {} more.", named.len() - LIST_LIMIT);
    }
    out.push('\n');
    out
}

/// The edges whose target is absent or has no practice.
fn edge_list(rows: &[&TopicCoverage]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for row in rows {
        for id in &row.dangling {
            lines.push(format!(
                "- `{}` → `{id}` (the tree holds no such topic)",
                row.topic_id
            ));
        }
        for id in &row.unpracticable {
            lines.push(format!(
                "- `{}` → `{id}` (the target has no practicable knowledge point)",
                row.topic_id
            ));
        }
    }
    let mut out = format!("**Edges that lead nowhere: {}.**\n\n", lines.len());
    if lines.is_empty() {
        out.push_str("None.\n\n");
        return out;
    }
    for line in lines.iter().take(LIST_LIMIT) {
        out.push_str(line);
        out.push('\n');
    }
    if lines.len() > LIST_LIMIT {
        let _ = writeln!(out, "- … and {} more.", lines.len() - LIST_LIMIT);
    }
    out.push('\n');
    out
}

/// One row per assumed-mastery topic, with its evidence.
fn floor_table(rows: &[&TopicCoverage], assumed: usize) -> String {
    let mut out = format!(
        "A course seeds {assumed} topics of this course as mastered, through `mastery_floor` or \
         `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable \
         diagnostic item to CONFIRM the assumption, and a practicable knowledge point to \
         REMEDIATE a failed confirmation.\n\n"
    );
    let seeded: Vec<&&TopicCoverage> = rows.iter().filter(|row| row.assumed_mastery()).collect();
    if seeded.is_empty() {
        out.push_str("No course seeds a topic of this course as mastered.\n\n");
        return out;
    }
    out.push_str(
        "| topic | seeded by | diagnostic | confirmable | remediable |\n|---|---|---|---|---|\n",
    );
    for row in seeded.iter().take(LIST_LIMIT) {
        let evidence = row.floor_evidence();
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | {} |",
            row.topic_id,
            row.assumed_by.join(", "),
            row.diagnostic.as_str(),
            yes_no(evidence.confirmable),
            yes_no(evidence.remediable),
        );
    }
    if seeded.len() > LIST_LIMIT {
        let _ = writeln!(
            out,
            "\n… and {} more seeded topics.",
            seeded.len() - LIST_LIMIT
        );
    }
    out.push('\n');
    out
}

/// The two words the evidence columns print.
const fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
