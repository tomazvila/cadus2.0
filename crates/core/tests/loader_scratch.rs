//! U1 acceptance, part 6: the forms no committed fixture carries, on trees the
//! test writes itself (spec section 7, "2.0 strictness").
//!
//! Every message below is the pydantic text of the 1.0 validator, or the 2.0
//! message that names the fix.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::os::unix::fs::PermissionsExt;

use cadus_core::curriculum::{lint_curriculum, load_raw_curriculum, parse_curriculum};
use common::parsed::triples;
use common::scratch::ScratchTree;

/// The findings of one unit file body, as `(code, message)` pairs.
fn unit_findings(label: &str, body: &str) -> Vec<(String, String)> {
    let tree = ScratchTree::new(label);
    tree.courses(&["c"]).write("c/00.yaml", body);
    let parsed = parse_curriculum(tree.root());
    parsed
        .findings
        .iter()
        .map(|f| (f.code.clone(), f.message.clone()))
        .collect()
}

/// The schema messages of one topic body: the topic fields after the id.
fn topic_messages(label: &str, fields: &str) -> Vec<String> {
    let body = format!("unit: u\ncourse: c\nmodule: M\ntopics:\n  - id: a\n{fields}");
    unit_findings(label, &body)
        .into_iter()
        .map(|(code, message)| {
            assert_eq!(code, "schema");
            message
        })
        .collect()
}

/// The schema messages of one `courses.yaml` body.
fn catalog_messages(label: &str, body: &str) -> Vec<String> {
    let tree = ScratchTree::new(label);
    tree.write("courses.yaml", body);
    let parsed = parse_curriculum(tree.root());
    assert_eq!(parsed.catalog, None, "a catalog with a finding is dropped");
    assert!(parsed.units.is_empty());
    parsed
        .findings
        .iter()
        .map(|f| {
            assert_eq!(f.code, "schema");
            assert_eq!(f.file.as_deref(), Some("courses.yaml"));
            f.message.clone()
        })
        .collect()
}

/// A catalog with a schema finding drops the whole tree: the raw load holds no
/// course, and the lint reports the parse findings alone.
#[test]
fn a_catalog_schema_finding_drops_the_tree() {
    let tree = ScratchTree::new("catalog-schema");
    tree.write("courses.yaml", "courses: 5\n");
    let (raw, findings) = load_raw_curriculum(tree.root()).expect("the stage starts");
    assert!(raw.catalog.courses.is_empty());
    assert!(raw.units.is_empty());
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].message, "courses: Input should be a valid list");
    assert_eq!(lint_curriculum(tree.root()), findings);
}

/// A course directory the process cannot read is a `yaml` finding on the
/// directory, and a unit path that is a directory is one on the file.
#[test]
fn an_unreadable_course_directory_and_a_directory_unit_are_yaml_findings() {
    let tree = ScratchTree::new("unreadable");
    tree.courses(&["c", "d"]).dir("c").dir("d/00-dir.yaml");
    let locked = tree.root().join("c");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))
        .expect("the mode changes");
    let parsed = parse_curriculum(tree.root());
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755))
        .expect("the mode changes back");
    assert_eq!(
        triples(&parsed),
        vec![
            ("yaml", "c/: Permission denied (os error 13)", Some("c/")),
            (
                "yaml",
                "d/00-dir.yaml: Is a directory (os error 21)",
                Some("d/00-dir.yaml")
            ),
        ]
    );
}

/// Every falsy document reads as `{}`, the 1.0 `data or {}` rule, and a tagged
/// document is not a mapping.
#[test]
fn a_falsy_document_is_an_empty_mapping() {
    let required = vec![
        ("schema".to_owned(), "unit: Field required".to_owned()),
        ("schema".to_owned(), "course: Field required".to_owned()),
        ("schema".to_owned(), "module: Field required".to_owned()),
    ];
    assert_eq!(unit_findings("falsy-false", "false\n"), required);
    assert_eq!(unit_findings("falsy-zero", "0\n"), required);
    assert_eq!(unit_findings("falsy-text", "''\n"), required);
    assert_eq!(unit_findings("falsy-list", "[]\n"), required);
    assert_eq!(
        unit_findings("falsy-tagged", "!custom value\n"),
        vec![(
            "schema".to_owned(),
            ": Input should be a valid dictionary or instance of Unit".to_owned()
        )]
    );
    assert_eq!(
        unit_findings("not-a-mapping", "- a\n"),
        vec![(
            "schema".to_owned(),
            ": Input should be a valid dictionary or instance of Unit".to_owned()
        )]
    );
    assert_eq!(
        unit_findings("true-document", "true\n"),
        vec![(
            "schema".to_owned(),
            ": Input should be a valid dictionary or instance of Unit".to_owned()
        )]
    );
}

