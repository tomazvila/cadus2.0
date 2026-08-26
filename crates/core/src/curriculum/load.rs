//! The parse stage of the curriculum loader (C5, spec sections 1 and 5).
//!
//! The stage reads `courses.yaml` and the unit files under each course
//! directory, and returns the parsed documents together with the parse-stage
//! findings. It never panics on content: a broken file becomes a finding and the
//! loader continues with the next file, the same as 1.0 `_parse_curriculum`
//! (`cadus/graph.py:561-614`).
//!
//! ## Schema errors
//!
//! `serde` alone reports neither the dotted location of a bad value nor the
//! range rules of `difficulty`, `weight` and `expected_time_secs`, so this
//! module walks the parsed YAML document against the schema of section 1 and
//! builds the findings itself. The message text copies the 1.0 pydantic text, so
//! the two implementations report the same defect the same way. The typed value
//! comes from `serde` afterwards, with `deny_unknown_fields` as the backstop.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_norway::{Mapping, Value};

use super::finding::Finding;
use super::model::{ANKI_TYPES, ANSWER_KINDS, Catalog, Topic, Unit};

/// The name of the mandatory catalog file.
const COURSES_FILE: &str = "courses.yaml";

/// The extension of a unit file. 1.0 globs `*.yaml`, so `.yml` is invisible.
const UNIT_EXTENSION: &str = ".yaml";

/// The parse stage could not start.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// No `courses.yaml` under the curriculum root. 1.0 raises
    /// `CurriculumNotFound` here (`graph.py:570-572`).
    #[error("no courses.yaml under {}", path.display())]
    CurriculumNotFound { path: PathBuf },
}

/// One unit file that passed the schema check.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedUnit {
    /// The catalog course whose directory holds the file.
    ///
    /// This is the directory, not the `course` field of the file. 1.0 does not
    /// compare the two and reads the field wherever a course of a topic is
    /// needed; `unit.course` stays the authority.
    pub course_id: String,
    /// The file name inside the course directory, for example `01-numbers.yaml`.
    pub file_name: String,
    /// The parsed unit.
    pub unit: Unit,
}

impl ParsedUnit {
    /// The path of the file relative to the curriculum root.
    pub fn rel_path(&self) -> String {
        format!("{}/{}", self.course_id, self.file_name)
    }
}

/// The result of the parse stage.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// The catalog, or `None` when `courses.yaml` failed to parse or is absent.
    pub catalog: Option<Catalog>,
    /// The unit files that passed, in load order.
    pub units: Vec<ParsedUnit>,
    /// The parse-stage findings, in the order the loader found them.
    pub findings: Vec<Finding>,
    /// Set when the stage could not start at all.
    pub error: Option<ParseError>,
}

