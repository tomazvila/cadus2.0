//! Writes current, non-semantic whole-course Teach technical evidence.
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use cadus_core::{
    curriculum::{curriculum_hash, load_curriculum},
    instruction::template_instances,
};
use cadus_worker::authoring::{
    cli::{AuthorArgs, select_for},
    job::{document_digest, verify_kind},
    prompt::{AuthoringSpec, Kind},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// Stable binding for an archived JSON row: recursively sort object keys,
/// compact-serialize, then hash. The archive bytes themselves remain verbatim.
fn historical_row_hash(value: &Value) -> String {
    let mut sorted = value.clone();
    sorted.sort_all_objects();
    hash(&serde_json::to_vec(&sorted).expect("historical row serialization"))
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .expect("usage: refresh_whole_course_teach_technical <output>");
    let root = root();
    let directory = root.join("docs/content-foundations/whole-course-teach");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).expect("curriculum");
    assert!(findings.is_empty(), "curriculum findings: {findings:?}");
    let specs: BTreeMap<String, AuthoringSpec> = select_for(
        &curriculum,
        &AuthorArgs {
            course: Some("foundations".into()),
            ..AuthorArgs::default()
        },
    )
    .expect("specs")
    .into_iter()
    .map(|s| (format!("{}/{}", s.topic_id, s.kp_id), s))
    .collect();
    let template_path = directory.join("inputs/templates.json");
    let template_bytes = fs::read(&template_path).expect("templates bytes");
    let template_value: Value = serde_json::from_slice(&template_bytes).expect("templates json");
    let mut templates = BTreeMap::new();
    for row in template_value.as_array().expect("templates") {
        let kp = row["kp_id"].as_str().expect("template kp").to_owned();
        assert!(
            templates.insert(kp.clone(), row.clone()).is_none(),
            "duplicate template {kp}"
        );
    }
    let mut input_hashes = BTreeMap::new();
    input_hashes.insert("inputs/templates.json".to_owned(), hash(&template_bytes));
    let mut historical_row_sha256 = BTreeMap::new();
    for part in 1..=30 {
        let path = directory.join(format!("historical-archive/reviews/part-{part:02}.json"));
        for row in serde_json::from_slice::<Value>(&fs::read(&path).expect("historical bytes"))
            .expect("historical json")
            .as_array()
            .expect("historical rows")
        {
            let kp = row["kp_id"].as_str().expect("historical kp").to_owned();
            assert!(
                historical_row_sha256
                    .insert(kp.clone(), historical_row_hash(row))
                    .is_none(),
                "duplicate historical KP {kp}"
            );
        }
    }
    assert_eq!(historical_row_sha256.len(), 735);
    let mut rows = Vec::new();
    for part in 1..=30 {
        let path = directory.join(format!("drafts/part-{part:02}.json"));
        let raw = fs::read(&path).expect("draft bytes");
        input_hashes.insert(format!("drafts/part-{part:02}.json"), hash(&raw));
        for draft in serde_json::from_slice::<Value>(&raw)
            .expect("draft json")
            .as_array()
            .expect("drafts")
        {
            let kp = draft["kp_id"].as_str().expect("kp");
            let spec = &specs[kp];
            let template = templates.get(kp).expect("selected template");
            let template_body = verify_kind(Kind::Template, spec, &template["arguments"], &[])
                .unwrap_or_else(|e| panic!("{kp}: {e}"));
            let served = template_instances(&template_body);
            let teach_body = verify_kind(Kind::Teach, spec, &draft["arguments"], &served)
                .unwrap_or_else(|e| panic!("{kp}: {e}"));
            rows.push(json!({"kp_id":kp,"teach_digest":document_digest(kp, Kind::Teach, &teach_body),"template_digest":document_digest(kp, Kind::Template, &template_body),"collision":"clear","production_gate":"accepted","context_coverage":"sampled_template_instances","ai_review":"pending"}));
        }
    }
    rows.sort_by(|a, b| a["kp_id"].as_str().cmp(&b["kp_id"].as_str()));
    assert_eq!(rows.len(), 735);
    assert!(
        rows.windows(2)
            .all(|pair| pair[0]["kp_id"] != pair[1]["kp_id"]),
        "duplicate draft KP"
    );
    let head = String::from_utf8(
        Command::new("git")
            .arg("rev-parse")
            .arg("HEAD")
            .current_dir(&root)
            .output()
            .expect("git")
            .stdout,
    )
    .expect("head")
    .trim()
    .to_owned();
    let value = json!({"schema_version":2,"status":"pending-ai-review","source_head":head,"curriculum_hash":curriculum_hash(&curriculum),"raw_input_sha256":input_hashes,"historical_row_sha256":historical_row_sha256,"rows":rows});
    let bytes = serde_json::to_vec_pretty(&value).expect("serialize");
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).expect("round-trip"),
        value
    );
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .expect("new output");
    file.write_all(&bytes).expect("write");
    assert_eq!(
        hash(&fs::read(&output).expect("output")),
        hash(&serde_json::to_vec_pretty(&value).unwrap())
    );
}
