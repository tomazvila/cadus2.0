//! Exact, offline verification of the whole-course pending Teach sidecar.
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
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
use serde_json::Value;
use sha2::{Digest, Sha256};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn directory() -> PathBuf {
    root().join("docs/content-foundations/whole-course-teach")
}

fn read(path: impl AsRef<Path>) -> Value {
    let path = path.as_ref();
    serde_json::from_slice(
        &std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn sha(path: impl AsRef<Path>) -> String {
    format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()))
}

fn canonical_sha(value: &Value) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap()))
}

/// The v2 migration binds each historical row with JSON whose object keys are
/// recursively sorted before compact serialization. Historical file bytes stay
/// untouched; this binds the row data independently of its source formatting.
fn canonical_historical_row_sha(value: &Value) -> String {
    let mut sorted = value.clone();
    sorted.sort_all_objects();
    format!("{:x}", Sha256::digest(serde_json::to_vec(&sorted).unwrap()))
}

fn rows(paths: impl Iterator<Item = PathBuf>) -> Vec<Value> {
    paths
        .flat_map(|path| read(path).as_array().expect("row array").clone())
        .collect()
}

fn keyed(rows: &[Value], label: &str) -> BTreeMap<String, Value> {
    let mut result = BTreeMap::new();
    for row in rows {
        let key = row["kp_id"]
            .as_str()
            .unwrap_or_else(|| panic!("{label} KP"))
            .to_owned();
        assert!(
            result.insert(key.clone(), row.clone()).is_none(),
            "duplicate {label} {key}"
        );
    }
    result
}

fn specs() -> BTreeMap<String, AuthoringSpec> {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).expect("curriculum");
    assert!(findings.is_empty(), "{findings:?}");
    select_for(
        &curriculum,
        &AuthorArgs {
            course: Some("foundations".into()),
            ..AuthorArgs::default()
        },
    )
    .expect("Foundations specs")
    .into_iter()
    .map(|spec| (format!("{}/{}", spec.topic_id, spec.kp_id), spec))
    .collect()
}

fn normalized(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .replace("\\times", "*")
        .replace("\\cdot", "*")
        .replace("\\div", "/")
}

fn part_paths(prefix: &str) -> BTreeSet<String> {
    (1..=30)
        .map(|part| format!("{prefix}/part-{part:02}.json"))
        .collect()
}

type Sources = BTreeMap<String, (String, Vec<cadus_core::instruction::ServedInstance>)>;

