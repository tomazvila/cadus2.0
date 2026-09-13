//! Source-bound native evidence for current Unit06 recipes; no AI or storage approval.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use cadus_core::{
    answer::{AnswerContract, Outcome},
    curriculum,
    learner::problem_text_hash,
    template,
};
use cadus_worker::authoring::{cli, job, prompt};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};

pub const CANDIDATES: &str = "crates/worker/tests/fixtures/unit06-template-candidates.json";
pub const RECEIPT: &str = "docs/content-foundations/unit06-correction/current-technical.json";
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
pub fn source_matches(receipt: &Value, expected: &Value) -> Result<(), &'static str> {
    if &receipt["sources"] == expected {
        Ok(())
    } else {
        Err("stale Unit06 technical receipt source binding")
    }
}

/// Change a mathematical value while retaining the declared response structure.
pub fn wrong_answer(answer: &str, contract: &AnswerContract) -> String {
    let wrong = if contract == &AnswerContract::RequiredSinglePower {
        let cadus_core::answer::Ast::Pow(base, exponent) =
            cadus_core::answer::parse(answer).unwrap()
        else {
            panic!("a native single-power answer must contain a literal base and integer exponent");
        };
        let next_exponent = exponent.checked_add(1).expect("bounded native exponent");
        let changed = format!("({})^({next_exponent})", template::write(&base).unwrap());
        assert!(
            matches!(cadus_core::answer::check_contract(answer, &changed, AnswerContract::Exact),
            Outcome::Decided(verdict) if !verdict.correct),
            "single-power control must change the mathematical value"
        );
        changed
    } else if let AnswerContract::Label { options } = contract {
        // Select a different declared meaning independently of the checker under test.
        let expected_groups: Vec<usize> = options
            .iter()
            .enumerate()
            .filter(|(_, aliases)| {
                aliases
                    .iter()
                    .any(|alias| alias.trim().eq_ignore_ascii_case(answer.trim()))
            })
            .map(|(index, _)| index)
            .collect();
        assert_eq!(
            expected_groups.len(),
            1,
            "expected label must name one declared synonym group"
        );
        let expected_group = expected_groups[0];
        let (wrong_group, wrong_aliases) = options
            .iter()
            .enumerate()
            .find(|(index, aliases)| *index != expected_group && !aliases.is_empty())
            .expect("a current label control needs another declared synonym group");
        assert_ne!(wrong_group, expected_group);
        let candidate = wrong_aliases[0].clone();
        assert!(
            !options[expected_group]
                .iter()
                .any(|alias| alias.trim().eq_ignore_ascii_case(candidate.trim())),
            "negative label must not be an alias of the expected meaning"
        );
        candidate
    } else if matches!(contract, AnswerContract::Multipart { .. }) {
        let mut fields: Vec<String> = answer.split(';').map(|x| x.trim().to_owned()).collect();
        let (name, value) = fields[0].split_once('=').expect("named multipart field");
        fields[0] = format!("{} = ({})+1", name.trim(), value.trim());
        fields.join("; ")
    } else {
        format!("({answer})+1")
    };
    contract
        .validate_expected(&wrong)
        .expect("negative control must retain the response type");
    assert!(
        matches!(cadus_core::answer::check_contract(answer, &wrong, contract.clone()),
        Outcome::Decided(verdict) if !verdict.correct),
        "negative control must be decidably wrong"
    );
    wrong
}

