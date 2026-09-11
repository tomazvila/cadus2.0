//! The schema walk of one document (spec section 1).

use serde::Deserialize;
use serde_norway::{Mapping, Value};

use super::super::finding::Finding;
use super::super::model::{ANKI_TYPES, ANSWER_KINDS};
use super::MERGE_KEY;
use super::message::{
    bool_message, integer_of, key_name, quoted_alternatives, string_number_message,
};

/// Check one document against the schema, then build the typed value.
///
/// `check` walks the document and collects the findings. When it finds nothing,
/// `serde` builds the value; `deny_unknown_fields` and the type rules of
/// [`super::model`] are the backstop behind the walk.
pub(super) fn validate<'f, T, F>(
    document: &Value,
    file: &'f str,
    check: F,
) -> Result<T, Vec<Finding>>
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
pub(super) struct Checker<'a> {
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
    fn field(&mut self, map: &Mapping, key: &str, required: bool, visit: fn(&mut Self, &Value)) {
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
    fn each(&mut self, value: &Value, visit: fn(&mut Self, &Value)) {
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
            // A YAML number always reads as an `f64`, so the fallback never fires.
            Value::Number(number) => number.as_f64().unwrap_or(f64::NAN),
            Value::Bool(flag) => {
                self.report(&format!("boolean {flag} is not accepted; write a number"));
                return;
            }
            Value::String(text) => {
                // A decimal literal that overflows `f64` arrives as a string.
                // 1.0 reads the infinity and reports the range, so the range
                // check below reports it too.
                match text.trim().parse::<f64>() {
                    Ok(number) if !number.is_finite() => number,
                    _ => {
                        let message = string_number_message(text);
                        self.report(&message);
                        return;
                    }
                }
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
        if let Err(message) = integer_of(value) {
            self.report(&message);
        }
    }

    fn check_positive_int(&mut self, value: &Value) {
        match integer_of(value) {
            Err(message) => self.report(&message),
            Ok(number) if number <= 0 => self.report_range("Input should be greater than 0"),
            Ok(_) => {}
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
    pub(super) fn check_catalog(&mut self, value: &Value) {
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
        const FIELDS: [&str; 6] = [
            "visuals",
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

    pub(super) fn check_unit(&mut self, value: &Value) {
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
        const FIELDS: [&str; 7] = [
            "visuals",
            "id",
            "name",
            "key_prerequisites",
            "exemplars",
            "constraints",
            "finite_objective_domain",
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
        self.field(map, "visuals", false, |checker, value| {
            checker.each(value, |checker, visual| {
                if let Err(error) = crate::visual::VisualSpec::deserialize(visual.clone()) {
                    checker.report(&error.to_string());
                }
            });
        });
        self.field(map, "constraints", false, |checker, value| {
            if !value.is_null() {
                checker.check_string(value);
            }
        });
        self.field(map, "finite_objective_domain", false, |checker, value| {
            match crate::curriculum::FiniteObjectiveDomain::deserialize(value.clone()) {
                Ok(_) => {}
                Err(reason) => checker.report(&reason.to_string()),
            }
        });
        if let Ok(point) = crate::curriculum::KnowledgePoint::deserialize(value.clone())
            && let Err(reason) = point.validate_finite_objective_domain()
        {
            self.report(&reason);
        }
        self.extras(map, &FIELDS);
    }

    fn check_exemplar(&mut self, value: &Value) {
        const FIELDS: [&str; 4] = ["problem", "answer", "solution_sketch", "answer_contract"];
        let Some(map) = self.struct_map(value, "Exemplar") else {
            return;
        };
        self.field(map, "problem", true, Self::check_string);
        self.field(map, "answer", true, Self::check_string);
        self.field(map, "answer_contract", false, |checker, value| {
            if value.is_null() {
                return;
            }
            match crate::answer::AnswerContract::deserialize(value.clone()) {
                Ok(contract) => {
                    if let Err(reason) = contract.validate() {
                        checker.report(reason.reason);
                    }
                }
                Err(reason) => checker.report(&reason.to_string()),
            }
        });
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
#[cfg(test)]
mod tests {
    use super::super::super::model::Course;
    use super::*;

    /// The typed build behind the walk is a backstop: when the walk finds
    /// nothing and the types still refuse the document, the finding names the
    /// file as the location.
    #[test]
    fn a_document_the_types_refuse_after_a_silent_walk_is_a_schema_finding() {
        let document: Value = serde_norway::from_str("id: c\nname: C\norder: []\n").unwrap();
        let findings = validate::<Course, _>(&document, "courses.yaml", |_, _| {})
            .err()
            .unwrap_or_default();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "schema");
        assert_eq!(findings[0].file.as_deref(), Some("courses.yaml"));
        assert_eq!(
            findings[0].message,
            "courses.yaml: invalid type: sequence, expected an integer, or a float with no \
             fractional part"
        );
    }
}

#[cfg(test)]
mod finite_tests;