fn manifests(directory: &Path) -> (Value, Value) {
    let manifest = read(directory.join("manifest.json"));
    let import = read(directory.join("import-manifest.json"));
    assert_eq!(manifest["status"], "pending-ai-review");
    assert_eq!(manifest["schema_version"], 2);
    assert!(
        manifest["historical_archive"]["sha256"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert_eq!(
        manifest["side_effects"],
        serde_json::json!({
            "api_calls": 0, "approvals": 0, "database_connections": 0, "imports": 0
        })
    );
    assert_eq!(
        manifest["counts"],
        serde_json::json!({
            "accepted_pending_teach": 735, "canonical_kps": 809,
            "missing_teach": 735, "rejected": 0, "residual": 0
        })
    );
    assert_eq!(import["status"], "pending-ai-review");
    assert_eq!(import["schema_version"], 2);
    assert_eq!(import["kinds"], serde_json::json!(["teach"]));
    assert_eq!(import["knowledge_points"], 735);
    assert_eq!(import["api_calls"], 0);
    assert_eq!(import["approved_by_this_tool"], 0);
    (manifest, import)
}

fn assert_archive_file_hash(path: &Path, expected: &Value) {
    assert_eq!(
        format!("sha256:{}", sha(path)),
        expected.as_str().expect("archive sha"),
        "{}",
        path.display()
    );
}

fn archive_paths() -> BTreeSet<String> {
    let mut expected = part_paths("reviews");
    expected.extend(["import-manifest.v1.json".into(), "manifest.v1.json".into()]);
    expected
}

fn verify_historical_archive(directory: &Path, manifest: &Value) -> BTreeMap<String, String> {
    let technical = &manifest["technical_evidence"];
    assert_eq!(technical["path"], "technical-evidence-v2.json");
    assert_archive_file_hash(
        &directory.join(technical["path"].as_str().expect("technical path")),
        &technical["sha256"],
    );
    let technical_value = read(directory.join(technical["path"].as_str().unwrap()));
    assert_eq!(technical_value["status"], "pending-ai-review");
    assert_eq!(technical_value["rows"].as_array().unwrap().len(), 735);
    let expected_inputs: BTreeSet<_> = std::iter::once("inputs/templates.json".to_owned())
        .chain((1..=30).map(|part| format!("drafts/part-{part:02}.json")))
        .collect();
    let input_hashes = technical_value["raw_input_sha256"]
        .as_object()
        .expect("technical raw input hashes");
    assert_eq!(
        input_hashes.keys().cloned().collect::<BTreeSet<_>>(),
        expected_inputs
    );
    for (path, digest) in input_hashes {
        assert_eq!(
            digest,
            &Value::String(format!("sha256:{}", sha(directory.join(path)))),
            "technical input {path}"
        );
    }
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).expect("curriculum");
    assert!(findings.is_empty(), "{findings:?}");
    assert_eq!(
        technical_value["curriculum_hash"],
        curriculum_hash(&curriculum)
    );
    let current_reviews =
        rows((1..=30).map(|part| directory.join(format!("reviews/part-{part:02}.json"))));
    let technical_rows = keyed(
        technical_value["rows"].as_array().expect("technical rows"),
        "technical",
    );
    let review_rows = keyed(&current_reviews, "current review");
    assert_eq!(
        technical_rows.keys().collect::<BTreeSet<_>>(),
        review_rows.keys().collect()
    );
    for (kp, row) in &technical_rows {
        let review = &review_rows[kp];
        for field in ["teach_digest", "template_digest", "ai_review"] {
            assert_eq!(row[field], review[field], "technical {field} {kp}");
        }
        for field in ["collision", "production_gate", "context_coverage"] {
            assert_eq!(
                row[field], review["verification"][field],
                "technical {field} {kp}"
            );
        }
    }

    let archive = &manifest["historical_archive"];
    assert_eq!(archive["path"], "historical-archive/index.json");
    let index_path = directory.join(archive["path"].as_str().expect("index path"));
    assert_archive_file_hash(&index_path, &archive["sha256"]);
    let index = read(&index_path);
    assert_eq!(index["status"], "historical-as-encountered");
    assert_eq!(
        index["source_head"],
        "caa62d3d88c002722105b8d1f12352b0f47f9da6"
    );

    let files = index["files"].as_array().expect("archive files");
    let paths: BTreeSet<_> = files
        .iter()
        .map(|file| file["path"].as_str().expect("archive path").to_owned())
        .collect();
    assert_eq!(paths, archive_paths());
    assert_eq!(paths.len(), files.len(), "duplicate archive file path");
    for file in files {
        let path = file["path"].as_str().expect("archive path");
        assert!(archive_paths().contains(path), "unsafe archive path {path}");
        assert_archive_file_hash(
            &directory.join("historical-archive").join(path),
            &file["sha256"],
        );
    }

    let review_files: BTreeSet<_> = index["reviews"]
        .as_array()
        .expect("archive reviews")
        .iter()
        .map(|file| file["path"].as_str().expect("review path").to_owned())
        .collect();
    assert_eq!(review_files, part_paths("reviews"));
    let mut rows = BTreeMap::new();
    for file in index["reviews"].as_array().expect("archive reviews") {
        let path = file["path"].as_str().expect("archive review path");
        let full = directory.join("historical-archive").join(path);
        assert_archive_file_hash(&full, &file["sha256"]);
        for row in read(full).as_array().expect("historical rows") {
            let kp = row["kp_id"].as_str().expect("historical kp").to_owned();
            let binding = format!("sha256:{}", canonical_historical_row_sha(row));
            assert!(
                rows.insert(kp.clone(), binding).is_none(),
                "duplicate historical {kp}"
            );
        }
    }
    assert_eq!(rows.len(), 735);
    let generated: BTreeMap<String, String> = technical_value["historical_row_sha256"]
        .as_object()
        .expect("generated historical bindings")
        .iter()
        .map(|(kp, digest)| {
            (
                kp.clone(),
                digest.as_str().expect("binding digest").to_owned(),
            )
        })
        .collect();
    assert_eq!(generated, rows, "reproducible historical row bindings");
    rows
}

#[test]
fn historical_hash_binding_rejects_tampered_file() {
    let source = directory().join("historical-archive/reviews/part-01.json");
    let expected = Value::String(format!("sha256:{}", sha(&source)));
    let path = std::env::temp_dir().join(format!(
        "cadus-whole-course-archive-tamper-{}-part-01.json",
        std::process::id()
    ));
    fs::write(&path, b"tampered archive bytes").expect("write tampered archive");
    let rejected = std::panic::catch_unwind(|| assert_archive_file_hash(&path, &expected)).is_err();
    fs::remove_file(&path).expect("remove tampered archive");
    assert!(
        rejected,
        "tampered archive bytes must fail their recorded SHA-256"
    );
}

fn verify_inventory(directory: &Path, manifest: &Value, import: &Value) {
    let files = manifest["files"].as_array().expect("evidence files");
    let paths: BTreeSet<_> = files
        .iter()
        .map(|file| file["path"].as_str().unwrap().to_owned())
        .collect();
    let mut expected = part_paths("drafts");
    expected.extend(part_paths("reviews"));
    expected.extend([
        "inputs/coverage.json".into(),
        "inputs/templates.json".into(),
    ]);
    assert_eq!(paths, expected);
    let import_paths: BTreeSet<_> = import["files"]
        .as_array()
        .expect("import files")
        .iter()
        .map(|file| file.as_str().expect("import path").to_owned())
        .collect();
    assert_eq!(import_paths, part_paths("drafts"));
    for file in files {
        let path = file["path"].as_str().expect("evidence path");
        let value = read(directory.join(path));
        let row_count = value
            .as_array()
            .or_else(|| value["rows"].as_array())
            .expect("evidence rows")
            .len();
        assert_eq!(row_count, file["rows"], "{path}");
        assert_eq!(
            sha(directory.join(path)),
            file["sha256"].as_str().unwrap(),
            "{path}"
        );
    }
}

fn artifacts(directory: &Path) -> (BTreeMap<String, Value>, BTreeMap<String, Value>) {
    let (manifest, import) = manifests(directory);
    let historical = verify_historical_archive(directory, &manifest);
    verify_inventory(directory, &manifest, &import);
    let draft_paths = import["files"]
        .as_array()
        .expect("import files")
        .iter()
        .map(|file| directory.join(file.as_str().expect("import path")));
    let review_paths = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|file| {
            file["path"]
                .as_str()
                .filter(|path| path.starts_with("reviews/"))
        })
        .map(|path| directory.join(path));
    let drafts = rows(draft_paths);
    let reviews = rows(review_paths);
    for review in &reviews {
        let kp = review["kp_id"].as_str().expect("review kp");
        assert_eq!(
            review["historical_review"]["sha256"], historical[kp],
            "{kp}"
        );
    }
    assert_eq!(drafts.len(), 735);
    assert_eq!(reviews.len(), 735);
    assert_eq!(
        canonical_sha(&Value::Array(drafts.clone())),
        manifest["canonical_arrays"]["drafts_sha256"]
    );
    assert_eq!(
        canonical_sha(&Value::Array(reviews.clone())),
        manifest["canonical_arrays"]["reviews_sha256"]
    );
    (keyed(&drafts, "draft"), keyed(&reviews, "review"))
}

