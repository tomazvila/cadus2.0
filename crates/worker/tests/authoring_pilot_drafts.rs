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
