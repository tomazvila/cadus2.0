//! The tree and the view of one KP: the exemplars with hash, verdict and
//! status, and the counts V and U. Each subcommand reads the tree through this
//! file, so one rule gives the `file` field, the floor and the status.

use std::collections::BTreeSet;
use std::path::Path;

use cadus_core::curriculum::load::{RawCurriculum, RawUnit, load_raw_curriculum};
use cadus_core::curriculum::{AnswerKind, Curriculum, Exemplar, KnowledgePoint, Topic};
use cadus_core::learner::problem_text_hash;
use serde_json::{Value, json};

use super::Fail;
use super::invariants::{grader, label};

/// The hard floor of I2: a KP with fewer verdict exemplars does not serve.
pub const HARD_FLOOR: usize = 4;

/// One exemplar of a KP view: from the tree, or a new item of a row.
#[derive(Debug, Clone)]
pub struct Item {
    /// The item id of a row (`n3`). `None` for an exemplar of the tree.
    pub id: Option<String>,
    /// The rule id of a row item (`R14`). `None` for an exemplar of the tree.
    pub rule: Option<String>,
    pub hash: String,
    pub exemplar: Exemplar,
    /// The contract as JSON. `Null` if the exemplar has no contract.
    pub contract: Value,
    /// `verdict_policy` gives a verdict.
    pub verdict: bool,
    /// The contract is `written`: a reference sentence for the model grader.
    pub written: bool,
    /// The text of the I3 breach of a verdict exemplar, if there is one.
    pub grader: Option<String>,
    /// I12, I13 and I14 apply (a new or changed exemplar).
    pub is_new: bool,
}

impl Item {
    /// Build an item. `contract` is the JSON form of `exemplar.answer_contract`.
    pub fn new(exemplar: Exemplar, contract: Value, kind: AnswerKind) -> Self {
        let verdict = exemplar.verdict_policy(kind).is_ok();
        let written = exemplar.is_written();
        let grader = verdict
            .then(|| grader::probe(&exemplar, &contract, kind))
            .flatten();
        Self {
            id: None,
            rule: None,
            hash: problem_text_hash(&exemplar.problem),
            exemplar,
            contract,
            verdict,
            written,
            grader,
            is_new: false,
        }
    }

    fn of_tree(exemplar: &Exemplar, kind: AnswerKind) -> Self {
        let contract = serde_json::to_value(&exemplar.answer_contract).unwrap_or(Value::Null);
        Self::new(exemplar.clone(), contract, kind)
    }

    /// The exemplar gives no verdict and is not a written item (I4, U).
    pub fn unmarked(&self) -> bool {
        !self.verdict && !self.written
    }

    /// The contract is not `label`, `none` or `written`: a typed answer.
    pub fn is_typed(&self) -> bool {
        !self.written && !matches!(self.kind(), Some("label" | "none" | "written"))
    }

    /// The `kind` text of the contract, if the exemplar has a contract.
    pub fn kind(&self) -> Option<&str> {
        self.contract["kind"].as_str()
    }
}

/// One KP with the data that the invariants read.
#[derive(Debug, Clone)]
pub struct KpView {
    /// `course/topic/kp`.
    pub kp: String,
    /// `topic/kp`: the key of the KP in `content_store`.
    pub store_key: String,
    pub course: String,
    pub file: String,
    /// The index of the unit file in the load order of the tree.
    pub unit: usize,
    pub answer_kind: AnswerKind,
    pub topic: Value,
    pub kp_block: Value,
    pub proof_kp: bool,
    pub floor: usize,
    pub items: Vec<Item>,
    pub diagnostic_hash: Option<String>,
    pub teach_hash: Option<String>,
}

impl KpView {
    /// V: the verdict exemplars with different hashes.
    pub fn v(&self) -> usize {
        let hashes: BTreeSet<&str> = self
            .items
            .iter()
            .filter(|item| item.verdict)
            .map(|item| item.hash.as_str())
            .collect();
        hashes.len()
    }

    /// The typed V: the verdict exemplars with different hashes whose contract
    /// is not `label`, `none` or `written`. Rule I9 reads it.
    pub fn typed_v(&self) -> usize {
        let hashes: BTreeSet<&str> = self
            .items
            .iter()
            .filter(|item| item.verdict && item.is_typed())
            .map(|item| item.hash.as_str())
            .collect();
        hashes.len()
    }

    /// U: the exemplars that give no verdict and are not written items.
    pub fn u(&self) -> usize {
        self.items.iter().filter(|item| item.unmarked()).count()
    }

    /// W: the written items. They give no verdict and do not count toward V or U.
    pub fn w(&self) -> usize {
        self.items.iter().filter(|item| item.written).count()
    }

    /// The text of the I5 breach of the item at `index`, if there is one.
    pub fn duplicate_of(&self, index: usize) -> Option<&'static str> {
        let hash = Some(&self.items[index].hash);
        if self.items[..index]
            .iter()
            .any(|item| Some(&item.hash) == hash)
        {
            Some("I5: the problem is equal to an earlier exemplar of the KP")
        } else if hash == self.diagnostic_hash.as_ref() {
            Some("I5: the problem is equal to the diagnostic exemplar of the topic")
        } else if hash == self.teach_hash.as_ref() {
            Some("I5: the problem is equal to the worked example of the teach page")
        } else {
            None
        }
    }

    /// The status of content-spec X7 and its reason.
    pub fn status(&self, index: usize) -> (&'static str, Option<String>) {
        let item = &self.items[index];
        if !item.verdict {
            return ("unmarked", None);
        }
        let reason = item
            .grader
            .as_ref()
            .map(|text| format!("I3: {text}"))
            .or_else(|| self.duplicate_of(index).map(str::to_owned))
            .or_else(|| label::option_count_breach(item).map(|text| format!("I8: {text}")));
        match reason {
            Some(reason) => ("must_replace", Some(reason)),
            None => ("keep", None),
        }
    }

    /// Set the worked-example problem of the approved teach page.
    pub fn set_teach_problem(&mut self, problem: Option<&str>) {
        self.teach_hash = problem.map(problem_text_hash);
    }
}

