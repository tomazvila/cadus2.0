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
mod check;
mod message;
mod numeric;

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_norway::{Mapping, Value};

use super::finding::Finding;
use super::model::{Catalog, Topic, Unit};
use check::{Checker, validate};
use message::parse_finding;
use numeric::numeric_form_findings;

/// The name of the mandatory catalog file.
const COURSES_FILE: &str = "courses.yaml";

/// The extension of a unit file. 1.0 globs `*.yaml`, so `.yml` is invisible.
const UNIT_EXTENSION: &str = ".yaml";

/// The YAML merge key. PyYAML flattens it into the mapping; 2.0 rejects it.
const MERGE_KEY: &str = "<<";

/// The message for an integer literal that no `i64` holds. Python integers have
/// no upper bound, so 1.0 reads such a literal and 2.0 refuses it.
const OUT_OF_RANGE_INTEGER: &str = "integer literal outside the 64-bit range";

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
    let catalog = match read_catalog(&courses_path) {
        Ok(catalog) => catalog,
        Err(findings) => {
            return Parsed {
                catalog: None,
                units: Vec::new(),
                findings,
                error: None,
            };
        }
    };
    let mut units = Vec::new();
    let mut findings = Vec::new();
    for course in &catalog.courses {
        read_course(root, course.id.as_str(), &mut units, &mut findings);
    }
    Parsed {
        catalog: Some(catalog),
        units,
        findings,
        error: None,
    }
}

/// Read and check `courses.yaml`. The error carries the findings that drop it.
fn read_catalog(courses_path: &Path) -> Result<Catalog, Vec<Finding>> {
    let document = read_document(courses_path, COURSES_FILE)?;
    validate(&document, COURSES_FILE, Checker::check_catalog)
}

/// Read every unit file of one course directory into `units`, and every
/// defect on the way into `findings`.
fn read_course(
    root: &Path,
    course_id: &str,
    units: &mut Vec<ParsedUnit>,
    findings: &mut Vec<Finding>,
) {
    let course_dir = root.join(course_id);
    if !course_dir.is_dir() {
        // Nothing was dropped: there was nothing to read.
        findings.push(Finding::advisory(
            "missing_course_dir",
            format!("no unit directory {course_id}/ for course"),
        ));
        return;
    }
    let entries = match unit_entries(&course_dir) {
        Ok(entries) => entries,
        Err(error) => {
            findings.push(
                Finding::new("yaml", format!("{course_id}/: {error}"))
                    .with_file(format!("{course_id}/")),
            );
            return;
        }
    };
    if entries.is_empty() {
        // An empty course omits no topics, so the finding is advisory.
        findings.push(Finding::advisory(
            "empty_course",
            format!("course {course_id} has no unit files"),
        ));
    }
    for entry in entries {
        read_unit(&course_dir, course_id, entry, units, findings);
    }
}

