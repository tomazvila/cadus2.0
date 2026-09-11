//! The two renderings of one readiness run: the Markdown an operator reads and
//! the JSON a script reads.

use std::fmt::Write as _;

use cadus_core::readiness::{Blocker, CourseReport, Readiness, TopicReport};
use serde_json::{Value, json};

use super::run::{ContractCheck, ReadinessRun};

/// The knowledge points one topic section lists in full.
///
/// A course of 809 knowledge points writes 809 lines, and the operator reads
/// the counts first. The per-knowledge-point lines therefore name the MISSING
/// items and stop at this many per topic; the JSON carries every one.
pub const KP_LINES_PER_TOPIC: usize = 12;

/// The run, as the Markdown report the repository commits.
#[must_use]
pub fn render_markdown(run: &ReadinessRun, generated: &str) -> String {
    let mut text = String::new();
    let scope = run.course.as_deref().unwrap_or("every course");
    let _ = writeln!(text, "# Readiness report — {scope}\n");
    let _ = writeln!(text, "Generated {generated} by `cadus-worker readiness`.\n");
    write_summary(&mut text, run);
    write_histogram(&mut text, run);
    write_contracts(&mut text, run);
    for course in &run.report.courses {
        write_course(&mut text, course);
    }
    text
}

/// The headline counts and the one sentence that explains them.
fn write_summary(text: &mut String, run: &ReadinessRun) {
    let (ready, blocked) = run.report.totals();
    let documents: usize = run.approved_documents.values().sum();
    let _ = writeln!(text, "## Summary\n");
    let _ = writeln!(text, "| Measure | Count |");
    let _ = writeln!(text, "|---|---:|");
    let _ = writeln!(text, "| Knowledge points a lesson serves | {ready} |");
    let _ = writeln!(text, "| Knowledge points a lesson blocks | {blocked} |");
    let _ = writeln!(
        text,
        "| Approved documents in `content_store` | {documents} |"
    );
    for (kind, count) in &run.approved_documents {
        let _ = writeln!(text, "| Approved `{kind}` documents | {count} |");
    }
    let _ = writeln!(text);
    if documents == 0 {
        let _ = writeln!(
            text,
            "`content_store` holds no approved document. Every knowledge point is \
             therefore blocked on `teachable`, and a lesson serves none of them. \
             The blockers under that line are the ones an authoring pass does NOT \
             fix on its own.\n"
        );
    }
}

/// The blocker histogram over every knowledge point of the scope.
fn write_histogram(text: &mut String, run: &ReadinessRun) {
    let _ = writeln!(text, "## Blockers\n");
    let _ = writeln!(text, "| Blocker | Knowledge points |");
    let _ = writeln!(text, "|---|---:|");
    let histogram = run.report.histogram();
    for blocker in Blocker::every() {
        let count = histogram.get(&blocker).copied().unwrap_or(0);
        let _ = writeln!(text, "| `{}` | {count} |", blocker.as_str());
    }
    let _ = writeln!(text);
}

/// The serve, render and grade contracts, and every knowledge point that fails
/// one.
fn write_contracts(text: &mut String, run: &ReadinessRun) {
    let failures = run.contract_failures();
    let _ = writeln!(text, "## Serve, render and grade contracts\n");
    let _ = writeln!(
        text,
        "Every knowledge point ran through the A6 serve path, the statement the \
         pool row stores, and the checker with its own authored answer.\n"
    );
    let _ = writeln!(
        text,
        "| Measure | Count |\n|---|---:|\n| Knowledge points checked | {} |\n\
         | Knowledge points that fail a contract | {} |\n",
        run.contracts.len(),
        failures.len()
    );
    if failures.is_empty() {
        let _ = writeln!(text, "Every knowledge point passes all three.\n");
        return;
    }
    let _ = writeln!(text, "| Knowledge point | What failed |");
    let _ = writeln!(text, "|---|---|");
    for (key, contract) in failures {
        let _ = writeln!(text, "| `{key}` | {} |", contract_reason(contract));
    }
    let _ = writeln!(text);
}

/// The one-line reason a knowledge point fails a contract.
fn contract_reason(contract: &ContractCheck) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(reason) = &contract.no_problem {
        parts.push(format!("serve: {reason}"));
    }
    if contract.empty_statements > 0 {
        parts.push(format!(
            "render: {} empty statement(s)",
            contract.empty_statements
        ));
    }
    for failure in &contract.grade_failures {
        parts.push(format!("grade: {failure}"));
    }
    parts.join("; ")
}

