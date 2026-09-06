//! Six zero-API-cost instruction drafts pass the production gates (C6, L4, L5).
#![allow(clippy::unwrap_used)]
use cadus_core::{curriculum::load_curriculum, instruction::ServedInstance};
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::Value;

#[test]
fn arithmetic_pilot_drafts_pass_with_dense_small_number_answers() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let drafts: Vec<Value> = serde_json::from_str(include_str!(
        "../../../docs/content-pilot/arithmetic-instruction-drafts.json"
    ))
    .unwrap();
    assert_eq!(drafts.len(), 6);
    let instances: Vec<_> = (0..=20)
        .map(|answer| ServedInstance {
            problem: format!("A different practice problem with answer {answer}"),
            answer: answer.to_string(),
        })
        .collect();
    for draft in drafts {
        let key = draft["kp_id"].as_str().unwrap().to_owned();
        let specs = select(&curriculum, std::slice::from_ref(&key)).unwrap();
        let kind = Kind::from_wire(draft["kind"].as_str().unwrap()).unwrap();
        assert!(
            verify_kind(kind, &specs[0], &draft["arguments"], &instances).is_ok(),
            "{key} {kind:?}"
        );
    }
}

#[test]
fn manual_template_spaces_reserve_the_worked_examples_and_pass_all_gates() {
    use cadus_core::template::{Compiled, from_body};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, _) = load_curriculum(&root.join("curriculum")).unwrap();
    let drafts: Vec<Value> = serde_json::from_str(include_str!(
        "../../../docs/content-pilot/arithmetic-template-drafts.json"
    ))
    .unwrap();
    let instruction: Vec<Value> = serde_json::from_str(include_str!(
        "../../../docs/content-pilot/arithmetic-instruction-drafts.json"
    ))
    .unwrap();
    assert_eq!(drafts.len(), 3);
    let expected_spaces = [62, 34, 65];
    for (index, draft) in drafts.iter().enumerate() {
        let key = draft["kp_id"].as_str().unwrap().to_owned();
        let specs = select(&curriculum, std::slice::from_ref(&key)).unwrap();
        let body = verify_kind(Kind::Template, &specs[0], &draft["arguments"], &[]).unwrap();
        let doc = from_body(&body).unwrap();
        let compiled = Compiled::new(&doc).unwrap();
        let walk =
            cadus_core::template::domain::walk_satisfying(&doc.params, &doc.constraints).unwrap();
        assert_eq!(walk.tuples.len(), expected_spaces[index]);
        let instances: Vec<_> = walk
            .tuples
            .iter()
            .map(|bindings| {
                let item = compiled.instantiate(bindings.clone()).unwrap();
                ServedInstance {
                    problem: item.text,
                    answer: item.answer,
                }
            })
            .collect();
        for instruction in instruction.iter().filter(|row| row["kp_id"] == key) {
            let kind = Kind::from_wire(instruction["kind"].as_str().unwrap()).unwrap();
            verify_kind(kind, &specs[0], &instruction["arguments"], &instances).unwrap();
        }
    }
}
