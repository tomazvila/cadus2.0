//! Separate source-bound historical replay and current native Unit06 regression.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

use cadus_core::template;
use cadus_worker::authoring::{job, prompt};
use serde_json::{Value, json};

#[path = "support/unit06_current_evidence.rs"]
mod current_evidence;

const HISTORY: &str = "docs/content-foundations/unit06-correction/history/pre-2026-09-13";

const SUPERSEDED_BY: &str = "docs/content-foundations/template19-production-gate/drafts.json";
const SUPERSEDED_CANONICAL_DIGESTS: [(&str, &str); 3] = [
    ("pythagorean-converse/kp3", "sha256:350315d088dc0313"),
    ("radical-equations-basic/kp2", "sha256:c5005dc8bce57196"),
    ("radical-equations-basic/kp3", "sha256:c1063117806e183a"),
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(file: &str) -> Vec<Value> {
    if file == "historical-candidates.json" {
        return serde_json::from_str(
            &fs::read_to_string(root().join(HISTORY).join("template-candidates.json")).unwrap(),
        )
        .unwrap();
    }
    if file == "candidates.json" {
        return serde_json::from_str(
            &fs::read_to_string(
                root().join("crates/worker/tests/fixtures/unit06-template-candidates.json"),
            )
            .unwrap(),
        )
        .unwrap();
    }
    serde_json::from_str(
        &fs::read_to_string(
            root()
                .join("docs/content-foundations/unit06-correction")
                .join(file),
        )
        .unwrap(),
    )
    .unwrap()
}

fn template19_rows() -> BTreeMap<String, Value> {
    serde_json::from_str::<Vec<Value>>(&fs::read_to_string(root().join(SUPERSEDED_BY)).unwrap())
        .unwrap()
        .into_iter()
        .map(|row| (row["kp_id"].as_str().unwrap().to_owned(), row))
        .collect()
}

fn template19_sources() -> BTreeMap<String, Value> {
    let path = root().join("docs/content-foundations/template19-production-gate/sources.json");
    serde_json::from_str::<Vec<Value>>(&fs::read_to_string(path).unwrap())
        .unwrap()
        .into_iter()
        .map(|row| (row["kp_id"].as_str().unwrap().to_owned(), row))
        .collect()
}

fn historical_spec(key: &str) -> prompt::AuthoringSpec {
    let records: Vec<Value> = serde_json::from_str(
        &fs::read_to_string(root().join(HISTORY).join("authoring-specs.json")).unwrap(),
    )
    .unwrap();
    let row = records
        .iter()
        .find(|r| {
            format!(
                "{}/{}",
                r["topic_id"].as_str().unwrap(),
                r["kp_id"].as_str().unwrap()
            ) == key
        })
        .unwrap();
    let finite = if row["finite_objective_domain"].is_null() {
        None
    } else {
        let domain = serde_json::from_value(row["finite_objective_domain"].clone()).unwrap();
        Some(prompt::FiniteAuthoringPolicy::new(key, &domain).unwrap())
    };
    prompt::AuthoringSpec {
        kp_id: row["kp_id"].as_str().unwrap().to_owned(),
        kp_name: row["kp_name"].as_str().unwrap().to_owned(),
        topic_id: row["topic_id"].as_str().unwrap().to_owned(),
        topic_name: row["topic_name"].as_str().unwrap().to_owned(),
        answer_kind: serde_json::from_value(row["answer_kind"].clone()).unwrap(),
        difficulty_target: serde_json::from_value(row["difficulty_target"].clone()).unwrap(),
        constraints: serde_json::from_value(row["constraints"].clone()).unwrap(),
        exemplars: serde_json::from_value(row["exemplars"].clone()).unwrap(),
        finite,
    }
}

fn assert_historical_sources() {
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(root().join(HISTORY).join("manifest.json")).unwrap(),
    )
    .unwrap();
    for (path, entry) in manifest["files"].as_object().unwrap() {
        assert_eq!(
            current_evidence::hash_bytes(&fs::read(root().join(path)).unwrap()),
            entry["sha256"],
            "historical source drift: {path}"
        );
    }
}

