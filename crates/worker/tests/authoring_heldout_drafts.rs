//! The generated held-out teach and hint drafts pass the production gates
//! (C6, L4, L5).
//!
//! `scripts/authoring/generate_foundations_drafts.py` writes one teach page
//! and one hint ladder for every classified pure-numeric knowledge point of
//! four units, under `docs/content-foundations/<unit>/`. Every draft here
//! runs through the same `verify_kind` the worker's `author` command calls,
//! with a dense synthetic answer set, so a draft that would fail import
//! fails here first.
#![allow(clippy::unwrap_used)]
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cadus_core::curriculum::load_curriculum;
use cadus_core::instruction::ServedInstance;
use cadus_worker::authoring::cli::select;
use cadus_worker::authoring::job::verify_kind;
use cadus_worker::authoring::prompt::Kind;
use serde_json::Value;

/// Unit id to its curriculum file, the units this generator has drafted.
const UNITS: &[(&str, &str)] = &[
    ("fractions-decimals", "01-fractions-decimals.yaml"),
    ("integers-negatives", "02-integers-negatives.yaml"),
    ("exponents-radicals", "06-exponents-radicals.yaml"),
    ("rational-trig", "09-rational-trig.yaml"),
];

/// The largest line count one tracked draft file keeps.
const MAX_LINES: usize = 2000;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest_dir(unit: &str) -> PathBuf {
    root().join("docs/content-foundations").join(unit)
}

/// Every draft row of one unit's manifest, in manifest order.
fn rows(unit: &str) -> Vec<Value> {
    let dir = manifest_dir(unit);
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    let mut rows = Vec::new();
    for file in manifest["files"].as_array().unwrap() {
        let path = dir.join(file.as_str().unwrap());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.lines().count() < MAX_LINES,
            "{} holds {MAX_LINES} lines or more",
            path.display()
        );
        let mut part: Vec<Value> = serde_json::from_str(&text).unwrap();
        rows.append(&mut part);
    }
    rows
}

/// A dense set of small answers, so no hint rung can coincidentally name one.
fn dense_instances() -> Vec<ServedInstance> {
    (0..=1000)
        .map(|answer| ServedInstance {
            problem: format!("A different practice problem with answer {answer}"),
            answer: answer.to_string(),
        })
        .collect()
}

#[test]
fn every_drafted_knowledge_point_sits_inside_its_declared_unit() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    for (unit, file) in UNITS {
        let text =
            std::fs::read_to_string(root().join("curriculum/foundations").join(file)).unwrap();
        let unit_topics: Vec<&str> = curriculum
            .topics()
            .iter()
            .map(|topic| topic.id.as_str())
            .filter(|id| text.contains(&format!("  - id: {id}\n")))
            .collect();
        let rows = rows(unit);
        assert!(!rows.is_empty(), "{unit} drafted nothing");
        let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
        for row in &rows {
            let key = row["kp_id"].as_str().unwrap().to_owned();
            let topic_id = key.split('/').next().unwrap();
            assert!(unit_topics.contains(&topic_id), "{key} is outside {unit}");
            let kind = row["kind"].as_str().unwrap().to_owned();
            *seen.entry((key, kind)).or_default() += 1;
        }
        for (key, count) in &seen {
            assert_eq!(*count, 1, "{key:?} drafted more than once in {unit}");
        }
        let kps: std::collections::BTreeSet<&str> =
            seen.keys().map(|(key, _)| key.as_str()).collect();
        for kp in kps {
            for kind in ["teach", "hint_ladder"] {
                assert!(
                    seen.contains_key(&(kp.to_owned(), kind.to_owned())),
                    "{kp} is missing its {kind} in {unit}"
                );
            }
        }
    }
}

#[test]
fn every_heldout_draft_passes_its_gate() {
    let (curriculum, _) = load_curriculum(&root().join("curriculum")).unwrap();
    let mut refusals = Vec::new();
    for (unit, _) in UNITS {
        for draft in rows(unit) {
            let key = draft["kp_id"].as_str().unwrap().to_owned();
            let specs = select(&curriculum, std::slice::from_ref(&key)).unwrap();
            let kind = Kind::from_wire(draft["kind"].as_str().unwrap()).unwrap();
            let mut instances = Vec::new();
            if kind == Kind::HintLadder {
                instances.extend(dense_instances());
            }
            if let Err(rejection) = verify_kind(kind, &specs[0], &draft["arguments"], &instances) {
                refusals.push(format!(
                    "{unit} {key} {}: {}",
                    kind.as_str(),
                    rejection.message
                ));
            }
        }
    }
    assert!(refusals.is_empty(), "{}", refusals.join("\n"));
}

#[test]
fn every_heldout_hint_ladder_holds_three_question_rungs_with_no_numeral() {
    for (unit, _) in UNITS {
        for draft in rows(unit)
            .into_iter()
            .filter(|row| row["kind"] == "hint_ladder")
        {
            let key = draft["kp_id"].as_str().unwrap().to_owned();
            let hints = draft["arguments"]["hints"].as_array().unwrap();
            assert_eq!(hints.len(), 3, "{unit} {key}");
            for rung in hints {
                let text = rung.as_str().unwrap();
                assert!(text.contains('?'), "{unit} {key}: {text}");
                assert!(
                    !text.chars().any(|character| character.is_ascii_digit()),
                    "{unit} {key}: {text}"
                );
            }
        }
    }
}
