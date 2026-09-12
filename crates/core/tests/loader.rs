//! U1 acceptance: the curriculum types and the YAML loader (C5, D2).
//!
//! Every number and every message below is a literal from
//! `docs/reference/curriculum-1.0-spec.md` or from a run of the 1.0 loader over
//! the fixture. No expected value comes from the code under test. The messages
//! of the forms 2.0 refuses and 1.0 accepts come from the table of spec
//! section 7, "2.0 strictness"; each test names the 1.0 behavior it departs
//! from.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::curriculum::{ParseError, RawCurriculum, load_raw_curriculum, parse_curriculum};
use common::parsed::{first_topic_ids, triples, unit_names};
use common::paths::{curriculum_root, fixture};

// --------------------------------------------------------------------------- //
// The checked-in tree
// --------------------------------------------------------------------------- //

#[test]
fn loads_the_checked_in_curriculum_with_the_literal_counts() {
    let parsed = parse_curriculum(&curriculum_root());
    assert_eq!(parsed.error, None);
    assert_eq!(
        triples(&parsed),
        Vec::new(),
        "the clean tree has 0 findings"
    );

    let catalog = parsed.catalog.clone().expect("courses.yaml loads");
    assert_eq!(catalog.courses.len(), 13, "courses");
    assert_eq!(parsed.units.len(), 88, "unit files");

    let raw = RawCurriculum::from_parsed(parsed).expect("the catalog is present");
    let topics: Vec<_> = raw.topics().collect();
    assert_eq!(topics.len(), 1090, "topics");
    assert_eq!(raw.topic_count(), 1090, "topics");

    let knowledge_points: usize = topics.iter().map(|t| t.topic.knowledge_points.len()).sum();
    assert_eq!(knowledge_points, 3138, "knowledge points");

    let exemplars: usize = topics
        .iter()
        .flat_map(|t| t.topic.knowledge_points.iter())
        .map(|kp| kp.exemplars.len())
        .sum();
    assert_eq!(exemplars, 8352, "exemplars");

    let anki_seeds: usize = topics.iter().map(|t| t.topic.anki_seeds.len()).sum();
    assert_eq!(anki_seeds, 2144, "anki seeds");

    let extra_edges: usize = topics
        .iter()
        .map(|t| t.topic.encompassings_extra.len())
        .sum();
    assert_eq!(extra_edges, 1, "encompassings_extra edges");
    let carrier = topics
        .iter()
        .find(|t| !t.topic.encompassings_extra.is_empty())
        .expect("one topic carries the edge");
    assert_eq!(carrier.topic.id.as_str(), "factoring-trinomials");
    assert_eq!(
        carrier.topic.encompassings_extra[0].id.as_str(),
        "difference-of-squares"
    );
    assert_eq!(carrier.topic.encompassings_extra[0].weight, 0.3);
}

#[test]
fn catalog_order_is_the_file_order_and_not_the_order_field() {
    // Parity trap 1: `proofs` is second in the file and fourth by `order`.
    let parsed = parse_curriculum(&curriculum_root());
    let catalog = parsed.catalog.expect("courses.yaml loads");
    let ids: Vec<&str> = catalog.courses.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "foundations",
            "proofs",
            "geometry",
            "probability-statistics",
            "precalculus",
            "discrete-mathematics",
            "calculus-1",
            "calculus-2",
            "linear-algebra",
            "multivariable-calculus",
            "differential-equations",
            "abstract-algebra",
            "category-theory",
        ]
    );
    let orders: Vec<i64> = catalog.courses.iter().map(|c| c.order).collect();
    assert_eq!(orders, vec![1, 4, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13]);
}

#[test]
fn the_load_index_counts_topics_in_load_order() {
    let (raw, findings) = load_raw_curriculum(&curriculum_root()).expect("the tree loads");
    assert!(findings.is_empty());
    let topics: Vec<_> = raw.topics().collect();

    let indices: Vec<usize> = topics.iter().map(|t| t.load_index).collect();
    assert_eq!(indices, (0..1090).collect::<Vec<usize>>());

    let named: Vec<&str> = topics.iter().take(3).map(|t| t.topic.id.as_str()).collect();
    assert_eq!(
        named,
        vec![
            "single-digit-addition",
            "subtraction-facts",
            "multiplication-tables"
        ]
    );
    let last: Vec<&str> = topics[1087..].iter().map(|t| t.topic.id.as_str()).collect();
    assert_eq!(
        last,
        vec![
            "cartesian-closed-categories",
            "presheaves-intro",
            "toposes-glimpse"
        ]
    );

    // The course of a topic is the `course` field of its file, never the
    // directory (spec section 1).
    let first = &topics[0];
    assert_eq!(first.course_dir, "foundations");
    assert_eq!(first.unit.course.as_str(), "foundations");
    assert_eq!(first.file_name, "00-arithmetic-core.yaml");
    assert_eq!(first.unit.unit, "arithmetic-core");
}

// --------------------------------------------------------------------------- //
// Parse-stage findings
// --------------------------------------------------------------------------- //

#[test]
fn an_unknown_key_is_a_schema_finding() {
    // Parity trap 4. The message is the 1.0 output for this fixture.
    let parsed = parse_curriculum(&fixture("unknown-key"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "schema",
            "topics.0.bogus: Extra inputs are not permitted",
            Some("demo/01-basics.yaml"),
        )]
    );
    assert!(parsed.findings[0].fatal);
    assert!(parsed.units.is_empty(), "the broken file is dropped");
}