#[test]
fn historical_candidates_match_their_original_gate_and_review_records() {
    assert_historical_sources();
    let pending: BTreeMap<String, Value> = read("pending-review.json")
        .into_iter()
        .map(|r| (r["kp_id"].as_str().unwrap().to_owned(), r))
        .collect();
    let blockers: BTreeMap<String, Value> = read("schema-blockers.json")
        .into_iter()
        .map(|r| (r["kp_id"].as_str().unwrap().to_owned(), r))
        .collect();
    assert_eq!((pending.len(), blockers.len()), (78, 0));
    let mut statuses = BTreeMap::new();
    let canonical = template19_rows();
    let sources = template19_sources();
    let mut superseded = BTreeSet::new();
    for row in read("historical-candidates.json") {
        let key = row["kp_id"].as_str().unwrap();
        let spec = historical_spec(key);
        let body = job::verify_kind(prompt::Kind::Template, &spec, &row["arguments"], &[])
            .unwrap_or_else(|reason| panic!("{key}: {reason:?}"));
        let evidence = &pending[key];
        assert_eq!(
            evidence["body"],
            serde_json::from_str::<Value>(&body).unwrap()
        );
        let status = evidence["status"].as_str().unwrap();
        assert!(
            matches!(status, "pending" | "superseded"),
            "{key}: {status}"
        );
        *statuses.entry(status).or_insert(0) += 1;
        if status == "superseded" {
            assert_eq!(evidence["superseded_by"], SUPERSEDED_BY, "{key}");
            let replacement = &canonical[key];
            assert_eq!(replacement["kind"], "template", "{key}");
            assert_ne!(replacement["arguments"], row["arguments"], "{key}");
            let replacement_body = job::verify_kind(
                prompt::Kind::Template,
                &spec,
                &replacement["arguments"],
                &[],
            )
            .unwrap_or_else(|reason| panic!("{key}: canonical replacement: {reason:?}"));
            let expected_digest = SUPERSEDED_CANONICAL_DIGESTS
                .iter()
                .find_map(|(candidate, digest)| (*candidate == key).then_some(*digest))
                .unwrap_or_else(|| panic!("{key}: no reviewed canonical digest"));
            assert_eq!(
                job::document_digest(key, prompt::Kind::Template, &replacement_body),
                expected_digest,
                "{key}: canonical replacement body drifted"
            );
            let replacement_body: Value = serde_json::from_str(&replacement_body).unwrap();
            assert_eq!(
                replacement_body["answer_contract"], sources[key]["contract"],
                "{key}"
            );
            assert_eq!(sources[key]["repaired"], true, "{key}");
            assert_eq!(sources[key]["samples"], 12, "{key}");
            assert!(
                sources[key]["sources"].as_array().unwrap().iter().any(
                    |source| source == "docs/content-foundations/unit06-correction/drafts.json"
                ),
                "{key}"
            );
            superseded.insert(key.to_owned());
        } else {
            assert_eq!(evidence.get("superseded_by"), None, "{key}");
        }
        assert_eq!(
            evidence["digest"],
            job::document_digest(key, prompt::Kind::Template, &body)
        );
    }
    assert_eq!(
        statuses,
        BTreeMap::from([("pending", 75), ("superseded", 3)])
    );
    assert_eq!(
        superseded,
        BTreeSet::from([
            "pythagorean-converse/kp3".to_owned(),
            "radical-equations-basic/kp2".to_owned(),
            "radical-equations-basic/kp3".to_owned(),
        ])
    );
}