/// The floor of I2 for a course: 4 for `foundations` and `geometry`, else 6.
pub fn floor_of(course: &str) -> usize {
    if matches!(course, "foundations" | "geometry") {
        HARD_FLOOR
    } else {
        6
    }
}

fn is_proof_kp(topic: &Topic, kp: &KnowledgePoint) -> bool {
    topic.answer_kind == AnswerKind::Proof
        || ["Prove", "Show", "Justify"]
            .iter()
            .any(|word| kp.name.starts_with(word))
}

/// A curriculum tree that the loader read with no fatal finding.
#[derive(Debug)]
pub struct Tree {
    pub raw: RawCurriculum,
    /// The name of the base directory: the first part of each `file` value.
    name: String,
}

impl Tree {
    /// Load the tree. No `courses.yaml` is exit 2; a loader refusal is exit 3.
    pub fn load(base: &str) -> Result<Self, Fail> {
        let path = Path::new(base);
        let (raw, findings) =
            load_raw_curriculum(path).map_err(|error| Fail::input(error.to_string()))?;
        let refused = findings
            .iter()
            .find(|finding| finding.fatal)
            .map(|finding| {
                let file = finding.file.as_deref().unwrap_or("-");
                format!("{file}: {}: {}", finding.code, finding.message)
            })
            .or_else(|| {
                Curriculum::build(raw.clone())
                    .err()
                    .as_ref()
                    .map(ToString::to_string)
            });
        if let Some(text) = refused {
            return Err(Fail::refused(format!(
                "the loader refused the tree: {text}"
            )));
        }
        let name = path
            .canonicalize()
            .ok()
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Self { raw, name })
    }

    /// The `file` field: the unit file that the loader read the KP from. Its
    /// header has the `unit:` and the `course:` of the KP (pack v2, D25).
    fn file_of(&self, unit: &RawUnit) -> String {
        format!("{}/{}/{}", self.name, unit.course_id, unit.file_name)
    }

    fn build(&self, index: usize, topic: &Topic, kp: &KnowledgePoint) -> KpView {
        let unit = &self.raw.units[index];
        let course = unit.unit.course.as_str();
        let slugs = |ids: Vec<&str>| Value::from(ids);
        let diagnostic = topic.diagnostic_exemplar.as_ref();
        KpView {
            kp: format!("{course}/{}/{}", topic.id, kp.id),
            store_key: format!("{}/{}", topic.id, kp.id),
            course: course.to_owned(),
            file: self.file_of(unit),
            unit: index,
            answer_kind: topic.answer_kind,
            topic: json!({"id": topic.id.as_str(), "name": topic.name,
                "answer_kind": topic.answer_kind.as_str(),
                "prerequisites": slugs(topic.prerequisites.iter().map(|edge| edge.id.as_str()).collect()),
                "diagnostic_problem": diagnostic.map(|exemplar| exemplar.problem.as_str())}),
            kp_block: json!({"id": kp.id.as_str(), "name": kp.name,
                "key_prerequisites": slugs(kp.key_prerequisites.iter().map(|id| id.as_str()).collect()),
                "constraints": kp.constraints}),
            proof_kp: is_proof_kp(topic, kp),
            floor: floor_of(course),
            items: kp
                .exemplars
                .iter()
                .map(|exemplar| Item::of_tree(exemplar, topic.answer_kind))
                .collect(),
            diagnostic_hash: diagnostic.map(|exemplar| problem_text_hash(&exemplar.problem)),
            teach_hash: None,
        }
    }

    /// Each KP of the tree in load order, with the index of its unit file.
    fn each_kp(&self) -> impl Iterator<Item = (usize, &Topic, &KnowledgePoint)> {
        self.raw.units.iter().enumerate().flat_map(|(index, unit)| {
            unit.unit.topics.iter().flat_map(move |topic| {
                topic
                    .knowledge_points
                    .iter()
                    .map(move |kp| (index, topic, kp))
            })
        })
    }

    /// The views of one course, or of each course (`None`), in load order.
    pub fn views(&self, course: Option<&str>) -> Vec<KpView> {
        self.each_kp()
            .filter(|(index, _, _)| {
                course.is_none_or(|course| self.raw.units[*index].unit.course.as_str() == course)
            })
            .map(|(index, topic, kp)| self.build(index, topic, kp))
            .collect()
    }

    /// The views of one unit file, in file order.
    pub fn file_views(&self, unit: usize) -> Vec<KpView> {
        self.each_kp()
            .filter(|(index, _, _)| *index == unit)
            .map(|(index, topic, kp)| self.build(index, topic, kp))
            .collect()
    }

    /// The view of one KP id `course/topic/kp`. An unknown id is exit 2.
    pub fn view(&self, id: &str) -> Result<KpView, Fail> {
        self.each_kp()
            .find(|(index, topic, kp)| {
                let course = self.raw.units[*index].unit.course.as_str();
                format!("{course}/{}/{}", topic.id, kp.id) == id
            })
            .map(|(index, topic, kp)| self.build(index, topic, kp))
            .ok_or_else(|| Fail::input(format!("the KP `{id}` is not in the tree")))
    }

    /// True if the catalog or a unit file names the course.
    pub fn has_course(&self, course: &str) -> bool {
        self.raw
            .units
            .iter()
            .any(|unit| unit.unit.course.as_str() == course)
    }
}
