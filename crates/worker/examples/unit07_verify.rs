//! Review unit07 candidates through the production worker gate, without a DB.
use std::collections::BTreeSet;
use std::path::Path;

use cadus_core::curriculum::{Curriculum, lint_curriculum, load_curriculum};
use cadus_core::learner::problem_text_hash;
use cadus_core::template::{Bindings, Compiled, TemplateDoc, render, walk_satisfying};
use cadus_worker::authoring::cli::select;
use cadus_worker::authoring::job::verify_kind;
use cadus_worker::authoring::prompt::{AuthoringSpec, Kind};
use serde_json::{Value, json};

fn balanced(text: &str) -> Result<(), String> {
    if !text.matches('$').count().is_multiple_of(2) {
        return Err(format!("unbalanced math delimiters: {text}"));
    }
    let mut depth = 0_i64;
    for ch in text.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            ch if ch.is_control() && ch != '\n' => return Err("control character".into()),
            _ => {}
        }
        if depth < 0 {
            return Err(format!("unbalanced braces: {text}"));
        }
    }
    if depth != 0 {
        return Err(format!("unbalanced braces: {text}"));
    }
    Ok(())
}

fn authored(curriculum: &Curriculum, owned: &BTreeSet<String>) -> Result<BTreeSet<String>, String> {
    let mut texts = BTreeSet::new();
    for topic in curriculum.topics() {
        for kp in &topic.knowledge_points {
            for item in &kp.exemplars {
                texts.insert(problem_text_hash(&item.problem));
                if owned.contains(&format!("{}/{}", topic.id, kp.id)) {
                    item.canonical_answer()
                        .map_err(|err| err.reason.to_string())?;
                    balanced(&item.problem)?;
                    balanced(item.solution_sketch.as_deref().ok_or("missing sketch")?)?;
                }
            }
        }
        if let Some(item) = &topic.diagnostic_exemplar {
            texts.insert(problem_text_hash(&item.problem));
        }
    }
    Ok(texts)
}

fn inspect_instances(body: &str, known: &BTreeSet<String>) -> Result<Vec<Value>, String> {
    let doc: TemplateDoc = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let compiled = Compiled::new(&doc).map_err(|e| e.to_string())?;
    let walk = walk_satisfying(&doc.params, &doc.constraints).map_err(|e| e.to_string())?;
    if !walk.exhaustive {
        return Err("the full parameter domain was not enumerated".into());
    }
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    for bindings in walk.tuples {
        rows.push(inspect_instance(
            &doc, &compiled, bindings, known, &mut seen,
        )?);
    }
    if rows.len() < 12 {
        return Err(format!(
            "instance-capacity: {} distinct valid instances; required 12",
            rows.len()
        ));
    }
    Ok(rows)
}

fn inspect_instance(
    doc: &TemplateDoc,
    compiled: &Compiled,
    bindings: Bindings,
    known: &BTreeSet<String>,
    seen: &mut BTreeSet<String>,
) -> Result<Value, String> {
    let item = compiled
        .instantiate(bindings.clone())
        .map_err(|e| e.to_string())?;
    if known.contains(&item.instance_hash) || !seen.insert(item.instance_hash.clone()) {
        return Err(format!("authored/sibling collision: {}", item.text));
    }
    let sketch = render(
        doc.solution_sketch.as_deref().ok_or("missing sketch")?,
        &bindings,
    )
    .map_err(|e| e.to_string())?;
    let hints = doc
        .hints
        .iter()
        .map(|hint| render(hint, &bindings))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for text in [&item.text, &item.answer, &sketch]
        .into_iter()
        .chain(hints.iter())
    {
        balanced(text)?;
    }
    let params: std::collections::BTreeMap<_, _> = bindings
        .iter()
        .map(|(key, value)| (key, value.canonical_string()))
        .collect();
    Ok(
        json!({"params":params,"problem":item.text,"answer":item.answer,
        "solution_sketch":sketch,"hints":hints,"hash":item.instance_hash}),
    )
}

