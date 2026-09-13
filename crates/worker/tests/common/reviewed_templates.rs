//! Shared gate runners for reviewed pending-template directories.

use cadus_core::{answer::AnswerContract, curriculum::load_curriculum};
use cadus_worker::authoring::{
    cli::{AuthorArgs, select_for},
    job::verify_kind,
    prompt::{AuthoringSpec, Kind},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

use super::{json_rows, repo_root};

#[path = "../../examples/unit01/verify.rs"]
mod verify;

pub fn file_rows(relative: &str) -> Vec<Value> {
    json_rows(&[relative], None)
}

pub fn directory_rows(relative: &str) -> Vec<Value> {
    let mut paths: Vec<_> = std::fs::read_dir(repo_root().join(relative))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .flat_map(|path| {
            serde_json::from_str::<Vec<Value>>(&std::fs::read_to_string(path).unwrap()).unwrap()
        })
        .collect()
}

pub fn spec(key: &str) -> AuthoringSpec {
    let (curriculum, findings) = load_curriculum(&repo_root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    select_for(
        &curriculum,
        &AuthorArgs {
            kps: vec![key.to_owned()],
            ..AuthorArgs::default()
        },
    )
    .unwrap()
    .remove(0)
}

pub fn run_rows(rows: &[Value], output_relative: &str) -> Value {
    let output = repo_root().join(output_relative);
    std::fs::create_dir_all(&output).unwrap();
    let input = output.join("drafts.json");
    std::fs::write(&input, serde_json::to_string(rows).unwrap()).unwrap();
    verify::run(&input, &output)
}

pub fn assert_report(report: &Value, expected_rows: usize, expected_instances: Option<u64>) {
    assert_report_with_authored_collisions(report, expected_rows, expected_instances, &[]);
}

pub fn assert_report_with_authored_collisions(
    report: &Value,
    expected_rows: usize,
    expected_instances: Option<u64>,
    colliding: &[&str],
) {
    assert_report_counts(report, expected_rows, expected_instances, colliding, &[]);
}

pub fn assert_report_with_instance_overrides(
    report: &Value,
    expected_rows: usize,
    expected_instances: u64,
    overrides: &[(&str, u64)],
) {
    let keys: BTreeSet<_> = overrides.iter().map(|(key, _)| *key).collect();
    assert_eq!(keys.len(), overrides.len(), "duplicate count override");
    for key in keys {
        assert_eq!(
            report["rows"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["kp_id"] == key)
                .count(),
            1
        );
    }
    assert_report_counts(
        report,
        expected_rows,
        Some(expected_instances),
        &[],
        overrides,
    );
}

fn assert_report_counts(
    report: &Value,
    expected_rows: usize,
    expected_instances: Option<u64>,
    colliding: &[&str],
    overrides: &[(&str, u64)],
) {
    assert_eq!(report["checked"], expected_rows, "{report}");
    assert_eq!(
        report["passed"],
        expected_rows - colliding.len(),
        "{report}"
    );
    let colliding: BTreeSet<_> = colliding.iter().copied().collect();
    let mut rejected = BTreeSet::new();
    let mut instances = 0;
    for row in report["rows"].as_array().unwrap() {
        let key = row["kp_id"].as_str().unwrap();
        if !row["passed"].as_bool().unwrap() {
            rejected.insert(key);
            assert!(
                row["rejection"]
                    .as_str()
                    .unwrap()
                    .starts_with("authored/sibling collision:"),
                "{key}: {row}"
            );
            continue;
        }
        assert_eq!(row["evidence"]["exhaustive"], true);
        if expected_instances.is_some() {
            let expected = overrides
                .iter()
                .find(|(candidate, _)| *candidate == key)
                .map_or(12, |(_, count)| *count);
            assert_eq!(row["evidence"]["distinct_instances"], expected, "{key}");
            assert_eq!(row["evidence"]["instances_checked"], expected, "{key}");
        } else {
            assert!(row["evidence"]["distinct_instances"].as_u64().unwrap() >= 12);
        }
        instances += row["evidence"]["instances_checked"].as_u64().unwrap();
    }
    if let Some(expected) = expected_instances {
        assert_eq!(instances, expected);
    }
    assert_eq!(rejected, colliding);
}

pub fn assert_template19_replacements(rows: &[Value], replaced: &[&str]) {
    let canonical = file_rows("docs/content-foundations/template19-production-gate/drafts.json");
    let canonical_keys: BTreeSet<_> = canonical
        .iter()
        .map(|row| row["kp_id"].as_str().unwrap())
        .collect();
    assert_eq!(canonical_keys.len(), canonical.len());
    for key in replaced {
        let mut archived = rows
            .iter()
            .find(|row| row["kp_id"] == *key)
            .unwrap()
            .clone();
        archived.as_object_mut().unwrap().remove("status");
        let replacement = canonical.iter().find(|row| row["kp_id"] == *key).unwrap();
        assert_eq!(replacement["kind"], "template", "{key}");
        assert_eq!(replacement, &archived, "{key}");
    }
}

pub fn assert_standard_negative(rows: Vec<Value>, key: &str) {
    let row = rows
        .into_iter()
        .find(|candidate| candidate["kp_id"] == key)
        .unwrap();
    let spec = spec(key);
    let mut wrong = row["arguments"].clone();
    wrong["samples"][0]["expected"] = json!(999);
    assert_eq!(
        verify_kind(Kind::Template, &spec, &wrong, &[])
            .unwrap_err()
            .code,
        "sample-agreement"
    );
    let mut small = row["arguments"].clone();
    small["params"]["a"]["values"] = json!([14]);
    assert!(verify_kind(Kind::Template, &spec, &small, &[]).is_err());
    let mut incompatible = spec.clone();
    for item in &mut incompatible.exemplars {
        item.answer_contract = Some(AnswerContract::ReducedRatio);
    }
    assert!(verify_kind(Kind::Template, &incompatible, &row["arguments"], &[]).is_err());
}

#[macro_export]
macro_rules! reviewed_template_tests {
    ($directory:expr, $rows:expr, $output:expr, $instances:expr, $key:expr) => {
        #[test]
        fn all_pending_templates_exhaust_the_real_gate_and_avoid_authored_and_sibling_problems() {
            let rows = $crate::common::reviewed_templates::directory_rows($directory);
            assert_eq!(rows.len(), $rows);
            let report = $crate::common::reviewed_templates::run_rows(&rows, $output);
            $crate::common::reviewed_templates::assert_report(&report, $rows, Some($instances));
        }

        #[test]
        fn wrong_samples_small_spaces_and_wrong_contracts_are_rejected() {
            let rows = $crate::common::reviewed_templates::directory_rows($directory);
            $crate::common::reviewed_templates::assert_standard_negative(rows, $key);
        }
    };
}
