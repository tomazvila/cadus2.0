//! The program registry rules (D-VBE1).
//!
//! A course may hold `program.yaml`, the list of the rows of the official
//! program. Each row must be in the `program_items` of a topic of the course,
//! or the row must carry an `excluded` reason. A topic must not name a row that
//! the registry lacks. A course with no registry gets no finding.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

use super::super::super::finding::Finding;
use super::super::super::model::Catalog;
use super::super::Lint;

/// The registry file name inside a course folder.
pub const PROGRAM_FILE: &str = "program.yaml";

/// One row of the registry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::curriculum::lint) struct ProgramRow {
    pub row: u32,
    /// The name of the row: a program term, a theorem label, or any short
    /// text. `lt_term` is an accepted alias (the first registry used it).
    #[serde(default, alias = "lt_term")]
    pub label: Option<String>,
    #[serde(default)]
    pub excluded: Option<String>,
}

/// The registry document.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgramFile {
    #[serde(default)]
    program: Vec<ProgramRow>,
}

/// Read the registry of one course. `Ok(None)` means the file is absent.
fn read_registry(path: &Path) -> Result<Option<Vec<ProgramRow>>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    serde_norway::from_str::<ProgramFile>(&text)
        .map(|file| Some(file.program))
        .map_err(|error| error.to_string())
}

/// File `program_item_uncovered`, `program_item_unknown` and `program_registry`.
pub(in crate::curriculum::lint) fn check_program(
    lint: &mut Lint<'_>,
    catalog: &Catalog,
    root: &Path,
) {
    for course in &catalog.courses {
        let course_id = course.id.as_str();
        let file = format!("{course_id}/{PROGRAM_FILE}");
        let rows = match read_registry(&root.join(course_id).join(PROGRAM_FILE)) {
            Ok(None) => continue,
            Ok(Some(rows)) => rows,
            Err(error) => {
                lint.findings.push(
                    Finding::new("program_registry", format!("{file}: {error}")).with_file(&file),
                );
                continue;
            }
        };
        let mut seen: BTreeSet<u32> = BTreeSet::new();
        for row in &rows {
            let problem = if row.row == 0 {
                Some("row number 0 is not valid. Row numbers start at 1.".to_owned())
            } else if !seen.insert(row.row) {
                Some(format!("row number {} appears more than once.", row.row))
            } else {
                None
            };
            if let Some(problem) = problem {
                lint.findings.push(
                    Finding::new("program_registry", format!("{file}: {problem}")).with_file(&file),
                );
            }
        }
        let known: BTreeSet<u32> = rows.iter().map(|row| row.row).collect();
        let mut covered: BTreeSet<u32> = BTreeSet::new();
        for position in 0..lint.table.topics.len() {
            if lint.table.course(position) != course_id {
                continue;
            }
            let topic = lint.table.topics[position];
            for item in &topic.program_items {
                covered.insert(*item);
                if !known.contains(item) {
                    lint.findings.push(
                        Finding::new(
                            "program_item_unknown",
                            format!(
                                "{}: program item {item} is not a row of {file}. \
                                 Add the row to the registry or remove it from the topic.",
                                topic.id
                            ),
                        )
                        .with_topic(topic.id.as_str())
                        .with_file(lint.table.file(position)),
                    );
                }
            }
        }
        for row in &rows {
            let excluded = row
                .excluded
                .as_deref()
                .is_some_and(|reason| !reason.trim().is_empty());
            if !covered.contains(&row.row) && !excluded {
                let term = row
                    .label
                    .as_deref()
                    .map_or_else(String::new, |term| format!(" ({term})"));
                lint.findings.push(
                    Finding::new(
                        "program_item_uncovered",
                        format!(
                            "{course_id}: program row {}{term} is in no topic. \
                             Add it to the program_items of a topic, or give it an \
                             `excluded` reason in {file}.",
                            row.row
                        ),
                    )
                    .with_file(&file),
                );
            }
        }
    }
}
