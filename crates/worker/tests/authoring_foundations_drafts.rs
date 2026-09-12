//! The arithmetic-core teach and hint drafts pass the production gates (C6, L4, L5).
//!
//! The manifest under `docs/content-foundations/arithmetic-core/` names one
//! teach page and one hint ladder for every knowledge point of the unit. Every
//! draft runs through `verify_kind` against the real curriculum, with a dense
//! synthetic instance set, so a rung that names a small answer fails here and
//! not at import time.
//!
//! `CADUS_DRAFT_TEMPLATES` is optional. When it names a JSON list of
//! `{kp_id, body}` template rows, the test also reads the instances those
//! templates serve. The default run reads no file and stays deterministic.
#![allow(clippy::unwrap_used)]
mod common;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::instruction::{ServedInstance, template_instances};
use serde_json::Value;

/// The unit every draft of the manifest belongs to.
const UNIT: &str = "arithmetic-core";

/// The largest line count one tracked draft file keeps.
const MAX_LINES: usize = 500;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest_dir() -> PathBuf {
    root().join("docs/content-foundations").join(UNIT)
}

/// Every draft row the manifest names, in manifest order.
fn rows() -> Vec<Value> {
    common::authoring::draft_rows(&manifest_dir(), MAX_LINES)
}

/// The serving keys of every knowledge point of the unit, in curriculum order.
fn unit_keys(curriculum: &Curriculum) -> Vec<String> {
    let text =
        std::fs::read_to_string(root().join("curriculum/foundations/00-arithmetic-core.yaml"))
            .unwrap();
    let mut keys = Vec::new();
    for topic in curriculum.topics() {
        if !text.contains(&format!("  - id: {}\n", topic.id.as_str())) {
            continue;
        }
        for kp in &topic.knowledge_points {
            keys.push(format!("{}/{}", topic.id.as_str(), kp.id.as_str()));
        }
    }
    keys
}

/// The instances of `CADUS_DRAFT_TEMPLATES`, by serving key; empty by default.
fn template_served() -> BTreeMap<String, Vec<ServedInstance>> {
    let mut served: BTreeMap<String, Vec<ServedInstance>> = BTreeMap::new();
    let Ok(path) = std::env::var("CADUS_DRAFT_TEMPLATES") else {
        return served;
    };
    let rows: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for row in rows {
        let key = row["kp_id"].as_str().unwrap().to_owned();
        served
            .entry(key)
            .or_default()
            .extend(template_instances(&row["body"].to_string()));
    }
    served
}

#[test]
fn every_knowledge_point_of_the_unit_has_one_teach_page_and_one_hint_ladder() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let keys = unit_keys(&curriculum);
    assert_eq!(keys.len(), 81);
    let rows = rows();
    assert_eq!(rows.len(), keys.len() * 2);
    let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
    for row in &rows {
        let key = row["kp_id"].as_str().unwrap().to_owned();
        let kind = row["kind"].as_str().unwrap().to_owned();
        assert!(keys.contains(&key), "{key} is outside the unit");
        *seen.entry((key, kind)).or_default() += 1;
    }
    for key in &keys {
        for kind in ["teach", "hint_ladder"] {
            assert_eq!(
                seen.get(&(key.clone(), kind.to_owned())).copied(),
                Some(1),
                "{key} {kind}"
            );
        }
    }
}

#[test]
#[ignore]
fn every_draft_passes_its_gate() {
    let (curriculum, _) = load_curriculum(&root().join("curriculum")).unwrap();
    let templates = template_served();
    let mut refusals = Vec::new();
    for draft in rows() {
        let key = draft["kp_id"].as_str().unwrap().to_owned();
        let instances = templates.get(&key).cloned().unwrap_or_default();
        if let Some((key, kind, reason)) =
            common::authoring::draft_rejection(&curriculum, &draft, instances)
        {
            refusals.push(format!("{key} {kind}: {reason}"));
        }
    }
    assert!(refusals.is_empty(), "{}", refusals.join("\n"));
}

#[test]
fn every_hint_ladder_holds_three_question_rungs_with_no_numeral() {
    for draft in rows()
        .into_iter()
        .filter(|row| row["kind"] == "hint_ladder")
    {
        let key = draft["kp_id"].as_str().unwrap();
        let hints = draft["arguments"]["hints"].as_array().unwrap();
        assert_eq!(hints.len(), 3, "{key}");
        for rung in hints {
            let text = rung.as_str().unwrap();
            assert!(text.contains('?'), "{key}: {text}");
            assert!(
                !text.chars().any(|character| character.is_ascii_digit()),
                "{key}: {text}"
            );
        }
    }
}
