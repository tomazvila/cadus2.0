//! Check every served problem against authored content and sibling templates.
use cadus_core::{
    curriculum::Curriculum,
    template::{Compiled, GateSpec, TemplateDoc, render},
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

pub fn check(
    doc: &TemplateDoc,
    occupied: &BTreeSet<String>,
    siblings: &BTreeSet<String>,
    gate_spec: &GateSpec<'_>,
) -> Result<(BTreeSet<String>, Vec<Value>), String> {
    let compiled = Compiled::new(doc).map_err(|e| format!("{e:?}"))?;
    let mut problems = BTreeSet::new();
    let mut instances = Vec::new();
    for sample in &doc.samples {
        let item = compiled
            .instantiate(sample.bindings())
            .map_err(|e| format!("{e:?}"))?;
        if siblings.contains(&item.instance_hash) || problems.contains(&item.instance_hash) {
            return Err(format!("authored/sibling collision: {}", item.text));
        }
        if occupied.contains(&item.instance_hash) {
            let Some(finite) = &gate_spec.finite else {
                return Err(format!("authored/sibling collision: {}", item.text));
            };
            // Only an exact reviewed practice variant may overlap authored history.
            // The native matcher excludes teach-only and reserved-assessment roles.
            finite
                .match_practice_instance(&item)
                .map_err(|error| format!("{}: {}", error.code, error.message))?;
        }
        if !balanced(&item.text) {
            return Err(format!("unbalanced statement: {}", item.text));
        }
        for field in doc.solution_sketch.iter().chain(doc.hints.iter()) {
            let rendered = render(field, &item.bindings).map_err(|e| format!("{e:?}"))?;
            if !balanced(&rendered) {
                return Err(format!("unbalanced field: {rendered}"));
            }
        }
        problems.insert(item.instance_hash);
        instances.push(json!({"params":sample.params,"problem":item.text,"answer":item.answer}));
    }
    Ok((problems, instances))
}

pub fn balanced(text: &str) -> bool {
    let mut dollars = 0;
    let mut previous = ' ';
    for character in text.chars() {
        if character == '$' && previous != '\\' {
            dollars += 1;
        }
        previous = character;
    }
    dollars % 2 == 0
        && text.matches("\\(").count() == text.matches("\\)").count()
        && text.matches("\\[").count() == text.matches("\\]").count()
}

pub fn authored(curriculum: &Curriculum, content: &Path) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for topic in curriculum.topics() {
        let exemplars = topic
            .knowledge_points
            .iter()
            .flat_map(|k| &k.exemplars)
            .chain(topic.diagnostic_exemplar.iter());
        result.extend(exemplars.map(|e| cadus_core::learner::problem_text_hash(&e.problem)));
    }
    authored_documents(content, &mut result);
    result
}

fn authored_documents(path: &Path, result: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            authored_documents(&path, result);
        } else if path.extension().is_some_and(|e| e == "json") {
            let value: Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            collect_problems(&value, result);
        }
    }
}

fn collect_problems(value: &Value, result: &mut BTreeSet<String>) {
    match value {
        Value::Array(rows) => {
            for row in rows {
                collect_problems(row, result);
            }
        }
        Value::Object(fields) => {
            if let Some(problem) = fields.get("problem").and_then(Value::as_str) {
                result.insert(cadus_core::learner::problem_text_hash(problem));
            }
            for child in fields.values() {
                collect_problems(child, result);
            }
        }
        _ => {}
    }
}