/// Read a curriculum tree and return the parsed documents plus the parse-stage
/// findings (spec sections 1 and 5).
///
/// Load order is the order of `courses.yaml`, then the unit files of each course
/// in code-point order, then the `topics:` order inside each file. The order of
/// `courses.yaml` is the file order and not the `order` field (parity trap 1).
pub fn parse_curriculum(root: &Path) -> Parsed {
    let courses_path = root.join(COURSES_FILE);
    if !courses_path.is_file() {
        return Parsed {
            catalog: None,
            units: Vec::new(),
            findings: Vec::new(),
            error: Some(ParseError::CurriculumNotFound {
                path: root.to_path_buf(),
            }),
        };
    }

    let mut findings = Vec::new();
    let catalog: Catalog = match read_document(&courses_path, COURSES_FILE) {
        Ok(document) => match validate(&document, COURSES_FILE, Checker::check_catalog) {
            Ok(catalog) => catalog,
            Err(schema_findings) => {
                findings.extend(schema_findings);
                return Parsed {
                    catalog: None,
                    units: Vec::new(),
                    findings,
                    error: None,
                };
            }
        },
        Err(finding) => {
            findings.push(*finding);
            return Parsed {
                catalog: None,
                units: Vec::new(),
                findings,
                error: None,
            };
        }
    };

    let mut units = Vec::new();
    for course in &catalog.courses {
        let course_id = course.id.as_str();
        let course_dir = root.join(course_id);
        if !course_dir.is_dir() {
            // Nothing was dropped: there was nothing to read.
            findings.push(Finding::advisory(
                "missing_course_dir",
                format!("no unit directory {course_id}/ for course"),
            ));
            continue;
        }
        let file_names = match unit_file_names(&course_dir) {
            Ok(names) => names,
            Err(error) => {
                findings.push(
                    Finding::new("yaml", format!("{course_id}/: {error}"))
                        .with_file(format!("{course_id}/")),
                );
                continue;
            }
        };
        if file_names.is_empty() {
            // An empty course omits no topics, so the finding is advisory.
            findings.push(Finding::advisory(
                "empty_course",
                format!("course {course_id} has no unit files"),
            ));
        }
        for file_name in file_names {
            let rel = format!("{course_id}/{file_name}");
            let document = match read_document(&course_dir.join(&file_name), &rel) {
                Ok(document) => document,
                Err(finding) => {
                    findings.push(*finding);
                    continue;
                }
            };
            match validate(&document, &rel, Checker::check_unit) {
                Ok(unit) => units.push(ParsedUnit {
                    course_id: course_id.to_owned(),
                    file_name,
                    unit,
                }),
                Err(schema_findings) => findings.extend(schema_findings),
            }
        }
    }

    Parsed {
        catalog: Some(catalog),
        units,
        findings,
        error: None,
    }
}

/// The loaded units in load order, with a load index per topic. U2 builds the
/// arena from this (D1).
#[derive(Debug, Clone, PartialEq)]
pub struct RawCurriculum {
    /// The catalog, in file order.
    pub catalog: Catalog,
    /// The unit files that passed, in load order.
    pub units: Vec<RawUnit>,
}

/// One unit file of a [`RawCurriculum`].
#[derive(Debug, Clone, PartialEq)]
pub struct RawUnit {
    /// The catalog course whose directory holds the file.
    pub course_id: String,
    /// The file name inside the course directory.
    pub file_name: String,
    /// The parsed unit.
    pub unit: Unit,
    /// The load index of the first topic of the file.
    pub first_load_index: usize,
}

/// One topic of a [`RawCurriculum`], with its load index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawTopic<'a> {
    /// The position of the topic in load order, counted from 0.
    pub load_index: usize,
    /// The course directory the file came from.
    pub course_dir: &'a str,
    /// The file name inside that directory.
    pub file_name: &'a str,
    /// The unit the topic belongs to.
    pub unit: &'a Unit,
    /// The topic.
    pub topic: &'a Topic,
}

impl RawCurriculum {
    /// Build a raw curriculum from a parse result. `None` when the catalog is
    /// absent, because a load has no courses to walk then.
    pub fn from_parsed(parsed: Parsed) -> Option<Self> {
        let catalog = parsed.catalog?;
        let mut next_index = 0;
        let mut units = Vec::with_capacity(parsed.units.len());
        for parsed_unit in parsed.units {
            let count = parsed_unit.unit.topics.len();
            units.push(RawUnit {
                course_id: parsed_unit.course_id,
                file_name: parsed_unit.file_name,
                unit: parsed_unit.unit,
                first_load_index: next_index,
            });
            next_index += count;
        }
        Some(Self { catalog, units })
    }

    /// The number of topics in the tree.
    pub fn topic_count(&self) -> usize {
        self.units.iter().map(|u| u.unit.topics.len()).sum()
    }

    /// Every topic in load order.
    pub fn topics(&self) -> impl Iterator<Item = RawTopic<'_>> {
        self.units.iter().flat_map(|raw| {
            raw.unit
                .topics
                .iter()
                .enumerate()
                .map(move |(offset, topic)| RawTopic {
                    load_index: raw.first_load_index + offset,
                    course_dir: &raw.course_id,
                    file_name: &raw.file_name,
                    unit: &raw.unit,
                    topic,
                })
        })
    }
}

