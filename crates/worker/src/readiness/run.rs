//! The run: read the content index, resolve the audit, and put every knowledge
//! point through the serve, render and grade contracts.

use std::collections::BTreeMap;

use cadus_core::answer::{Outcome, check};
use cadus_core::curriculum::{AnswerKind, Curriculum, KnowledgePoint, Topic};
use cadus_core::pool::{ExemplarSource, ProblemSource, kp_key};
use cadus_core::readiness::{ReadinessIndex, ReadinessReport, ReadinessSet};
use cadus_store::Db;
use cadus_store::content::approved_index;

use crate::WorkerError;

/// What the three contracts said about one knowledge point.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContractCheck {
    /// The instances the serve path built.
    pub instances: usize,
    /// The candidates the serve path refused.
    pub refusals: usize,
    /// The reason the serve path built nothing at all.
    pub no_problem: Option<String>,
    /// The rendered statements that came back empty.
    pub empty_statements: usize,
    /// The authored answers the checker does not mark correct against
    /// themselves, as `"<answer>: <reason>"`.
    pub grade_failures: Vec<String>,
}

impl ContractCheck {
    /// Whether all three contracts hold.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.no_problem.is_none() && self.empty_statements == 0 && self.grade_failures.is_empty()
    }
}

/// One readiness run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessRun {
    /// The course the run covers, or `None` for every course.
    pub course: Option<String>,
    /// The counts per course, per topic and per knowledge point.
    pub report: ReadinessReport,
    /// The contract result of every knowledge point, by serving key.
    pub contracts: BTreeMap<String, ContractCheck>,
    /// The approved documents the store holds, by kind.
    pub approved_documents: BTreeMap<String, usize>,
}

impl ReadinessRun {
    /// The knowledge points whose three contracts do not all hold.
    #[must_use]
    pub fn contract_failures(&self) -> Vec<(&String, &ContractCheck)> {
        self.contracts
            .iter()
            .filter(|(_, contract)| !contract.ok())
            .collect()
    }
}

/// Run the audit over `curriculum` against the approved documents of `db`.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the content read fails or passes its
/// bound.
pub async fn run(
    db: &Db,
    curriculum: &Curriculum,
    course: Option<&str>,
) -> Result<ReadinessRun, WorkerError> {
    let content = approved_index(db.pool()).await?;
    let index = ReadinessIndex::build(curriculum);
    let set: ReadinessSet = index.resolve(&content);
    let report = ReadinessReport::build(&index, &set, course);
    let mut approved_documents: BTreeMap<String, usize> = BTreeMap::new();
    for ((_, kind), count) in &content.documents {
        *approved_documents.entry(kind.clone()).or_insert(0) += count;
    }
    Ok(ReadinessRun {
        course: course.map(ToOwned::to_owned),
        contracts: contracts_of(curriculum, course),
        report,
        approved_documents,
    })
}

/// The contract result of every knowledge point of the course.
fn contracts_of(curriculum: &Curriculum, course: Option<&str>) -> BTreeMap<String, ContractCheck> {
    let mut out: BTreeMap<String, ContractCheck> = BTreeMap::new();
    for (position, topic) in curriculum.topics().iter().enumerate() {
        if let Some(wanted) = course {
            let idx = match u32::try_from(position) {
                Ok(raw) => cadus_core::curriculum::TopicIdx::from_u32(raw),
                Err(_) => continue,
            };
            if curriculum.course_of(idx) != wanted {
                continue;
            }
        }
        for kp in &topic.knowledge_points {
            let key = kp_key(topic.id.as_str(), kp.id.as_str());
            out.insert(key.clone(), one_contract(&key, topic, kp));
        }
    }
    out
}

/// The serve, render and grade contracts of one knowledge point.
fn one_contract(key: &str, topic: &Topic, kp: &KnowledgePoint) -> ContractCheck {
    let mut result = ContractCheck {
        grade_failures: grade_failures(kp, topic.answer_kind),
        ..ContractCheck::default()
    };
    // The SERVE contract: the exact call `crates/web/src/serve/draw.rs` makes.
    let source = ExemplarSource::new(key, kp.exemplars.as_slice());
    match source.fill(key, source.len().max(1), 0) {
        Ok(batch) => {
            result.instances = batch.instances().len();
            result.refusals = batch.refusals().len();
            // The RENDER contract: the statement the pool row stores.
            result.empty_statements = batch
                .instances()
                .iter()
                .filter(|instance| instance.text.trim().is_empty())
                .count();
        }
        Err(reason) => result.no_problem = Some(reason.to_string()),
    }
    result
}

/// The authored answers the checker does not mark correct against themselves.
///
/// The learner who types the authored answer must be marked correct. An answer
/// that fails here fails at the grade route for every learner who gets it right
/// (audit finding a).
fn grade_failures(kp: &KnowledgePoint, kind: AnswerKind) -> Vec<String> {
    let mut out = Vec::new();
    for exemplar in &kp.exemplars {
        if exemplar.answer_contract.is_some() {
            if let Err(reason) = exemplar.canonical_answer() {
                out.push(format!("{}: {}", exemplar.answer, reason.reason));
            }
            continue;
        }
        let reason = match check(&exemplar.answer, &exemplar.answer, kind) {
            Outcome::Decided(verdict) if verdict.correct => continue,
            Outcome::Decided(_) => "the checker marks the authored answer wrong".to_owned(),
            Outcome::Undecidable(refusal) => refusal.reason.to_string(),
        };
        out.push(format!("{}: {reason}", exemplar.answer));
    }
    out
}
