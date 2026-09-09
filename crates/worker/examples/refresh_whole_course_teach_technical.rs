//! Writes current, non-semantic whole-course Teach technical evidence.
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};

use cadus_core::{curriculum::load_curriculum, instruction::template_instances};
use cadus_worker::authoring::{cli::{AuthorArgs, select_for}, job::{document_digest, verify_kind}, prompt::{AuthoringSpec, Kind}};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn read(path: &Path) -> Value { serde_json::from_slice(&fs::read(path).expect("read")).expect("json") }
fn hash(bytes: &[u8]) -> String { format!("sha256:{:x}", Sha256::digest(bytes)) }
fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") }

fn main() {
    let output = std::env::args_os().nth(1).expect("usage: refresh_whole_course_teach_technical <output>");
    let root = root();
    let directory = root.join("docs/content-foundations/whole-course-teach");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).expect("curriculum");
    assert!(findings.is_empty(), "curriculum findings: {findings:?}");
    let specs: BTreeMap<String, AuthoringSpec> = select_for(&curriculum, &AuthorArgs { course: Some("foundations".into()), ..AuthorArgs::default() }).expect("specs").into_iter().map(|s| (format!("{}/{}", s.topic_id, s.kp_id), s)).collect();
    let templates: BTreeMap<String, Value> = read(&directory.join("inputs/templates.json")).as_array().expect("templates").iter().map(|r| (r["kp_id"].as_str().unwrap().to_owned(), r.clone())).collect();
    let mut rows = Vec::new();
    for part in 1..=30 {
        for draft in read(&directory.join(format!("drafts/part-{part:02}.json"))).as_array().expect("drafts") {
            let kp = draft["kp_id"].as_str().expect("kp"); let spec = &specs[kp]; let template = &templates[kp];
            let template_body = verify_kind(Kind::Template, spec, &template["arguments"], &[]).unwrap_or_else(|e| panic!("{kp}: {e}"));
            let served = template_instances(&template_body);
            let teach_body = verify_kind(Kind::Teach, spec, &draft["arguments"], &served).unwrap_or_else(|e| panic!("{kp}: {e}"));
            rows.push(json!({"kp_id":kp,"teach_digest":document_digest(kp, Kind::Teach, &teach_body),"template_digest":document_digest(kp, Kind::Template, &template_body),"collision":"clear","production_gate":"accepted","ai_review":"pending"}));
        }
    }
    rows.sort_by(|a,b| a["kp_id"].as_str().cmp(&b["kp_id"].as_str()));
    assert_eq!(rows.len(), 735); let value=json!({"schema_version":2,"status":"pending-ai-review","rows":rows});
    let bytes=serde_json::to_vec_pretty(&value).expect("serialize");
    assert_eq!(serde_json::from_slice::<Value>(&bytes).expect("round-trip"), value);
    fs::write(&output, bytes).expect("write");
    assert_eq!(hash(&fs::read(&output).expect("output")), hash(&serde_json::to_vec_pretty(&value).unwrap()));
}