/// Read a curriculum tree into a [`RawCurriculum`] plus the parse-stage
/// findings. The error case is the absent `courses.yaml` of 1.0.
pub fn load_raw_curriculum(root: &Path) -> Result<(RawCurriculum, Vec<Finding>), ParseError> {
    let parsed = parse_curriculum(root);
    if let Some(error) = parsed.error {
        return Err(error);
    }
    let findings = parsed.findings.clone();
    match RawCurriculum::from_parsed(parsed) {
        Some(raw) => Ok((raw, findings)),
        None => Ok((
            RawCurriculum {
                catalog: Catalog {
                    courses: Vec::new(),
                },
                units: Vec::new(),
            },
            findings,
        )),
    }
}

// --------------------------------------------------------------------------- //
// File reading
// --------------------------------------------------------------------------- //

/// The unit file names of one course directory, in code-point order.
///
/// The glob of 1.0 is `*.yaml` and it is not recursive, so a subdirectory and a
/// `.yml` file stay invisible (parity traps 2 and 3).
fn unit_file_names(course_dir: &Path) -> Result<Vec<String>, std::io::Error> {
    let mut names = Vec::new();
    for entry in fs::read_dir(course_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with('.') || !name.ends_with(UNIT_EXTENSION) {
            continue;
        }
        names.push(name);
    }
    // A `String` sorts by its bytes, and UTF-8 bytes sort in code-point order.
    names.sort();
    Ok(names)
}

/// Read one YAML file. An empty document is `{}` (spec section 1).
///
/// The finding is boxed because it is much larger than the `Value` of the happy
/// path (`clippy::result_large_err`).
fn read_document(path: &Path, rel: &str) -> Result<Value, Box<Finding>> {
    let text = fs::read_to_string(path).map_err(|error| {
        Box::new(Finding::new("yaml", format!("{rel}: {error}")).with_file(rel))
    })?;
    let value: Value = serde_norway::from_str(&text).map_err(|error| {
        Box::new(Finding::new("yaml", format!("{rel}: {error}")).with_file(rel))
    })?;
    Ok(if is_falsy(&value) {
        Value::Mapping(Mapping::new())
    } else {
        value
    })
}

/// True when Python reads the value as false. 1.0 writes `model_validate(data or {})`,
/// so every falsy document — a null, an empty mapping, an empty list, an empty
/// string, a zero — becomes `{}`.
fn is_falsy(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(flag) => !flag,
        Value::Number(number) => number.as_f64() == Some(0.0),
        Value::String(text) => text.is_empty(),
        Value::Sequence(items) => items.is_empty(),
        Value::Mapping(map) => map.is_empty(),
        Value::Tagged(_) => false,
    }
}

// --------------------------------------------------------------------------- //
// Schema check
// --------------------------------------------------------------------------- //

/// Check one document against the schema, then build the typed value.
///
/// `check` walks the document and collects the findings. When it finds nothing,
/// `serde` builds the value; `deny_unknown_fields` and the type rules of
/// [`super::model`] are the backstop behind the walk.
fn validate<'f, T, F>(document: &Value, file: &'f str, check: F) -> Result<T, Vec<Finding>>
where
    T: for<'de> Deserialize<'de>,
    F: FnOnce(&mut Checker<'f>, &Value),
{
    let mut checker = Checker::new(file);
    check(&mut checker, document);
    if !checker.out.is_empty() {
        return Err(checker.out);
    }
    // A residual error means the walk and the types disagree. Report it rather
    // than drop the file in silence.
    T::deserialize(document.clone())
        .map_err(|error| vec![Finding::new("schema", format!(": {error}")).with_file(file)])
}

/// Walks a YAML document against the schema of spec section 1 and collects the
/// findings, with the dotted location in front of each message.
struct Checker<'a> {
    file: &'a str,
    loc: Vec<String>,
    out: Vec<Finding>,
}