#[test]
fn a_weight_above_one_is_weight_out_of_range() {
    let parsed = parse_curriculum(&fixture("weight-out-of-range"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "weight_out_of_range",
            "topics.1.prerequisites.0.weight: Input should be less than or equal to 1",
            Some("demo/01-basics.yaml"),
        )]
    );
    assert!(parsed.findings[0].fatal);
}

#[test]
fn a_missing_courses_yaml_is_curriculum_not_found() {
    let root = fixture("no-courses-file");
    let parsed = parse_curriculum(&root);
    assert_eq!(
        parsed.error,
        Some(ParseError::CurriculumNotFound { path: root.clone() })
    );
    assert_eq!(
        parsed.error.unwrap().to_string(),
        format!("no courses.yaml under {}", root.display())
    );
    assert_eq!(parsed.catalog, None);
    assert!(parsed.units.is_empty());

    let error = load_raw_curriculum(&root).expect_err("the load fails");
    assert_eq!(error, ParseError::CurriculumNotFound { path: root });
}

#[test]
fn an_absent_course_directory_is_an_advisory_finding() {
    let parsed = parse_curriculum(&fixture("missing-course-dir"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "missing_course_dir",
            "no unit directory ghost/ for course",
            None
        )]
    );
    assert!(!parsed.findings[0].fatal, "nothing was dropped");
    assert_eq!(parsed.units.len(), 1, "the course that exists still loads");
}

#[test]
fn a_course_without_unit_files_is_an_advisory_finding() {
    let parsed = parse_curriculum(&fixture("empty-course"));
    assert_eq!(
        triples(&parsed),
        vec![("empty_course", "course hollow has no unit files", None)]
    );
    assert!(!parsed.findings[0].fatal, "an empty course omits no topics");
    assert_eq!(parsed.units.len(), 1);
}

#[test]
fn an_empty_document_is_an_empty_mapping() {
    // Spec section 1: an empty document is `{}`, so the three required keys of
    // `Unit` are missing. The messages are the 1.0 output for this fixture.
    let parsed = parse_curriculum(&fixture("empty-document"));
    assert_eq!(
        triples(&parsed),
        vec![
            ("schema", "unit: Field required", Some("demo/00-empty.yaml")),
            (
                "schema",
                "course: Field required",
                Some("demo/00-empty.yaml")
            ),
            (
                "schema",
                "module: Field required",
                Some("demo/00-empty.yaml")
            ),
        ]
    );
    assert_eq!(parsed.units.len(), 1, "the second file still loads");
}

#[test]
fn a_broken_yaml_file_is_one_finding_and_the_other_files_load() {
    let parsed = parse_curriculum(&fixture("broken-yaml"));
    assert_eq!(parsed.findings.len(), 1, "one YAML error per file");
    let finding = &parsed.findings[0];
    assert_eq!(finding.code, "yaml");
    assert_eq!(finding.file.as_deref(), Some("demo/01-bad.yaml"));
    assert!(
        finding.message.starts_with("demo/01-bad.yaml: "),
        "message was {:?}",
        finding.message
    );
    assert!(finding.fatal);
    assert_eq!(parsed.units.len(), 1, "the good file still loads");
    assert_eq!(parsed.units[0].file_name, "02-good.yaml");
}

// --------------------------------------------------------------------------- //
// File discovery
// --------------------------------------------------------------------------- //

#[test]
fn unit_files_load_in_code_point_order_and_not_recursively() {
    // Parity traps 2 and 3: a code-point sort, a non-recursive `*.yaml` glob,
    // and `.yml` stays invisible.
    let parsed = parse_curriculum(&fixture("file-order"));
    assert_eq!(triples(&parsed), Vec::new());
    assert_eq!(
        unit_names(&parsed),
        vec!["01-a.yaml", "02-b.yaml", "10-c.yaml", "Z-upper.yaml"],
        "digits sort before an upper-case letter"
    );
    assert_eq!(
        first_topic_ids(&parsed),
        vec!["first", "second", "third", "fourth"]
    );
    assert_eq!(parsed.units[0].rel_path(), "demo/01-a.yaml");
}

// --------------------------------------------------------------------------- //
// YAML 1.1 booleans
// --------------------------------------------------------------------------- //

#[test]
fn a_yaml_1_1_boolean_yes_is_a_schema_error() {
    // Pinned choice: the loader reads YAML 1.2, so `yes` is the string "yes"
    // and not the boolean true. 1.0 runs PyYAML, which reads YAML 1.1 and
    // accepts `yes`. The message is 2.0's own (review finding 16): it names the
    // form and the fix, and it does not borrow the pydantic text for a value
    // pydantic accepts. The checked-in tree writes `true` and `false` only, so
    // the two loaders agree on it.
    let parsed = parse_curriculum(&fixture("yaml-1-1-booleans"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "schema",
            "topics.0.core: YAML 1.1 boolean 'yes' is not accepted; write true or false",
            Some("demo/01-basics.yaml"),
        )]
    );
}

#[test]
fn the_checked_in_tree_writes_no_yaml_1_1_boolean() {
    // The guard for the choice above: every `core`, `drill` and `key` value of
    // the tree is a YAML 1.2 boolean, so the divergence cannot reach the tree
    // without this test going red.
    let (raw, _findings) = load_raw_curriculum(&curriculum_root()).expect("the tree loads");
    let mut cores = 0_usize;
    let mut drills = 0_usize;
    for topic in raw.topics() {
        if topic.topic.core {
            cores += 1;
        }
        if topic.topic.drill {
            drills += 1;
        }
    }
    // Spec section 3: `core: True 725, False 365` and `drill: True 22`.
    assert_eq!(cores, 725, "core topics");
    assert_eq!(drills, 22, "drill topics");
}