#[test]
fn every_historical_instance_rejects_an_off_by_one_answer_and_matches_its_recorded_derivation() {
    let mut count = 0;
    for row in read("pending-review.json") {
        let doc = template::from_body(&row["body"].to_string()).unwrap();
        let compiled = template::Compiled::new(&doc).unwrap();
        let walked = template::walk_satisfying(&doc.params, &doc.constraints).unwrap();
        assert!(walked.exhaustive);
        assert_eq!(walked.tuples.len(), 12);
        assert_eq!(
            row["instances"].as_array().unwrap().len(),
            walked.tuples.len()
        );
        for (tuple, recorded) in walked
            .tuples
            .into_iter()
            .zip(row["instances"].as_array().unwrap())
        {
            let instance = compiled.instantiate(tuple).unwrap();
            assert_eq!(recorded["problem"], instance.text);
            assert_eq!(recorded["answer"], instance.answer);
            let sketch = cadus_core::template::render(
                doc.solution_sketch.as_ref().unwrap(),
                &instance.bindings,
            )
            .unwrap();
            assert_eq!(recorded["solution_sketch"], sketch);
            let wrong = format!("({})+1", instance.answer);
            let policy = doc
                .answer_contract
                .clone()
                .unwrap_or(cadus_core::answer::AnswerContract::Exact);
            let outcome = cadus_core::answer::check_contract(&instance.answer, &wrong, policy);
            assert!(
                matches!(outcome, cadus_core::answer::Outcome::Decided(v) if !v.correct),
                "{}: {wrong}",
                row["kp_id"]
            );
            count += 1;
        }
    }
    assert_eq!(count, 936);
}

#[test]
fn every_historical_candidate_is_recorded_pending_or_explicitly_superseded() {
    let drafts = read("historical-candidates.json");
    let pending = read("pending-review.json");
    assert_eq!(drafts.len(), pending.len());
    for draft in drafts {
        assert_eq!(draft["kind"], "template");
        let review = pending
            .iter()
            .find(|review| review["kp_id"] == draft["kp_id"])
            .unwrap();
        assert!(matches!(
            review["status"].as_str(),
            Some("pending" | "superseded")
        ));
        assert_eq!(draft.get("approved"), None);
    }
    assert_eq!(json!(read("schema-blockers.json").len()), 0);
}

/// Preserve true mirrors and independently source-bound pre-existing draft lineages.
fn validate_current_lineages(
    drafts: &[Value],
    candidates: &[Value],
    manifest: &Value,
) -> Result<(), String> {
    let index = |rows: &[Value]| -> Result<BTreeMap<String, Value>, String> {
        if rows.len() != 78 {
            return Err("expected78lineage rows".to_owned());
        }
        let mut found = BTreeMap::new();
        for row in rows {
            let key = row["kp_id"].as_str().ok_or("missing lineage KP ID")?;
            if row["kind"] != "template" {
                return Err(format!("{key}: wrong lineage kind"));
            }
            if found.insert(key.to_owned(), row.clone()).is_some() {
                return Err(format!("{key}: duplicate lineage key"));
            }
        }
        Ok(found)
    };
    let drafts = index(drafts)?;
    let candidates = index(candidates)?;
    let expected: BTreeSet<String> =
        serde_json::from_value(manifest["all_kp_ids"].clone()).map_err(|e| e.to_string())?;
    if expected.len() != 78
        || drafts.keys().cloned().collect::<BTreeSet<_>>() != expected
        || candidates.keys().cloned().collect::<BTreeSet<_>>() != expected
    {
        return Err("lineage key sets differ from original78KP inventory".to_owned());
    }
    let distinct = manifest["distinct_lineages"]
        .as_object()
        .ok_or("missing distinct lineages")?;
    if distinct.len() != 21 || distinct.keys().any(|key| !expected.contains(key)) {
        return Err("expected21named distinct lineages".to_owned());
    }
    let mut mirrors = 0;
    for key in &expected {
        let draft = &drafts[key];
        let candidate = &candidates[key];
        if let Some(binding) = distinct.get(key) {
            if current_evidence::hash_bytes(draft.to_string().as_bytes())
                != binding["draft_row_sha256"]
                || current_evidence::hash_bytes(candidate.to_string().as_bytes())
                    != binding["fixture_row_sha256"]
            {
                return Err(format!("{key}: original distinct lineage row drift"));
            }
        } else {
            if draft != candidate {
                return Err(format!("{key}: true mirror drift"));
            }
            mirrors += 1;
        }
    }
    if mirrors != 57 {
        return Err("expected57true mirrors".to_owned());
    }
    Ok(())
}