pub fn current_receipt(root: &Path) -> Value {
    let bytes = fs::read(root.join(CANDIDATES)).unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 78);
    let (curriculum, findings) = curriculum::load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let keys: Vec<String> = rows
        .iter()
        .map(|r| r["kp_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(keys.iter().collect::<BTreeSet<_>>().len(), 78);
    let specs = cli::select(&curriculum, &keys).unwrap();
    let mut authored = BTreeSet::new();
    for spec in &specs {
        for item in &spec.exemplars {
            authored.insert(problem_text_hash(item.problem.trim()));
        }
    }
    for topic in curriculum.topics() {
        if specs.iter().any(|s| s.topic_id == topic.id.as_str()) {
            if let Some(item) = &topic.diagnostic_exemplar {
                authored.insert(problem_text_hash(item.problem.trim()));
            }
        }
    }
    let mut siblings = BTreeSet::new();
    let mut evidence = Vec::new();
    for (row, spec) in rows.iter().zip(&specs) {
        let key = row["kp_id"].as_str().unwrap();
        assert_eq!(key, spec.kp_key());
        let body = job::verify_kind(prompt::Kind::Template, spec, &row["arguments"], &[])
            .unwrap_or_else(|reason| panic!("{key}: {reason:?}"));
        let doc = template::from_body(&body).unwrap();
        if key == "estimating-square-roots/kp3" {
            assert_eq!(
                doc.samples
                    .iter()
                    .map(|s| s.expected.text())
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from(["1".to_owned(), "2".to_owned()]),
                "comparison family must exercise both orderings"
            );
        }
        let compiled = template::Compiled::new(&doc).unwrap();
        let walk = template::walk_satisfying(&doc.params, &doc.constraints).unwrap();
        assert!(walk.exhaustive, "{key}: complete current coverage required");
        let contract = doc.answer_contract.clone().unwrap_or(AnswerContract::Exact);
        let mut instances = Vec::new();
        for tuple in walk.tuples {
            let instance = compiled.instantiate(tuple).unwrap();
            let hash = problem_text_hash(instance.text.trim());
            assert!(
                siblings.insert(hash.clone()),
                "{key}: repeated sibling problem"
            );
            let finite_case = spec.finite.as_ref().and_then(|policy| {
                policy.domain.case_for(
                    &instance.text,
                    &instance.answer,
                    instance.answer_contract.as_ref(),
                )
            });
            if authored.contains(&hash) {
                assert!(
                    finite_case.is_some_and(
                        |case| case.role == curriculum::FiniteCaseRole::TaughtRehearsal
                    ),
                    "{key}: authored collision without reviewed rehearsal role"
                );
            }
            let wrong = wrong_answer(&instance.answer, &contract);
            instances.push(json!({"params": instance.bindings.iter().map(|(k,v)| (k.clone(),v.canonical_string())).collect::<std::collections::BTreeMap<_,_>>(),
                "problem":instance.text,"answer":instance.answer,"answer_contract":instance.answer_contract,
                "solution_sketch":template::render(doc.solution_sketch.as_ref().unwrap(), &instance.bindings).unwrap(),
                "finite_case":finite_case.map(|case| json!({"id":case.id.as_str(),"role":case.role})),
                "typed_wrong_answer":wrong,"negative_control_decided_incorrect":true}));
        }
        assert!(!instances.is_empty());
        let actual = compiled.instantiate(doc.samples[0].bindings()).unwrap();
        let wrong = wrong_answer(&actual.answer, &contract);
        let mut corrupted = row["arguments"].clone();
        corrupted["samples"][0]["expected"] = json!(wrong);
        let refusal = job::verify_kind(prompt::Kind::Template, spec, &corrupted, &[])
            .expect_err("corrupted current oracle accepted");
        assert_eq!(refusal.code, "sample-agreement", "{key}: {refusal:?}");
        evidence.push(
            json!({"kp_id":key,"kind":"template","status":"technical_pass",
            "arguments_sha256":hash_bytes(row["arguments"].to_string().as_bytes()),
            "digest":job::document_digest(key,prompt::Kind::Template,&body),
            "body":serde_json::from_str::<Value>(&body).unwrap(),"instances":instances,
            "exhaustive":true,"valid_distinct_instances":instances.len(),
            "negative_control":{"code":refusal.code,"wrong_sample":wrong,"type_valid":true}}),
        );
    }
    json!({"schema_version":1,"artifact_kind":"native_technical_receipt","ai_review":"not_performed",
        "storage":"offline; no database rows created; no content approval",
        "sources":{"candidate_sha256":hash_bytes(&bytes),"curriculum_hash":curriculum::curriculum_hash(&curriculum),
            "canonical_curriculum_digest":curriculum::review_context_digest(&curriculum).unwrap(),
            "gate_source_hash":env!("CADUS_TEMPLATE_GATE_SOURCE_HASH"),"review_engine_digest":cadus_core::review_engine::DIGEST,
            "generator_sha256":hash_bytes(&fs::read(root.join("crates/worker/examples/unit06_templates.rs")).unwrap()),
            "evidence_logic_sha256":hash_bytes(&fs::read(root.join("crates/worker/tests/support/unit06_current_evidence.rs")).unwrap())},
        "rows":evidence,"valid_distinct_instances":siblings.len()})
}