fn review(row: &Value, spec: &AuthoringSpec, known: &BTreeSet<String>) -> Value {
    match verify_kind(Kind::Template, spec, &row["arguments"], &[]) {
        Err(error) => json!({"kp_id":row["kp_id"],"stage":"production-worker-gate",
            "code":error.code,"message":error.message,"candidate":row,
            "inherited_contract":spec.template_contract(),
            "authored_contracts":spec.exemplars.iter().map(|item| &item.answer_contract).collect::<Vec<_>>()}),
        Ok(body) => match inspect_instances(&body, known) {
            Err(error) => json!({"kp_id":row["kp_id"],"stage":"exhaustive-instance-review",
                "code":"instance-review","message":error,"candidate":row}),
            Ok(instances) => json!({"kp_id":row["kp_id"],"stage":"passed",
                "body":body,"instances":instances}),
        },
    }
}

fn read_curriculum(root: &Path) -> Result<Curriculum, String> {
    let findings = lint_curriculum(&root.join("curriculum"));
    if !findings.is_empty() {
        return Err(format!("lint findings: {findings:?}"));
    }
    let (curriculum, findings) =
        load_curriculum(&root.join("curriculum")).map_err(|e| e.to_string())?;
    if !findings.is_empty() {
        return Err(format!("load findings: {findings:?}"));
    }
    Ok(curriculum)
}

fn owned_keys(curriculum: &Curriculum) -> BTreeSet<String> {
    curriculum
        .topics_in_course("foundations")
        .iter()
        .filter(|index| curriculum.unit_of(**index) == "polynomials-quadratics")
        .filter_map(|index| curriculum.topic(*index))
        .flat_map(|topic| {
            topic
                .knowledge_points
                .iter()
                .map(|kp| format!("{}/{}", topic.id, kp.id))
        })
        .collect()
}

fn run() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let curriculum = read_curriculum(&root)?;
    let input = std::fs::read_to_string(root.join("target/unit07/candidates.json"))
        .map_err(|e| e.to_string())?;
    let rows: Vec<Value> = serde_json::from_str(&input).map_err(|e| e.to_string())?;
    let owned = owned_keys(&curriculum);
    let mut known = authored(&curriculum, &owned)?;
    let mut keys = BTreeSet::new();
    let mut pending = Vec::new();
    let mut evidence = Vec::new();
    let mut blockers = Vec::new();
    for row in rows {
        let key = row["kp_id"].as_str().ok_or("missing key")?.to_owned();
        if !keys.insert(key.clone()) {
            return Err(format!("duplicate candidate key: {key}"));
        }
        let spec = select(&curriculum, std::slice::from_ref(&key))
            .map_err(|e| e.to_string())?
            .remove(0);
        let checked = review(&row, &spec, &known);
        if checked["stage"] == "passed" {
            let instances = checked["instances"].as_array().ok_or("missing instances")?;
            println!("PASS {key}: {} instances", instances.len());
            for instance in instances {
                known.insert(instance["hash"].as_str().ok_or("missing hash")?.to_owned());
            }
            pending.push(row);
            evidence.push(checked);
        } else {
            println!("BLOCK {key}: {}: {}", checked["code"], checked["message"]);
            blockers.push(checked);
        }
    }
    if keys != owned || owned.len() != 102 {
        return Err("candidate ownership mismatch".into());
    }
    println!(
        "TOTAL: {} pending, {} blocked",
        pending.len(),
        blockers.len()
    );
    save(
        &root,
        "docs/content-foundations/unit07/templates.json",
        &json!(pending),
    )?;
    save(
        &root,
        "docs/reports/unit07-template-evidence.json",
        &json!(evidence),
    )?;
    save(
        &root,
        "docs/reports/unit07-schema-blockers.json",
        &json!(blockers),
    )
}

fn save(root: &Path, name: &str, value: &Value) -> Result<(), String> {
    let body = serde_json::to_string_pretty(value).map_err(|e| e.to_string())? + "\n";
    std::fs::write(root.join(name), body).map_err(|e| e.to_string())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("unit07 verification refused: {error}");
        std::process::exit(1);
    }
}
