//! Exact, offline verification of the whole-course pending Teach sidecar.
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use cadus_core::{
    curriculum::{
        load_curriculum, AnswerKind, Exemplar, FiniteCaseRole, FiniteCaseVariant,
        FiniteObjectiveCase, FiniteObjectiveDomain, Slug,
    },
    instruction::template_instances,
};
use cadus_worker::authoring::{
    cli::{AuthorArgs, select_for},
    job::{document_digest, verify_kind},
    prompt::{AuthoringSpec, FiniteAuthoringPolicy, Kind},
};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[path = "whole_course_teach/archive.rs"]
mod archive;

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

/// V2 array fingerprints use recursively sorted object keys and compact JSON.
/// The Python refresher uses this same explicit canonical contract.
fn canonical_sha(value: &Value) -> String {
    let mut sorted = value.clone();
    sorted.sort_all_objects();
    format!("{:x}", Sha256::digest(serde_json::to_vec(&sorted).unwrap()))
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
    let historical = archive::verify_historical_archive(directory, &manifest);
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
        let raw_problem = body["worked_example"]["problem"].as_str().expect("problem");
        let identity = normalized(raw_problem);
        if occupied.contains(&identity) {
            assert!(
                is_registered_teach_case(&specs[kp], raw_problem),
                "{kp}: exemplar/template collision"
            );
        }
        assert!(
            teach_problems.insert(identity),
            "{kp}: duplicate Teach problem"
        );
    }
}

/// Whether the problem is an EXACT registered variant of a reviewed finite
/// teaching case (TeachOnly or TaughtRehearsal) in the spec's policy.
///
/// The production teach gate (`cadus_core::instruction::gate_teach_with_policy`
/// -> `finite::permitted_collision`) recognizes exactly this subset: without a
/// registered TeachOnly/TaughtRehearsal variant, an exemplar or template
/// collision stays a Hard-Rule-1 rejection. The whole-course pages must mirror
/// that gate and not re-reject what the gate expressly permits.
fn is_registered_teach_case(spec: &AuthoringSpec, problem: &str) -> bool {
    let Some(finite) = &spec.finite else {
        return false;
    };
    finite.domain.cases.iter().any(|case| {
        matches!(
            case.role,
            FiniteCaseRole::TeachOnly | FiniteCaseRole::TaughtRehearsal
        ) && case
            .variants
            .iter()
            .any(|variant| variant.problem.trim() == problem.trim())
    })
}

/// The collision allowance is a NARROW exception: the worked problem must be an
/// exact registered teaching variant, and the production gate (already run by
/// [`verify_kind`] above) has validated that the policy is current and the role
/// eligible. PracticeFresh and ReservedAssessment cases stay protected, and a
/// knowledge point with no finite policy stays fully collision-checked.
#[test]
fn finite_teach_collision_allowance_matches_the_registered_teaching_roles() {
    fn case(id: &str, role: FiniteCaseRole, problems: &[&str]) -> FiniteObjectiveCase {
        FiniteObjectiveCase {
            id: Slug::new(id).unwrap(),
            role,
            variants: problems
                .iter()
                .map(|problem| FiniteCaseVariant {
                    problem: (*problem).to_owned(),
                    answer: "test".to_owned(),
                    answer_contract: None,
                })
                .collect(),
        }
    }
    fn domain(cases: Vec<FiniteObjectiveCase>) -> FiniteObjectiveDomain {
        FiniteObjectiveDomain {
            schema_version: 1,
            review_ref: "sha256:regression".to_owned(),
            cases,
        }
    }
    fn spec(finite: Option<FiniteAuthoringPolicy>) -> AuthoringSpec {
        AuthoringSpec {
            kp_id: "kp1".to_owned(),
            kp_name: "Regression".to_owned(),
            topic_id: "finite-regression".to_owned(),
            topic_name: "Regression".to_owned(),
            answer_kind: AnswerKind::Numeric,
            difficulty_target: None,
            constraints: None,
            exemplars: vec![Exemplar {
                problem: "Practice case.".to_owned(),
                answer: "4".to_owned(),
                answer_contract: None,
                solution_sketch: None,
            }],
            finite,
        }
    }
    fn finite(cases: Vec<FiniteObjectiveCase>) -> FiniteAuthoringPolicy {
        FiniteAuthoringPolicy {
            domain: domain(cases),
            fingerprint: "sha256:regression".to_owned(),
        }
    }

    let teach_only = spec(Some(finite(vec![case(
        "taught",
        FiniteCaseRole::TeachOnly,
        &["Two sides and their included angle are known."],
    )])));
    assert!(is_registered_teach_case(
        &teach_only,
        "Two sides and their included angle are known."
    ));

    let rehearsal = spec(Some(finite(vec![case(
        "rehearsal",
        FiniteCaseRole::TaughtRehearsal,
        &["A pair of triangles is measured."],
    )])));
    assert!(is_registered_teach_case(
        &rehearsal,
        "A pair of triangles is measured."
    ));

    let practice = spec(Some(finite(vec![case(
        "practice",
        FiniteCaseRole::PracticeFresh,
        &["Compute 2 + 2."],
    )])));
    assert!(!is_registered_teach_case(&practice, "Compute 2 + 2."));

    let assessment = spec(Some(finite(vec![case(
        "assessment",
        FiniteCaseRole::ReservedAssessment,
        &["Reserved item."],
    )])));
    assert!(!is_registered_teach_case(&assessment, "Reserved item."));

    let undeclared = spec(Some(finite(vec![case(
        "taught",
        FiniteCaseRole::TeachOnly,
        &["A registered exact problem."],
    )])));
    assert!(!is_registered_teach_case(&undeclared, "An undeclared problem."));

    assert!(!is_registered_teach_case(&spec(None), "Compute 3 + 3."));
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