/// The type messages of the string, slug, list, number and enum fields.
#[test]
fn the_field_type_messages_carry_the_pydantic_text() {
    assert_eq!(
        topic_messages(
            "types",
            "    name: 5\n    difficulty: [1]\n    answer_kind: 5\n    expected_time_secs: 0\n    \
             core: maybe\n    drill: [1]\n    prerequisites: [5]\n    knowledge_points: 5\n"
        ),
        vec![
            "topics.0.name: Input should be a valid string",
            "topics.0.core: Input should be a valid boolean, unable to interpret input",
            "topics.0.difficulty: Input should be a valid number",
            "topics.0.drill: Input should be a valid boolean",
            "topics.0.answer_kind: Input should be 'numeric', 'expression', 'multi-step' or 'proof'",
            "topics.0.expected_time_secs: Input should be greater than 0",
            "topics.0.prerequisites.0: Input should be a valid dictionary or instance of PrereqEdge",
            "topics.0.knowledge_points: Input should be a valid list",
        ]
    );
    let body = "unit: u\ncourse: c\nmodule: M\ntopics:\n  - id: '  '\n  - id: 5\n  - 5\n";
    assert_eq!(
        unit_findings("slugs", body)
            .into_iter()
            .map(|(_, message)| message)
            .collect::<Vec<String>>(),
        vec![
            "topics.0.id: String should have at least 1 character",
            "topics.0.name: Field required",
            "topics.0.difficulty: Field required",
            "topics.0.answer_kind: Field required",
            "topics.0.expected_time_secs: Field required",
            "topics.1.id: Input should be a valid string",
            "topics.1.name: Field required",
            "topics.1.difficulty: Field required",
            "topics.1.answer_kind: Field required",
            "topics.1.expected_time_secs: Field required",
            "topics.2: Input should be a valid dictionary or instance of Topic",
        ]
    );
}

/// The boolean messages: the quoted lax words, a whole number, and a fraction.
#[test]
fn the_boolean_messages_name_the_lax_forms() {
    assert_eq!(
        topic_messages(
            "bools",
            "    difficulty: 0.3\n    answer_kind: numeric\n    expected_time_secs: 30\n    \
             name: A\n    core: 'true'\n    drill: 1\n    prerequisites:\n      - id: a\n        \
             weight: 0.5\n        key: 2\n      - id: a\n        weight: 0.5\n        key: 1.5\n"
        ),
        vec![
            "topics.0.core: string 'true' is not accepted; write true or false",
            "topics.0.drill: number 1 is not accepted; write true or false",
            "topics.0.prerequisites.0.key: Input should be a valid boolean, unable to interpret \
             input",
            "topics.0.prerequisites.1.key: Input should be a valid boolean",
        ]
    );
}

/// The integer messages of the catalog `order` field.
#[test]
fn the_integer_messages_name_the_fix() {
    let course = |order: &str| format!("courses:\n- id: c\n  name: C\n  order: {order}\n");
    let cases = [
        (
            "int-text",
            "x",
            "Input should be a valid integer, unable to parse string as an integer",
        ),
        ("int-quoted", "'3'", "string '3' is not accepted; write 3"),
        (
            "int-bool",
            "true",
            "boolean true is not accepted; write an integer",
        ),
        ("int-list", "[1]", "Input should be a valid integer"),
        ("int-inf", "\n    .inf", "Input should be a finite number"),
        (
            "int-fraction",
            "\n    1.5",
            "Input should be a valid integer, got a number with a fractional part",
        ),
        ("int-whole", "\n    2.0", ""),
    ];
    for (label, order, message) in cases {
        let tree = ScratchTree::new(label);
        tree.write("courses.yaml", &course(order));
        let parsed = parse_curriculum(tree.root());
        let want: Vec<String> = if message.is_empty() {
            Vec::new()
        } else {
            vec![format!("courses.0.order: {message}")]
        };
        let schema: Vec<String> = parsed
            .findings
            .iter()
            .filter(|f| f.code == "schema")
            .map(|f| f.message.clone())
            .collect();
        assert_eq!(schema, want, "{label}");
    }
}