/// One course section: the counts, then a table per topic.
fn write_course(text: &mut String, course: &CourseReport) {
    let _ = writeln!(text, "## Course `{}`\n", course.course_id);
    let _ = writeln!(
        text,
        "Topics {}, knowledge points {}, ready {}, blocked {}.\n",
        course.topics, course.knowledge_points, course.ready, course.blocked
    );
    let _ = writeln!(text, "| Topic | Ready | Blocked | Blockers |");
    let _ = writeln!(text, "|---|---:|---:|---|");
    for topic in &course.topic_reports {
        let _ = writeln!(
            text,
            "| `{}` | {} | {} | {} |",
            topic.topic_id,
            topic.ready,
            topic.blocked,
            blocker_names(&topic.blocker_list())
        );
    }
    let _ = writeln!(text);
    let _ = writeln!(text, "### Knowledge points that need work\n");
    for topic in &course.topic_reports {
        write_topic_points(text, topic);
    }
}

/// The blocked knowledge points of one topic, and what each one needs.
fn write_topic_points(text: &mut String, topic: &TopicReport) {
    let blocked: Vec<&Readiness> = topic
        .knowledge_points
        .iter()
        .filter(|readiness| !readiness.blockers().is_empty())
        .collect();
    if blocked.is_empty() {
        return;
    }
    let _ = writeln!(text, "- `{}`", topic.topic_id);
    for readiness in blocked.iter().take(KP_LINES_PER_TOPIC) {
        let _ = writeln!(
            text,
            "  - `{}` — {} (decidable exemplars {}, approved templates {}, practice items {})",
            readiness.kp_key,
            blocker_names(&readiness.blockers()),
            readiness.decidable_exemplars,
            readiness.approved_templates,
            readiness.practice_items
        );
    }
    if blocked.len() > KP_LINES_PER_TOPIC {
        let _ = writeln!(
            text,
            "  - … {} more blocked knowledge point(s); the JSON report holds every one.",
            blocked.len() - KP_LINES_PER_TOPIC
        );
    }
}

/// The blocker names of one list, comma separated, or `none`.
fn blocker_names(blockers: &[Blocker]) -> String {
    if blockers.is_empty() {
        return "none".to_owned();
    }
    blockers
        .iter()
        .map(|blocker| format!("`{}`", blocker.as_str()))
        .collect::<Vec<String>>()
        .join(", ")
}

/// The run, as the JSON a script reads. Every knowledge point is in it.
#[must_use]
pub fn render_json(run: &ReadinessRun) -> Value {
    let (ready, blocked) = run.report.totals();
    json!({
        "course": run.course,
        "ready": ready,
        "blocked": blocked,
        "approved_documents": run.approved_documents,
        "histogram": histogram_json(run),
        "courses": run
            .report
            .courses
            .iter()
            .map(course_json)
            .collect::<Vec<Value>>(),
        "contracts": run
            .contracts
            .iter()
            .map(|(key, contract)| {
                json!({
                    "kp_id": key,
                    "instances": contract.instances,
                    "refusals": contract.refusals,
                    "no_problem": contract.no_problem,
                    "empty_statements": contract.empty_statements,
                    "grade_failures": contract.grade_failures,
                })
            })
            .collect::<Vec<Value>>(),
    })
}

/// The blocker histogram, by wire name.
fn histogram_json(run: &ReadinessRun) -> Value {
    let histogram = run.report.histogram();
    let pairs: serde_json::Map<String, Value> = Blocker::every()
        .into_iter()
        .map(|blocker| {
            (
                blocker.as_str().to_owned(),
                json!(histogram.get(&blocker).copied().unwrap_or(0)),
            )
        })
        .collect();
    Value::Object(pairs)
}

/// One course, with every topic and every knowledge point.
fn course_json(course: &CourseReport) -> Value {
    json!({
        "course_id": course.course_id,
        "topics": course.topics,
        "knowledge_points": course.knowledge_points,
        "ready": course.ready,
        "blocked": course.blocked,
        "topic_reports": course
            .topic_reports
            .iter()
            .map(topic_json)
            .collect::<Vec<Value>>(),
    })
}

/// One topic, with every knowledge point.
fn topic_json(topic: &TopicReport) -> Value {
    json!({
        "topic_id": topic.topic_id,
        "ready": topic.ready,
        "blocked": topic.blocked,
        "blockers": topic
            .blocker_list()
            .iter()
            .map(|blocker| blocker.as_str())
            .collect::<Vec<&str>>(),
        "knowledge_points": topic
            .knowledge_points
            .iter()
            .map(readiness_json)
            .collect::<Vec<Value>>(),
    })
}

/// One knowledge point, with every condition.
fn readiness_json(readiness: &Readiness) -> Value {
    json!({
        "kp_id": readiness.kp_key,
        "teachable": readiness.teachable,
        "practicable": readiness.practicable,
        "assessable": readiness.assessable,
        "hints": readiness.hints,
        "solutions": readiness.solutions,
        "prerequisites_ok": readiness.prerequisites_ok,
        "visual_needed": readiness.visual_needed,
        "visual_present": readiness.visual_present,
        "decidable_exemplars": readiness.decidable_exemplars,
        "approved_templates": readiness.approved_templates,
        "practice_items": readiness.practice_items,
        "blockers": readiness
            .blockers()
            .iter()
            .map(|blocker| blocker.as_str())
            .collect::<Vec<&str>>(),
    })
}
