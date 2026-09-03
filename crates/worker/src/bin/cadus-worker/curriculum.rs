//! The curriculum tree the worker reads, and the first reason a tree does not
//! load.

use std::path::PathBuf;

use cadus_core::curriculum::{Curriculum, CurriculumError, Finding, LoadError, load_curriculum};
use cadus_worker::WorkerError;

/// The environment variable that names the curriculum tree.
///
/// The refill job (D-O4) reads the authored exemplars from it for the A6
/// fallback, and it reads the topic answer kind for the gate re-run.
const CURRICULUM_ENV: &str = "CADUS_CURRICULUM";

/// The tree the worker reads when the variable names none.
///
/// The path is relative to the working directory. The image sets
/// `CADUS_CURRICULUM=/app/curriculum` and carries the tree there, and the
/// repository holds `curriculum/` at its root, so both the container and a run
/// from the repository root find a tree without an operator flag.
const DEFAULT_CURRICULUM: &str = "curriculum";

/// Read the curriculum tree that `CADUS_CURRICULUM` names.
///
/// # Errors
///
/// Returns [`WorkerError::Config`] when the tree does not load. The message names
/// the path and the first finding, so an operator reads the cause in the one line
/// the process prints before it exits 2.
pub(crate) fn load_arena() -> Result<Curriculum, WorkerError> {
    let path = PathBuf::from(
        std::env::var(CURRICULUM_ENV).unwrap_or_else(|_| DEFAULT_CURRICULUM.to_string()),
    );
    let shown = path.display().to_string();
    match load_curriculum(&path) {
        Ok((curriculum, findings)) => {
            let topics = curriculum.topic_count();
            let findings = findings.len();
            tracing::info!(
                path = shown,
                topics,
                findings,
                "cadus-worker: curriculum is loaded"
            );
            Ok(curriculum)
        }
        Err(err) => {
            let reason = first_reason(&err);
            tracing::error!(
                path = shown,
                error = reason,
                "cadus-worker: the curriculum did not load; set CADUS_CURRICULUM to a tree that does"
            );
            Err(WorkerError::Config(format!(
                "the curriculum at {shown} did not load: {reason}"
            )))
        }
    }
}

/// The first finding of a load error, or the error itself.
///
/// A fatal parse stage carries every finding, and the joined text of a large tree
/// runs to many lines. The first one names the file the operator must fix.
fn first_reason(err: &LoadError) -> String {
    let findings: &[Finding] = match err {
        LoadError::Curriculum(CurriculumError::FatalFindings { findings }) => findings,
        _ => &[],
    };
    findings.first().map_or_else(
        || err.to_string(),
        |finding| format!("[{}] {}", finding.code, finding.message),
    )
}

#[cfg(test)]
mod tests {
    use cadus_core::curriculum::{CurriculumError, Finding, LoadError};

    use super::first_reason;

    /// A fatal parse stage names its first finding; every other load error
    /// gives its own text.
    #[test]
    fn the_first_finding_names_the_file_the_operator_must_fix() {
        let fatal = LoadError::Curriculum(CurriculumError::FatalFindings {
            findings: vec![
                Finding::new("E001", "the first one"),
                Finding::new("E002", "the second one"),
            ],
        });
        assert_eq!(first_reason(&fatal), "[E001] the first one");
        let empty = LoadError::Curriculum(CurriculumError::FatalFindings {
            findings: Vec::new(),
        });
        assert_eq!(first_reason(&empty), empty.to_string());
    }
}