/// A null in an optional field is the absent value, and no finding.
#[test]
fn a_null_optional_field_is_absent() {
    let tree = ScratchTree::new("nulls");
    tree.write(
        "courses.yaml",
        "courses:\n- id: c\n  name: C\n  order: 1\n  mastery_floor_course: null\n",
    )
    .unit(
        "c/00.yaml",
        "c",
        &[(
            "a",
            "    diagnostic_exemplar: null\n    knowledge_points:\n      - id: kp1\n        name: K\n        \
             constraints: null\n        exemplars:\n          - problem: P\n            answer: A\n            \
             solution_sketch: null\n",
        )],
    );
    let parsed = parse_curriculum(tree.root());
    assert_eq!(triples(&parsed), Vec::new());
    assert_eq!(parsed.units.len(), 1);
}

/// Every mapping shape of section 1 refuses a scalar where it wants an object.
#[test]
fn a_scalar_in_place_of_an_object_names_the_model() {
    assert_eq!(
        catalog_messages("catalog-list", "- a\n"),
        vec![": Input should be a valid dictionary or instance of CourseCatalog"]
    );
    assert_eq!(
        catalog_messages("course-scalar", "courses: [5]\n"),
        vec!["courses.0: Input should be a valid dictionary or instance of Course"]
    );
    assert_eq!(
        topic_messages(
            "objects",
            "    name: A\n    difficulty: 0.3\n    answer_kind: numeric\n    expected_time_secs: 30\n    \
             diagnostic_exemplar: 5\n    anki_seeds: [5]\n    knowledge_points:\n      - 5\n      - id: kp1\n        \
             name: K\n        exemplars: [5]\n"
        ),
        vec![
            "topics.0.knowledge_points.0: Input should be a valid dictionary or instance of \
             KnowledgePoint",
            "topics.0.knowledge_points.1.exemplars.0: Input should be a valid dictionary or \
             instance of Exemplar",
            "topics.0.diagnostic_exemplar: Input should be a valid dictionary or instance of \
             Exemplar",
            "topics.0.anki_seeds.0: Input should be a valid dictionary or instance of AnkiSeed",
        ]
    );
}

/// An extra key of any scalar type is named the way Python names it.
#[test]
fn an_extra_key_of_any_type_is_named() {
    let body = "unit: u\ncourse: c\nmodule: M\ntrue: 1\n1: x\n~: x\n? [a]\n: x\n";
    assert_eq!(
        unit_findings("keys", body)
            .into_iter()
            .map(|(_, message)| message)
            .collect::<Vec<String>>(),
        vec![
            "true: Extra inputs are not permitted",
            "1: Extra inputs are not permitted",
            "None: Extra inputs are not permitted",
            ": Extra inputs are not permitted",
        ]
    );
}

/// A duplicate key inside a flow mapping has no line the scan reads, and a
/// root key that starts a longer key is not the repeated key.
#[test]
fn a_duplicate_key_the_scan_cannot_place_reads_line_unknown() {
    assert_eq!(
        catalog_messages("dup-flow", "{courses: [], courses: []}\n"),
        vec!["courses.yaml: duplicate mapping key 'courses' at line ?"]
    );
    assert_eq!(
        catalog_messages("dup-prefix", "courses: []\ncourses_extra: 1\ncourses: []\n"),
        vec!["courses.yaml: duplicate mapping key 'courses' at line 3"]
    );
}

/// A float with no digit after the point is a numeric form 2.0 refuses.
#[test]
fn a_float_with_no_fraction_digits_is_a_refused_form() {
    assert_eq!(
        topic_messages("point", "    difficulty: 1.\n"),
        vec![
            "c/00.yaml:6: numeric literal form '1.' is not accepted; write a plain decimal number"
        ]
    );
}
