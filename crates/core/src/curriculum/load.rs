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

/// The YAML merge key. PyYAML flattens it into the mapping; 2.0 rejects it.
const MERGE_KEY: &str = "<<";

/// The message for an integer literal that no `i64` holds. Python integers have
/// no upper bound, so 1.0 reads such a literal and 2.0 refuses it.
const OUT_OF_RANGE_INTEGER: &str = "integer literal outside the 64-bit range";

/// The text `serde_norway` writes for a repeated mapping key, up to the key.
const DUPLICATE_KEY_MARKER: &str = "duplicate entry with key \"";

/// The text `serde_norway` writes for an integer literal outside `i64`/`u64`.
const BIG_INTEGER_MARKERS: [&str; 2] = [" as u128, expected", " as i128, expected"];

/// `i64::MIN` as an `f64`. The conversion is exact: the value is a power of two.
const I64_MIN_AS_F64: f64 = -9_223_372_036_854_775_808.0;

/// The first `f64` above `i64::MAX`. `i64::MAX` itself has no `f64` form, so the
/// range test is half-open and `as i64` never saturates.
const I64_MAX_EXCLUSIVE_AS_F64: f64 = 9_223_372_036_854_775_808.0;

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
/// `.yml` file stay invisible (parity traps 2 and 3). The name test is the only
/// test: Python `pathlib.Path.glob` reads a dot-prefixed name and follows a
/// symlink, so a filter on either one drops content that 1.0 loads.
fn unit_file_names(course_dir: &Path) -> Result<Vec<String>, std::io::Error> {
    let mut names = Vec::new();
    for entry in fs::read_dir(course_dir)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !name.ends_with(UNIT_EXTENSION) {
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
    // Accept a UTF-8 BOM. An editor on Windows writes one, the Python side
    // removes it before the parser sees it, and libyaml does not (spec section 7,
    // "2.0 strictness").
    let text = text.strip_prefix('\u{feff}').unwrap_or(text.as_str());
    let value: Value = serde_norway::from_str(text)
        .map_err(|error| Box::new(parse_finding(rel, &error.to_string())))?;
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
    // A residual error means the walk and the types disagree. The walk covers
    // every field of spec section 1, so this is a backstop: report it with the
    // file as the location rather than drop the file in silence.
    T::deserialize(document.clone())
        .map_err(|error| vec![Finding::new("schema", format!("{file}: {error}")).with_file(file)])
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
    ///
    /// A merge key stops the walk of that mapping. PyYAML flattens `<<` into the
    /// mapping and 2.0 does not (spec section 7, "2.0 strictness"), so every
    /// merged field looks absent. One finding on the merge key names the cause;
    /// a list of "Field required" findings names a phantom one.
    fn struct_map<'v>(&mut self, value: &'v Value, type_name: &str) -> Option<&'v Mapping> {
        match value {
            Value::Mapping(map) => {
                if map.contains_key(Value::String(MERGE_KEY.to_owned())) {
                    self.push(MERGE_KEY);
                    self.report("merge keys are not accepted; write the fields out");
                    self.pop();
                    return None;
                }
                Some(map)
            }
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
        if let Some(message) = bool_message(value) {
            self.report(&message);
        }
    }

    /// A float in the closed range 0..=1 (`difficulty` and `weight`).
    fn check_unit_interval(&mut self, value: &Value) {
        let number = match value {
            Value::Number(number) => match number.as_f64() {
                Some(number) => number,
                None => {
                    self.report("Input should be a valid number");
                    return;
                }
            },
            Value::Bool(flag) => {
                self.report(&format!("boolean {flag} is not accepted; write a number"));
                return;
            }
            Value::String(text) => {
                let message = string_number_message(text);
                self.report(&message);
                return;
            }
            _ => {
                self.report("Input should be a valid number");
                return;
            }
        };
        // 1.0 pydantic reports the upper bound for NaN and for both infinities
        // out of the two bounds it holds, so the port reports the same one.
        if number.is_nan() || number > 1.0 {
            self.report_range("Input should be less than or equal to 1");
        } else if number < 0.0 {
            self.report_range("Input should be greater than or equal to 0");
        }
    }

    fn check_int(&mut self, value: &Value) {
        if let Some(message) = int_message(value) {
            self.report(&message);
        }
    }

    fn check_positive_int(&mut self, value: &Value) {
        match int_message(value) {
            Some(message) => self.report(&message),
            None => {
                if as_i64(value).is_none_or(|number| number <= 0) {
                    self.report_range("Input should be greater than 0");
                }
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

/// The integer behind a YAML scalar. A float with no fractional part counts,
/// the same as 1.0 pydantic in lax mode (spec section 7, "2.0 strictness").
fn as_i64(value: &Value) -> Option<i64> {
    let Value::Number(number) = value else {
        return None;
    };
    if let Some(integer) = number.as_i64() {
        return Some(integer);
    }
    let float = number.as_f64()?;
    if float.fract() == 0.0 && (I64_MIN_AS_F64..I64_MAX_EXCLUSIVE_AS_F64).contains(&float) {
        return Some(float as i64);
    }
    None
}

/// The message for a value that is not an integer, or `None` when it is one.
///
/// 1.0 validates in pydantic lax mode, so it accepts values 2.0 refuses. This
/// function writes the pydantic text only where pydantic also refuses the value.
/// Every other message is 2.0's own, and it names the fix (spec section 7,
/// "2.0 strictness").
fn int_message(value: &Value) -> Option<String> {
    match value {
        Value::Number(number) => {
            if number.as_i64().is_some() {
                return None;
            }
            if number.as_u64().is_some() {
                // Above `i64::MAX`. Python integers have no upper bound.
                return Some(OUT_OF_RANGE_INTEGER.to_owned());
            }
            let Some(float) = number.as_f64() else {
                return Some("Input should be a valid integer".to_owned());
            };
            if !float.is_finite() {
                return Some("Input should be a finite number".to_owned());
            }
            if float.fract() != 0.0 {
                return Some(
                    "Input should be a valid integer, got a number with a fractional part"
                        .to_owned(),
                );
            }
            if !(I64_MIN_AS_F64..I64_MAX_EXCLUSIVE_AS_F64).contains(&float) {
                return Some(
                    "Unable to parse input string as an integer, exceeded maximum size".to_owned(),
                );
            }
            None
        }
        Value::Bool(flag) => Some(format!("boolean {flag} is not accepted; write an integer")),
        Value::String(text) => Some(match yaml_1_1_integer(text) {
            Yaml11Integer::Value(number) => {
                format!("integer {text} is not accepted; write {number}")
            }
            Yaml11Integer::OutOfRange => OUT_OF_RANGE_INTEGER.to_owned(),
            Yaml11Integer::No => match text.trim().parse::<i64>() {
                Ok(number) => format!("string '{text}' is not accepted; write {number}"),
                Err(_) => "Input should be a valid integer, unable to parse string as an integer"
                    .to_owned(),
            },
        }),
        _ => Some("Input should be a valid integer".to_owned()),
    }
}

/// The message for a value that is not a boolean, or `None` when it is one.
fn bool_message(value: &Value) -> Option<String> {
    match value {
        Value::Bool(_) => None,
        Value::String(text) => Some(if is_yaml_1_1_bool(text) {
            format!("YAML 1.1 boolean '{text}' is not accepted; write true or false")
        } else if is_lax_bool_text(text) {
            format!("string '{text}' is not accepted; write true or false")
        } else {
            "Input should be a valid boolean, unable to interpret input".to_owned()
        }),
        Value::Number(number) => Some(match number.as_f64() {
            // 1.0 pydantic reads 0 and 1 as booleans, in both the integer and
            // the float form.
            Some(float) if float == 0.0 || float == 1.0 => {
                format!("number {number} is not accepted; write true or false")
            }
            Some(float) if float.is_finite() && float.fract() == 0.0 => {
                "Input should be a valid boolean, unable to interpret input".to_owned()
            }
            _ => "Input should be a valid boolean".to_owned(),
        }),
        _ => Some("Input should be a valid boolean".to_owned()),
    }
}

/// The message for a string in a `difficulty` or `weight` field.
fn string_number_message(text: &str) -> String {
    match yaml_1_1_integer(text) {
        Yaml11Integer::Value(number) => format!("integer {text} is not accepted; write {number}"),
        Yaml11Integer::OutOfRange => OUT_OF_RANGE_INTEGER.to_owned(),
        Yaml11Integer::No => {
            if text.trim().parse::<f64>().is_ok() {
                format!("string '{text}' is not accepted; write the number unquoted")
            } else {
                "Input should be a valid number, unable to parse string as a number".to_owned()
            }
        }
    }
}

/// True for a plain scalar that YAML 1.1 reads as a boolean and YAML 1.2 reads
/// as a string. The YAML 1.1 words are `y`, `n`, `yes`, `no`, `on` and `off`, in
/// any case (`true` and `false` are booleans in both versions).
fn is_yaml_1_1_bool(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        "y" | "n" | "yes" | "no" | "on" | "off"
    )
}

/// True for a string that 1.0 pydantic reads as a boolean in lax mode.
fn is_lax_bool_text(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        "true" | "false" | "t" | "f" | "1" | "0" | "yes" | "no" | "on" | "off" | "y" | "n"
    )
}

/// What a plain scalar is worth to the YAML 1.1 integer resolver of PyYAML.
enum Yaml11Integer {
    /// A YAML 1.1 integer literal and the value 1.0 gives it.
    Value(i64),
    /// A YAML 1.1 integer literal that no `i64` holds.
    OutOfRange,
    /// Not a YAML 1.1 integer literal.
    No,
}

/// The value PyYAML gives a plain scalar that YAML 1.2 reads as a string: an
/// octal `060`, an underscore group `1_200`, or a sexagesimal `1:30`.
fn yaml_1_1_integer(text: &str) -> Yaml11Integer {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let leads = digits.starts_with(|character: char| character.is_ascii_digit());
    let value = if digits.contains(':') {
        sexagesimal_value(digits)
    } else if let Some(octal) = digits.strip_prefix('0').filter(|_| digits.len() > 1) {
        radix_value(octal, 8)
    } else if digits.contains('_') && leads {
        radix_value(digits, 10)
    } else {
        return Yaml11Integer::No;
    };
    match value {
        None => Yaml11Integer::No,
        Some(None) => Yaml11Integer::OutOfRange,
        Some(Some(value)) if negative => match value.checked_neg() {
            Some(value) => Yaml11Integer::Value(value),
            None => Yaml11Integer::OutOfRange,
        },
        Some(Some(value)) => Yaml11Integer::Value(value),
    }
}

/// The value of a digit group with `_` separators. The outer `None` means the
/// text is not a digit group of that radix; the inner one means it overflows.
fn radix_value(text: &str, radix: u32) -> Option<Option<i64>> {
    let mut value: Option<i64> = Some(0);
    let mut digits = 0_usize;
    for character in text.chars() {
        if character == '_' {
            continue;
        }
        let digit = character.to_digit(radix)?;
        digits += 1;
        value = value
            .and_then(|value| value.checked_mul(i64::from(radix)))
            .and_then(|value| value.checked_add(i64::from(digit)));
    }
    if digits == 0 {
        return None;
    }
    Some(value)
}

/// The value of a `1:30` sexagesimal group: base 60, most significant first.
///
/// The PyYAML pattern is `[1-9][0-9_]*(:[0-5]?[0-9])+`, so the first group is a
/// decimal number and every later group is one base-60 place.
fn sexagesimal_value(text: &str) -> Option<Option<i64>> {
    let mut value: Option<i64> = Some(0);
    let mut groups = 0_usize;
    for group in text.split(':') {
        let digits = if groups == 0 {
            if !group.starts_with(|character: char| ('1'..='9').contains(&character)) {
                return None;
            }
            radix_value(group, 10)?
        } else {
            let place = group.parse::<i64>().ok().filter(|place| *place < 60)?;
            if group.len() > 2 {
                return None;
            }
            Some(place)
        };
        groups += 1;
        value = value
            .and_then(|value| value.checked_mul(60))
            .and_then(|value| digits.and_then(|digits| value.checked_add(digits)));
    }
    if groups < 2 {
        return None;
    }
    Some(value)
}

/// The finding for a document the YAML parser rejects.
///
/// Two of those rejections are YAML 1.1 forms that 1.0 accepts, and the parser
/// text names neither the form nor the fix, so the port writes its own message
/// with the `schema` code (spec section 7, "2.0 strictness"). Every other parser
/// error keeps the 1.0 `yaml` code and text.
fn parse_finding(rel: &str, error: &str) -> Finding {
    if let Some((_, tail)) = error.split_once(DUPLICATE_KEY_MARKER)
        && let Some((key, tail)) = tail.split_once('"')
    {
        let line = tail
            .split_once("at line ")
            .and_then(|(_, tail)| tail.split(' ').next())
            .unwrap_or("?");
        return Finding::new(
            "schema",
            format!("{rel}: duplicate mapping key '{key}' at line {line}"),
        )
        .with_file(rel);
    }
    for marker in BIG_INTEGER_MARKERS {
        if let Some((head, _)) = error.split_once(marker) {
            let loc = head
                .split_once(": invalid type")
                .map_or(rel, |(loc, _)| loc);
            let loc = if loc.is_empty() { rel } else { loc };
            return Finding::new("schema", format!("{}: {OUT_OF_RANGE_INTEGER}", dotted(loc)))
                .with_file(rel);
        }
    }
    Finding::new("yaml", format!("{rel}: {error}")).with_file(rel)
}

/// The dotted form of the bracket path of a parser error: `topics[0].id` becomes
/// `topics.0.id`, which is the location shape of spec section 5, rule 2.
fn dotted(loc: &str) -> String {
    loc.replace('[', ".").replace(']', "")
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