/// Read one unit file into `units`, or its defects into `findings`.
fn read_unit(
    course_dir: &Path,
    course_id: &str,
    entry: UnitEntry,
    units: &mut Vec<ParsedUnit>,
    findings: &mut Vec<Finding>,
) {
    let file_name = match entry {
        UnitEntry::Name(name) => name,
        UnitEntry::NotUtf8(lossy) => {
            // 1.0 `pathlib.Path.glob` decodes the name with `surrogateescape`
            // and reads the file; 2.0 holds file names as `String`, so it
            // reports the drop (spec section 7, "2.0 strictness").
            let rel = format!("{course_id}/{lossy}");
            findings.push(
                Finding::new("yaml", format!("{rel}: file name is not valid UTF-8")).with_file(rel),
            );
            return;
        }
    };
    let rel = format!("{course_id}/{file_name}");
    let document = match read_document(&course_dir.join(&file_name), &rel) {
        Ok(document) => document,
        Err(read_findings) => {
            findings.extend(read_findings);
            return;
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

/// One `*.yaml` entry of a course directory.
enum UnitEntry {
    /// The file name, which is valid UTF-8.
    Name(String),
    /// The file name in its lossy form, because the name is not valid UTF-8.
    NotUtf8(String),
}

/// The `*.yaml` entries of one course directory, in byte order.
///
/// The glob of 1.0 is `*.yaml` and it is not recursive, so a subdirectory and a
/// `.yml` file stay invisible (parity traps 2 and 3). The name test is the only
/// test: Python `pathlib.Path.glob` reads a dot-prefixed name and follows a
/// symlink, so a filter on either one drops content that 1.0 loads.
///
/// A name that is not valid UTF-8 is an entry too. 1.0 reads such a file and 2.0
/// holds every file name as a `String`, so the caller reports the drop instead
/// of skipping the entry in silence.
fn unit_entries(course_dir: &Path) -> io::Result<Vec<UnitEntry>> {
    let names = fs::read_dir(course_dir)?.map(|entry| entry.map(|entry| entry.file_name()));
    unit_entries_of(names)
}

/// The `*.yaml` entries among the directory names `names`, in byte order.
fn unit_entries_of(
    names: impl Iterator<Item = io::Result<OsString>>,
) -> io::Result<Vec<UnitEntry>> {
    let mut entries: Vec<(Vec<u8>, UnitEntry)> = Vec::new();
    for name in names {
        let name = name?;
        let bytes = name.as_encoded_bytes().to_vec();
        if !bytes.ends_with(UNIT_EXTENSION.as_bytes()) {
            continue;
        }
        let item = match name.to_str() {
            Some(text) => UnitEntry::Name(text.to_owned()),
            None => UnitEntry::NotUtf8(name.to_string_lossy().into_owned()),
        };
        entries.push((bytes, item));
    }
    // The bytes of a name sort the way 1.0 sorts the path string: UTF-8 bytes
    // sort in code-point order.
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(entries.into_iter().map(|(_, item)| item).collect())
}

/// Read one YAML file. An empty document is `{}` (spec section 1).
///
/// The error carries every finding that drops the file: one parser finding, or
/// the numeric-literal findings of [`numeric_form_findings`].
fn read_document(path: &Path, rel: &str) -> Result<Value, Vec<Finding>> {
    let text = fs::read_to_string(path)
        .map_err(|error| vec![Finding::new("yaml", format!("{rel}: {error}")).with_file(rel)])?;
    // Accept a UTF-8 BOM. An editor on Windows writes one, the Python side
    // removes it before the parser sees it, and libyaml does not (spec section 7,
    // "2.0 strictness").
    let text = text.strip_prefix('\u{feff}').unwrap_or(text.as_str());
    let value: Value = serde_norway::from_str(text)
        .map_err(|error| vec![parse_finding(rel, text, &error.to_string())])?;
    // The parsed value keeps the number, not the spelling the author wrote, so
    // the numeric-literal rule reads the text (spec section 7, "2.0 strictness").
    let numeric = numeric_form_findings(text, rel);
    if !numeric.is_empty() {
        return Err(numeric);
    }
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
#[cfg(test)]
mod tests {
    use super::*;

    /// The walk keeps the `*.yaml` names in byte order, reports a name that is
    /// not UTF-8, and fails on a directory entry the file system cannot read.
    #[test]
    fn the_directory_walk_sorts_filters_and_fails_on_an_entry_error() {
        use std::os::unix::ffi::OsStringExt;

        let names = vec![
            Ok(OsString::from("b.yaml")),
            Ok(OsString::from("notes.txt")),
            Ok(OsString::from_vec(b"a\xff.yaml".to_vec())),
            Ok(OsString::from("a.yaml")),
        ];
        let entries = unit_entries_of(names.into_iter()).unwrap();
        let shown: Vec<String> = entries
            .into_iter()
            .map(|entry| match entry {
                UnitEntry::Name(name) => name,
                UnitEntry::NotUtf8(lossy) => format!("lossy:{lossy}"),
            })
            .collect();
        assert_eq!(shown, ["a.yaml", "lossy:a\u{fffd}.yaml", "b.yaml"]);

        let names = vec![
            Ok(OsString::from("b.yaml")),
            Err(io::Error::other("entry vanished")),
        ];
        let error = unit_entries_of(names.into_iter()).err();
        assert_eq!(
            error.map(|error| error.to_string()),
            Some("entry vanished".to_owned())
        );
    }
}