fn missing_ids(directory: &Path) -> BTreeSet<String> {
    let coverage = read(directory.join("inputs/coverage.json"));
    let missing: BTreeSet<_> = coverage["rows"]
        .as_array()
        .expect("coverage rows")
        .iter()
        .filter(|row| row["kind"] == "teach" && row["status"] == "skipped")
        .map(|row| row["kp_id"].as_str().expect("coverage KP").to_owned())
        .collect();
    assert_eq!(missing.len(), 735);
    missing
}

fn source_evidence(
    directory: &Path,
    specs: &BTreeMap<String, AuthoringSpec>,
) -> (Sources, BTreeSet<String>) {
    let templates = read(directory.join("inputs/templates.json"));
    let templates = keyed(templates.as_array().expect("templates"), "template");
    assert_eq!(templates.len(), 809);
    let mut sources = BTreeMap::new();
    let mut occupied = BTreeSet::new();
    for (kp, spec) in specs {
        occupied.extend(spec.exemplars.iter().map(|row| normalized(&row.problem)));
        let template = &templates[kp];
        assert_eq!(template["kind"], "template", "{kp}");
        assert_eq!(template.as_object().unwrap().len(), 3, "{kp}");
        let body = verify_kind(Kind::Template, spec, &template["arguments"], &[])
            .unwrap_or_else(|error| panic!("{kp}: {error}"));
        let served = template_instances(&body);
        occupied.extend(served.iter().map(|row| normalized(&row.problem)));
        sources.insert(
            kp.clone(),
            (document_digest(kp, Kind::Template, &body), served),
        );
    }
    (sources, occupied)
}