fn assert_current_lineages() {
    let bytes = fs::read(
        root().join("docs/content-foundations/unit06-correction/current-lineage-manifest.json"),
    )
    .unwrap();
    assert_eq!(
        current_evidence::hash_bytes(&bytes),
        "sha256:7a87fab092427f1573699544a23aa61469d6c8e57430d96cb3817dd21e22ce53",
        "lineage manifest source drift"
    );
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    let archived = |name: &str| -> Vec<Value> {
        let source = &manifest["archives"][name];
        let bytes = fs::read(root().join(source["path"].as_str().unwrap())).unwrap();
        assert_eq!(
            current_evidence::hash_bytes(&bytes),
            source["sha256"],
            "original {name} archive drift"
        );
        serde_json::from_slice(&bytes).unwrap()
    };
    // Replay the original two lineages from exact archived source bytes as well
    // as checking their current descendants; Git and the original checkout are unnecessary.
    validate_current_lineages(&archived("drafts"), &archived("fixture"), &manifest).unwrap();
    let drafts = read("drafts.json");
    let candidates = read("candidates.json");
    validate_current_lineages(&drafts, &candidates, &manifest).unwrap();
    // Mutate each named distinct lineage independently on both sides. A relaxed
    // exception must never allow a changed row to inherit its original binding.
    for key in manifest["distinct_lineages"].as_object().unwrap().keys() {
        let mut changed_drafts = drafts.clone();
        let row = changed_drafts
            .iter_mut()
            .find(|row| row["kp_id"] == key.as_str())
            .unwrap();
        row["arguments"]["statement"] = json!("altered draft lineage negative control");
        assert!(
            validate_current_lineages(&changed_drafts, &candidates, &manifest).is_err(),
            "{key}: changed draft accepted"
        );
        let mut changed_candidates = candidates.clone();
        let row = changed_candidates
            .iter_mut()
            .find(|row| row["kp_id"] == key.as_str())
            .unwrap();
        row["arguments"]["statement"] = json!("altered fixture lineage negative control");
        assert!(
            validate_current_lineages(&drafts, &changed_candidates, &manifest).is_err(),
            "{key}: changed fixture accepted"
        );
    }
    let mirror_index = drafts
        .iter()
        .position(|row| {
            !manifest["distinct_lineages"]
                .as_object()
                .unwrap()
                .contains_key(row["kp_id"].as_str().unwrap())
        })
        .unwrap();
    let mut changed_mirror = drafts.clone();
    changed_mirror[mirror_index]["arguments"]["statement"] =
        json!("altered true mirror negative control");
    assert!(validate_current_lineages(&changed_mirror, &candidates, &manifest).is_err());
}

#[test]
fn all_current_candidates_match_current_source_bound_native_evidence() {
    let actual = current_evidence::current_receipt(&root());
    let recorded: Value =
        serde_json::from_str(&fs::read_to_string(root().join(current_evidence::RECEIPT)).unwrap())
            .unwrap();
    assert_eq!(recorded["artifact_kind"], "native_technical_receipt");
    assert_eq!(recorded["ai_review"], "not_performed");
    current_evidence::source_matches(&recorded, &actual["sources"]).unwrap();
    assert_eq!(
        recorded, actual,
        "regenerate only the current technical receipt after reviewed source changes"
    );
    assert_current_lineages();
    let mut stale = actual["sources"].clone();
    let mut changed_input = fs::read(root().join(current_evidence::CANDIDATES)).unwrap();
    changed_input.push(b' ');
    stale["candidate_sha256"] = json!(current_evidence::hash_bytes(&changed_input));
    assert!(
        current_evidence::source_matches(&recorded, &stale).is_err(),
        "stale candidate bytes must invalidate the technical receipt"
    );
}