impl<'a> Checker<'a> {
    fn new(file: &'a str) -> Self {
        Self {
            file,
            loc: Vec::new(),
            out: Vec::new(),
        }
    }

    fn push(&mut self, segment: impl Into<String>) {
        self.loc.push(segment.into());
    }

    fn pop(&mut self) {
        self.loc.pop();
    }

    /// Record a schema finding at the current location.
    fn report(&mut self, message: &str) {
        let loc = self.loc.join(".");
        self.out
            .push(Finding::new("schema", format!("{loc}: {message}")).with_file(self.file));
    }

    /// Record a range finding. 1.0 gives the `weight` range its own code
    /// (`graph.py:544-558`), which is how `cadus curriculum lint` reports it
    /// apart from the other schema errors.
    fn report_range(&mut self, message: &str) {
        if self.loc.iter().any(|segment| segment == "weight") {
            let loc = self.loc.join(".");
            self.out.push(
                Finding::new("weight_out_of_range", format!("{loc}: {message}"))
                    .with_file(self.file),
            );
        } else {
            self.report(message);
        }
    }

    /// The mapping of a struct value, or `None` after a report.
    fn struct_map<'v>(&mut self, value: &'v Value, type_name: &str) -> Option<&'v Mapping> {
        match value {
            Value::Mapping(map) => Some(map),
            _ => {
                self.report(&format!(
                    "Input should be a valid dictionary or instance of {type_name}"
                ));
                None
            }
        }
    }

    /// Visit one field. A missing required field is reported here.
    fn field<F>(&mut self, map: &Mapping, key: &str, required: bool, visit: F)
    where
        F: FnOnce(&mut Self, &Value),
    {
        match map.get(key) {
            Some(value) => {
                self.push(key);
                visit(self, value);
                self.pop();
            }
            None if required => {
                self.push(key);
                self.report("Field required");
                self.pop();
            }
            None => {}
        }
    }

    /// Visit every item of a list field.
    fn each<F>(&mut self, value: &Value, mut visit: F)
    where
        F: FnMut(&mut Self, &Value),
    {
        match value {
            Value::Sequence(items) => {
                for (index, item) in items.iter().enumerate() {
                    self.push(index.to_string());
                    visit(self, item);
                    self.pop();
                }
            }
            _ => self.report("Input should be a valid list"),
        }
    }

    /// Report every key the struct does not declare. 1.0 forbids extra keys at
    /// every level (parity trap 4).
    fn extras(&mut self, map: &Mapping, known: &[&str]) {
        for key in map.keys() {
            let name = key_name(key);
            if !known.contains(&name.as_str()) {
                self.push(name);
                self.report("Extra inputs are not permitted");
                self.pop();
            }
        }
    }

    fn check_string(&mut self, value: &Value) {
        if !matches!(value, Value::String(_)) {
            self.report("Input should be a valid string");
        }
    }

    fn check_slug(&mut self, value: &Value) {
        match value {
            Value::String(text) => {
                if text.trim().is_empty() {
                    self.report("String should have at least 1 character");
                }
            }
            _ => self.report("Input should be a valid string"),
        }
    }

    fn check_bool(&mut self, value: &Value) {
        if !matches!(value, Value::Bool(_)) {
            let message = if matches!(value, Value::String(_)) {
                "Input should be a valid boolean, unable to interpret input"
            } else {
                "Input should be a valid boolean"
            };
            self.report(message);
        }
    }

    /// A float in the closed range 0..=1 (`difficulty` and `weight`).
    fn check_unit_interval(&mut self, value: &Value) {
        let Some(number) = as_f64(value) else {
            let message = if matches!(value, Value::String(_)) {
                "Input should be a valid number, unable to parse string as a number"
            } else {
                "Input should be a valid number"
            };
            self.report(message);
            return;
        };
        if number < 0.0 {
            self.report_range("Input should be greater than or equal to 0");
        } else if number > 1.0 {
            self.report_range("Input should be less than or equal to 1");
        }
    }

    fn check_int(&mut self, value: &Value) {
        if as_i64(value).is_none() {
            let message = int_message(value);
            self.report(message);
        }
    }

    fn check_positive_int(&mut self, value: &Value) {
        match as_i64(value) {
            Some(number) if number > 0 => {}
            Some(_) => self.report_range("Input should be greater than 0"),
            None => {
                let message = int_message(value);
                self.report(message);
            }
        }
    }

    fn check_enum(&mut self, value: &Value, allowed: &[&str]) {
        let ok = match value {
            Value::String(text) => allowed.contains(&text.as_str()),
            _ => false,
        };
        if !ok {
            let message = format!("Input should be {}", quoted_alternatives(allowed));
            self.report(&message);
        }
    }

    // -- the models of spec section 1 -------------------------------------- //

    fn check_catalog(&mut self, value: &Value) {
        const FIELDS: [&str; 1] = ["courses"];
        let Some(map) = self.struct_map(value, "CourseCatalog") else {
            return;
        };
        self.field(map, "courses", false, |checker, value| {
            checker.each(value, Self::check_course);
        });
        self.extras(map, &FIELDS);
    }

    fn check_course(&mut self, value: &Value) {
        const FIELDS: [&str; 5] = [
            "id",
            "name",
            "order",
            "mastery_floor",
            "mastery_floor_course",
        ];
        let Some(map) = self.struct_map(value, "Course") else {
            return;
        };
        self.field(map, "id", true, Self::check_slug);
        self.field(map, "name", true, Self::check_string);
        self.field(map, "order", true, Self::check_int);
        self.field(map, "mastery_floor", false, |checker, value| {
            checker.each(value, Self::check_slug);
        });
        self.field(map, "mastery_floor_course", false, |checker, value| {
            if !value.is_null() {
                checker.check_slug(value);
            }
        });
        self.extras(map, &FIELDS);
    }

    fn check_unit(&mut self, value: &Value) {
        const FIELDS: [&str; 4] = ["unit", "course", "module", "topics"];
        let Some(map) = self.struct_map(value, "Unit") else {
            return;
        };
        self.field(map, "unit", true, Self::check_string);
        self.field(map, "course", true, Self::check_slug);
        self.field(map, "module", true, Self::check_string);
        self.field(map, "topics", false, |checker, value| {
            checker.each(value, Self::check_topic);
        });
        self.extras(map, &FIELDS);
    }

    fn check_topic(&mut self, value: &Value) {
        const FIELDS: [&str; 12] = [
            "id",
            "name",
            "core",
            "difficulty",
            "drill",
            "answer_kind",
            "expected_time_secs",
            "prerequisites",
            "encompassings_extra",
            "knowledge_points",
            "diagnostic_exemplar",
            "anki_seeds",
        ];
        let Some(map) = self.struct_map(value, "Topic") else {
            return;
        };
        self.field(map, "id", true, Self::check_slug);
        self.field(map, "name", true, Self::check_string);
        self.field(map, "core", false, Self::check_bool);
        self.field(map, "difficulty", true, Self::check_unit_interval);
        self.field(map, "drill", false, Self::check_bool);
        self.field(map, "answer_kind", true, |checker, value| {
            checker.check_enum(value, &ANSWER_KINDS);
        });
        self.field(map, "expected_time_secs", true, Self::check_positive_int);
        self.field(map, "prerequisites", false, |checker, value| {
            checker.each(value, Self::check_prereq_edge);
        });
        self.field(map, "encompassings_extra", false, |checker, value| {
            checker.each(value, Self::check_prereq_edge);
        });
        self.field(map, "knowledge_points", false, |checker, value| {
            checker.each(value, Self::check_knowledge_point);
        });
        self.field(map, "diagnostic_exemplar", false, |checker, value| {
            if !value.is_null() {
                checker.check_exemplar(value);
            }
        });
        self.field(map, "anki_seeds", false, |checker, value| {
            checker.each(value, Self::check_anki_seed);
        });
        self.extras(map, &FIELDS);
    }

    fn check_prereq_edge(&mut self, value: &Value) {
        const FIELDS: [&str; 3] = ["id", "weight", "key"];
        let Some(map) = self.struct_map(value, "PrereqEdge") else {
            return;
        };
        self.field(map, "id", true, Self::check_slug);
        self.field(map, "weight", true, Self::check_unit_interval);
        self.field(map, "key", false, Self::check_bool);
        self.extras(map, &FIELDS);
    }

    fn check_knowledge_point(&mut self, value: &Value) {
        const FIELDS: [&str; 5] = [
            "id",
            "name",
            "key_prerequisites",
            "exemplars",
            "constraints",
        ];
        let Some(map) = self.struct_map(value, "KnowledgePoint") else {
            return;
        };
        self.field(map, "id", true, Self::check_slug);
        self.field(map, "name", true, Self::check_string);
        self.field(map, "key_prerequisites", false, |checker, value| {
            checker.each(value, Self::check_slug);
        });
        self.field(map, "exemplars", false, |checker, value| {
            checker.each(value, Self::check_exemplar);
        });
        self.field(map, "constraints", false, |checker, value| {
            if !value.is_null() {
                checker.check_string(value);
            }
        });
        self.extras(map, &FIELDS);
    }

    fn check_exemplar(&mut self, value: &Value) {
        const FIELDS: [&str; 3] = ["problem", "answer", "solution_sketch"];
        let Some(map) = self.struct_map(value, "Exemplar") else {
            return;
        };
        self.field(map, "problem", true, Self::check_string);
        self.field(map, "answer", true, Self::check_string);
        self.field(map, "solution_sketch", false, |checker, value| {
            if !value.is_null() {
                checker.check_string(value);
            }
        });
        self.extras(map, &FIELDS);
    }

    fn check_anki_seed(&mut self, value: &Value) {
        const FIELDS: [&str; 3] = ["front", "back", "type"];
        let Some(map) = self.struct_map(value, "AnkiSeed") else {
            return;
        };
        self.field(map, "front", true, Self::check_string);
        self.field(map, "back", true, Self::check_string);
        self.field(map, "type", false, |checker, value| {
            checker.check_enum(value, &ANKI_TYPES);
        });
        self.extras(map, &FIELDS);
    }
}

