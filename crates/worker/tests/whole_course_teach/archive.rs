//! Archive and current-evidence integrity checks for whole-course Teach.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use cadus_core::curriculum::{curriculum_hash, load_curriculum};
use serde_json::Value;

use super::{canonical_historical_row_sha, directory, part_paths, read, root, sha};

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

pub(crate) fn verify_historical_archive(
    directory: &Path,
    manifest: &Value,
) -> BTreeMap<String, String> {
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
