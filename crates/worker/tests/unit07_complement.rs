//! Exhaustive source-only worker-gate evidence for the five U07 residuals.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::{collections::BTreeSet, fs, path::PathBuf};

use cadus_core::{
    answer::{Outcome, check_contract},
    curriculum::{load_curriculum, FiniteCaseRole},
    learner::problem_text_hash,
    template::{Compiled, from_body, render, walk_satisfying},
};
use cadus_worker::authoring::{
    cli::select,
    job::{document_digest, verify_kind},
    prompt::Kind,
};
use serde_json::{Value, json};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rows() -> Vec<Value> {
    serde_json::from_str(
        &fs::read_to_string(
            root().join("docs/content-foundations/unit07-complement/templates.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn exhaustive_production_gate_and_negative_controls() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let mut seen = BTreeSet::new();
    for topic in curriculum.topics() {
        for kp in &topic.knowledge_points {
            for ex in &kp.exemplars {
                seen.insert(problem_text_hash(&ex.problem));
            }
        }
        if let Some(ex) = &topic.diagnostic_exemplar {
            seen.insert(problem_text_hash(&ex.problem));
        }
    }
    let mut evidence = Vec::new();
    assert_eq!(rows().len(), 5);
    for row in rows() {
        let key = row["kp_id"].as_str().unwrap();
        assert_eq!(row["status"], "pending");
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        assert_eq!(spec.exemplars.len(), 4);
        for ex in &spec.exemplars {
            ex.canonical_answer().unwrap();
        }
        let body = verify_kind(Kind::Template, &spec, &row["arguments"], &[])
            .unwrap_or_else(|e| panic!("{key}: {e:?}"));
        let instances = inspect(key, &body, &mut seen, &spec);
        negative_controls(&row, &spec);
        evidence.push(json!({"kp_id":key,"kind":"template","status":"pending",
            "stage":"production-worker-gate-passed", "digest":document_digest(key,Kind::Template,&body),
            "body":serde_json::from_str::<Value>(&body).unwrap(),"instances":instances,
            "finite_policy":spec.finite.as_ref().map(|finite| &finite.domain),
            "finite_policy_fingerprint":spec.finite.as_ref().map(|finite| &finite.fingerprint)}));
    }
    let output = root().join("target/unit07-complement");
    fs::create_dir_all(&output).unwrap();
    fs::write(
        output.join("production-evidence.json"),
        serde_json::to_string_pretty(&evidence).unwrap() + "\n",
    )
    .unwrap();
}

fn reviewed_domain(key: &str) -> Value {
    // Independently accepted domain snapshots; recipe shrinkage cannot lower the expected count.
    let domains: Value = serde_json::from_str(r#"{"polynomial-basics/kp2":{"params":{"a":{"kind":"choice","values":[0,2]},"b":{"kind":"choice","values":[0,3]},"c":{"kind":"choice","values":[0,5]},"d":{"kind":"choice","values":[0,-9]},"f":{"kind":"choice","values":["monomial"]},"g":{"kind":"choice","values":["binomial"]},"h":{"kind":"choice","values":["trinomial"]}},"constraints":[{"op":"ge","left":{"add":[{"mul":[{"lit":"1/2"},"a"]},{"mul":[{"lit":"1/3"},"b"]},{"mul":[{"lit":"1/5"},"c"]},{"mul":[{"lit":"-1/9"},"d"]}]},"right":{"lit":1}},{"op":"le","left":{"add":[{"mul":[{"lit":"1/2"},"a"]},{"mul":[{"lit":"1/3"},"b"]},{"mul":[{"lit":"1/5"},"c"]},{"mul":[{"lit":"-1/9"},"d"]}]},"right":{"lit":3}}],"count":14},"difference-of-squares/kp1":{"params":{"a":{"kind":"choice","values":[1,4,9,16,25,49,64,81,100]}},"constraints":[],"count":9},"choosing-factoring-strategy/kp1":{"params":{"g":{"kind":"choice","values":[1,2]},"b":{"kind":"choice","values":[0,48,80,120]},"c":{"kind":"choice","values":[49,81,121]},"f":{"kind":"choice","values":["GCF"]},"s":{"kind":"choice","values":["difference of squares"]},"h":{"kind":"choice","values":["trinomial factoring"]}},"constraints":[{"op":"eq","left":{"mul":["b",{"add":["b",{"mul":[{"lit":-1},"c"]},{"lit":1}]}]},"right":{"lit":0}}],"count":12},"parabola-vertex-form/kp2":{"params":{"a":{"kind":"choice","values":[-2,3]},"k":{"kind":"choice","values":[-8,-5,-2,7,10,12]},"d":{"kind":"choice","values":["downward"]},"u":{"kind":"choice","values":["upward"]}},"constraints":[],"count":12},"quadratic-graphs-vertex/kp3":{"params":{"a":{"kind":"choice","values":[-1,1]},"l":{"kind":"choice","values":[-8,-6]},"r":{"kind":"choice","values":[4,8,12]},"d":{"kind":"choice","values":["downward"]},"u":{"kind":"choice","values":["upward"]}},"constraints":[],"count":12}}"#).unwrap();
    domains.get(key).unwrap_or_else(|| panic!("unreviewed key: {key}")).clone()
}

fn inspect(key: &str, body: &str, seen: &mut BTreeSet<String>, spec: &cadus_worker::authoring::prompt::AuthoringSpec) -> Vec<Value> {
    let doc = from_body(body).unwrap();
    let compiled = Compiled::new(&doc).unwrap();
    let walk = walk_satisfying(&doc.params, &doc.constraints).unwrap();
    assert!(walk.exhaustive);
    let expected = reviewed_domain(key);
    assert_eq!(serde_json::to_value(&doc.params).unwrap(), expected["params"]);
    assert_eq!(serde_json::to_value(&doc.constraints).unwrap(), expected["constraints"]);
    let capacity = expected["count"].as_u64().unwrap() as usize;
    if key == "difference-of-squares/kp1" {
        let finite = spec.finite.as_ref().expect("reviewed finite square domain");
        finite.validate(key).unwrap();
        assert_eq!(finite.domain.cases.len(), 10);
        assert_eq!(finite.domain.cases.iter().filter(|case| case.role == FiniteCaseRole::PracticeFresh).count(), 9);
        assert_eq!(finite.domain.cases.iter().filter(|case| case.role == FiniteCaseRole::TeachOnly).count(), 1);
        let worked = "Factor $x^2-36$. Report lower_factor and upper_factor in increasing constant order.";
        let taught = finite.domain.cases.iter().find(|case| case.variants.iter().any(|variant| variant.problem == worked)).expect("reserved worked square36");
        assert_eq!(taught.role, FiniteCaseRole::TeachOnly);
    } else {
        assert!(spec.finite.is_none(), "new finite objective needs an explicit regression review");
    }
    let mut practice_hashes = BTreeSet::new();
    let mut practice_cases = BTreeSet::new();
    assert_eq!(walk.tuples.len(), capacity);
    assert_eq!(doc.samples.len(), capacity);
    let contract = doc.answer_contract.clone().unwrap();
    let mut rows = Vec::new();
    let mut answers = BTreeSet::new();
    for binding in walk.tuples {
        let sample = doc
            .samples
            .iter()
            .find(|s| s.bindings() == binding)
            .unwrap();
        let item = compiled.instantiate(binding.clone()).unwrap();
        assert!(practice_hashes.insert(item.instance_hash.clone()), "duplicate practice instance");
        let finite_case = spec.finite.as_ref().map(|finite| {
            let case = finite.domain.case_for(&item.text, &item.answer, item.answer_contract.as_ref()).expect("native registered finite variant");
            assert_eq!(case.role, FiniteCaseRole::PracticeFresh);
            assert!(practice_cases.insert(case.id.as_str().to_owned()), "duplicate semantic practice case");
            case
        });
        if !seen.insert(item.instance_hash.clone()) {
            assert!(finite_case.is_some(), "unregistered collision: {}", item.text);
        }
        let outcome = check_contract(&sample.expected.text(), &item.answer, contract.clone());
        assert!(matches!(outcome, Outcome::Decided(r) if r.correct));
        for wrong in wrong_answers(&item.answer) {
            assert!(
                !matches!(check_contract(&item.answer, &wrong, contract.clone()),
                Outcome::Decided(r) if r.correct),
                "wrong accepted: {wrong}"
            );
        }
        answers.insert(item.canon.clone());
        rows.push(
            json!({"problem":item.text,"answer":item.answer,"hash":item.instance_hash,
            "solution_sketch":render(doc.solution_sketch.as_ref().unwrap(), &binding).unwrap(),
            "hints":doc.hints.iter().map(|h| render(h,&binding).unwrap()).collect::<Vec<_>>(),
            "native_finite_case":finite_case.map(|case| json!({"id":case.id.as_str(),"role":case.role}))}),
        );
    }
    assert!(answers.len() >= 3, "constant or low-entropy family");
    rows
}

fn wrong_answers(answer: &str) -> Vec<String> {
    if !answer.contains(';') {
        return [
            "monomial",
            "binomial",
            "trinomial",
            "GCF",
            "difference of squares",
            "trinomial factoring",
            "unknown",
        ]
        .into_iter()
        .filter(|s| *s != answer)
        .map(str::to_owned)
        .collect();
    }
    let fields: Vec<_> = answer.split("; ").collect();
    let mut wrong = vec![fields[1..].join("; "), format!("{answer}; extra = 0")];
    for (index, field) in fields.iter().enumerate() {
        let (name, value) = field.split_once(" = ").unwrap();
        let replacement = if value == "upward" {
            "downward".into()
        } else if value == "downward" {
            "upward".into()
        } else if value.starts_with('(') {
            "(999, 999)".into()
        } else {
            format!("({value})+1")
        };
        let mut parts = fields.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        parts[index] = format!("{name} = {replacement}");
        wrong.push(parts.join("; "));
    }
    wrong
}

fn negative_controls(row: &Value, spec: &cadus_worker::authoring::prompt::AuthoringSpec) {
    for expr in ["0", "multipart(0,0)", "signcase(0,[0,0])", "a-a"] {
        let mut args = row["arguments"].clone();
        args["answer_expr"] = json!(expr);
        assert!(verify_kind(Kind::Template, spec, &args, &[]).is_err());
    }
    let mut args = row["arguments"].clone();
    args["samples"][0]["expected"] = json!("incorrect label = 999");
    assert!(verify_kind(Kind::Template, spec, &args, &[]).is_err());
}
