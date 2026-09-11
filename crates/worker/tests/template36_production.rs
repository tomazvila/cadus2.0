//! Exhaustive production verification and whole-corpus collision inventory.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use cadus_core::{
    curriculum::load_curriculum,
    learner::problem_text_hash,
    template::{
        TemplateDoc,
        constraint::all_hold,
        domain::{Params, enumerate},
        render,
    },
};
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

fn read(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn collect(value: &Value, rows: &mut Vec<Value>) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| collect(item, rows)),
        Value::Object(fields) => {
            if value["kind"] == "template"
                && value["kp_id"].is_string()
                && value.get("status").is_none_or(|s| s == "pending")
            {
                rows.push(value.clone());
            }
            fields.values().for_each(|value| collect(value, rows));
        }
        _ => {}
    }
}

fn documents(path: &Path, rows: &mut Vec<Value>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            documents(&path, rows);
        } else if path.extension().is_some_and(|x| x == "json") {
            collect(&read(&path), rows);
        }
    }
}

fn problems(row: &Value) -> Vec<String> {
    let args = row.get("arguments").or_else(|| row.get("body")).unwrap();
    if let Some(problem) = args["problem"].as_str() {
        return vec![problem.to_owned()];
    }
    let params: Params = serde_json::from_value(args["params"].clone()).unwrap();
    let constraints: Vec<cadus_core::template::Constraint> =
        serde_json::from_value(args.get("constraints").cloned().unwrap_or(json!([]))).unwrap();
    enumerate(&params, 1_000_000)
        .unwrap()
        .into_iter()
        .filter(|b| all_hold(&constraints, b).unwrap())
        .map(|b| render(args["statement"].as_str().unwrap(), &b).unwrap())
        .collect()
}

fn candidates(root: &Path) -> Vec<Value> {
    ["expressions", "inequalities", "graphs"]
        .into_iter()
        .flat_map(|name| {
            read(&root.join(format!("docs/content-foundations/template36/{name}.json")))
                .as_array()
                .unwrap()
                .clone()
        })
        .collect()
}

#[test]
fn exact_36_keys_pass_worker_gate_exhaustively() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let rows = candidates(&root);
    let expected: BTreeSet<_> = read(&root.join("docs/content-foundations/template36/keys.json"))
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(rows.len(), 36);
    assert_eq!(
        rows.iter()
            .map(|r| r["kp_id"].as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>(),
        expected
    );
    let mut counts = BTreeMap::new();
    let mut errors = Vec::new();
    for row in rows {
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        let body = match verify_kind(Kind::Template, &spec, &row["arguments"], &[]) {
            Ok(body) => body,
            Err(error) => {
                errors.push(format!("{key}: {error}"));
                continue;
            }
        };
        let doc: TemplateDoc = serde_json::from_str(&body).unwrap();
        let domain = enumerate(&doc.params, 10_000).unwrap();
        assert!(
            domain
                .iter()
                .all(|b| all_hold(&doc.constraints, b).unwrap())
        );
        let samples: BTreeSet<_> = doc.samples.iter().map(|s| s.bindings()).collect();
        assert_eq!(
            samples,
            domain.iter().cloned().collect(),
            "{key}: samples not exhaustive"
        );
        let rendered = problems(&row);
        assert!(rendered.len() >= 12, "{key}");
        assert_eq!(
            rendered.iter().collect::<BTreeSet<_>>().len(),
            rendered.len(),
            "{key}"
        );
        counts.insert(key.to_owned(), rendered.len());
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    println!(
        "production keys={} exhaustive instances={} counts={counts:?}",
        counts.len(),
        counts.values().sum::<usize>()
    );
}

#[test]
fn all_pending_domains_and_authored_exemplars_have_no_exact_collisions_with_slice() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rows = candidates(&root);
    let own: BTreeSet<_> = rows.iter().map(|r| r.to_string()).collect();
    let (curriculum, _) = load_curriculum(&root.join("curriculum")).unwrap();
    let mut corpus = Vec::new();
    let mut hashes = BTreeSet::new();
    for spec in select(&curriculum, &[]).unwrap() {
        for item in spec.exemplars {
            hashes.insert(problem_text_hash(&item.problem));
            corpus.push(json!({"kp_id":format!("{}/{}",spec.topic_id,spec.kp_id),"problem":item.problem,"source":"authored"}));
        }
    }
    let mut all = Vec::new();
    documents(&root.join("docs/content-foundations"), &mut all);
    let mut seen = BTreeSet::new();
    for row in all {
        if own.contains(&row.to_string()) || !seen.insert(row.to_string()) {
            continue;
        }
        for problem in problems(&row) {
            hashes.insert(problem_text_hash(&problem));
            corpus.push(json!({"kp_id":row["kp_id"],"problem":problem,"source":"pending"}));
        }
    }
    let mut count = 0;
    for row in rows {
        for problem in problems(&row) {
            assert!(
                hashes.insert(problem_text_hash(&problem)),
                "{}: collision {problem}",
                row["kp_id"]
            );
            count += 1;
        }
    }
    std::fs::create_dir_all(root.join("target")).unwrap();
    std::fs::write(
        root.join("target/template36-corpus.json"),
        serde_json::to_vec(&corpus).unwrap(),
    )
    .unwrap();
    println!(
        "exact collisions=0 candidate instances={count} corpus instances={} pending recipes={}",
        corpus.len(),
        seen.len()
    );
}