fn verify_review(kp: &str, review: &Value) {
    assert_eq!(review["kp_id"], kp);
    assert_eq!(review["ai_review"], "pending", "{kp}");
    assert_eq!(review["verification"]["collision"], "clear", "{kp}");
    assert_eq!(
        review["verification"]["production_gate"], "accepted",
        "{kp}"
    );
    assert_eq!(
        review["verification"]["context_coverage"], "sampled_template_instances",
        "{kp}"
    );
    assert!(
        review["historical_review"]["path"]
            .as_str()
            .unwrap()
            .starts_with("historical-archive/reviews/")
    );
    assert!(
        review["historical_review"]["sha256"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    for forbidden in [
        "human_approval",
        "independent_acceptance",
        "semantic_rationale",
        "approved",
    ] {
        assert!(
            review.get(forbidden).is_none(),
            "{kp}: historical semantic claim leaked"
        );
    }
}
fn verify_pages(
    drafts: &BTreeMap<String, Value>,
    reviews: &BTreeMap<String, Value>,
    specs: &BTreeMap<String, AuthoringSpec>,
    sources: &Sources,
    occupied: &BTreeSet<String>,
) {
    let mut teach_problems = BTreeSet::new();
    for (kp, draft) in drafts {
        assert_eq!(draft["kind"], "teach", "{kp}");
        assert_eq!(draft.as_object().unwrap().len(), 3, "{kp}");
        let review = &reviews[kp];
        verify_review(kp, review);
        let (source_digest, served) = &sources[kp];
        assert_eq!(review["template_digest"], *source_digest, "{kp}");
        let body = verify_kind(Kind::Teach, &specs[kp], &draft["arguments"], served)
            .unwrap_or_else(|error| panic!("{kp}: {error}"));
        assert_eq!(
            review["teach_digest"],
            document_digest(kp, Kind::Teach, &body),
            "{kp}"
        );
        let body: Value = serde_json::from_str(&body).unwrap();
        let identity = normalized(body["worked_example"]["problem"].as_str().expect("problem"));
        assert!(
            !occupied.contains(&identity),
            "{kp}: exemplar/template collision"
        );
        assert!(
            teach_problems.insert(identity),
            "{kp}: duplicate Teach problem"
        );
    }
}

#[test]
fn all_735_pending_teach_pages_are_source_bound_and_production_gated() {
    let directory = directory();
    let (drafts, reviews) = artifacts(&directory);
    assert_eq!(
        drafts.keys().collect::<BTreeSet<_>>(),
        reviews.keys().collect()
    );
    assert_eq!(
        drafts.keys().cloned().collect::<BTreeSet<_>>(),
        missing_ids(&directory)
    );
    let specs = specs();
    assert_eq!(specs.len(), 809);
    let (sources, occupied) = source_evidence(&directory, &specs);
    verify_pages(&drafts, &reviews, &specs, &sources, &occupied);
}