/// The text of a mapping key, for the dotted location of an extra key.
fn key_name(key: &Value) -> String {
    match key {
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Null => "None".to_owned(),
        _ => String::new(),
    }
}

/// The float behind a YAML scalar. A YAML integer is a valid float here, the
/// same as in 1.0.
fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        _ => None,
    }
}

/// The integer behind a YAML scalar. A float with no fractional part counts.
fn as_i64(value: &Value) -> Option<i64> {
    let Value::Number(number) = value else {
        return None;
    };
    if let Some(integer) = number.as_i64() {
        return Some(integer);
    }
    let float = number.as_f64()?;
    if float.fract() == 0.0 && float >= i64::MIN as f64 && float <= i64::MAX as f64 {
        return Some(float as i64);
    }
    None
}

/// The message for a value that is not an integer.
fn int_message(value: &Value) -> &'static str {
    match value {
        Value::Number(_) => "Input should be a valid integer, got a number with a fractional part",
        Value::String(_) => "Input should be a valid integer, unable to parse string as an integer",
        _ => "Input should be a valid integer",
    }
}

/// `'a', 'b' or 'c'` — the alternatives list of a pydantic enum message.
fn quoted_alternatives(allowed: &[&str]) -> String {
    let quoted: Vec<String> = allowed.iter().map(|item| format!("'{item}'")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, head)) => format!("{} or {last}", head.join(", ")),
    }
}
