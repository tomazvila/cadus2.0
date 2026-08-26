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

use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use cadus_core::curriculum::{
    AnswerKind, Finding, ParseError, Parsed, RawCurriculum, Slug, Topic, load_raw_curriculum,
    parse_curriculum,
};

/// The curriculum tree of the repository (C5).
fn curriculum_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum")
}

/// One fixture tree under `crates/core/tests/fixtures/`.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The findings of a fixture, as `(code, message, file)` triples.
fn triples(parsed: &Parsed) -> Vec<(&str, &str, Option<&str>)> {
    parsed
        .findings
        .iter()
        .map(|f| (f.code.as_str(), f.message.as_str(), f.file.as_deref()))
        .collect()
}

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
    assert_eq!(exemplars, 6800, "exemplars");

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
// Types
// --------------------------------------------------------------------------- //

#[test]
fn a_slug_trims_the_outer_whitespace() {
    // Parity trap 6. The fixture writes " demo " and " foo " and "  kp1  ".
    let parsed = parse_curriculum(&fixture("slug-whitespace"));
    assert_eq!(triples(&parsed), Vec::new());
    let catalog = parsed.catalog.clone().expect("courses.yaml loads");
    assert_eq!(catalog.courses[0].id.as_str(), "demo");
    let unit = &parsed.units[0].unit;
    assert_eq!(unit.course.as_str(), "demo");
    assert_eq!(unit.topics[0].id.as_str(), "foo");
    assert_eq!(unit.topics[0].knowledge_points[0].id.as_str(), "kp1");

    assert_eq!(Slug::new(" foo ").unwrap().as_str(), "foo");
    assert_eq!(Slug::new(" foo ").unwrap().to_string(), "foo");
    assert_eq!(
        Slug::new("   ").unwrap_err().to_string(),
        "String should have at least 1 character"
    );
    assert_eq!(
        serde_norway::from_str::<Slug>("'  bar  '")
            .unwrap()
            .as_str(),
        "bar"
    );
    assert_eq!(
        serde_json::to_string(&Slug::new(" baz ").unwrap()).unwrap(),
        "\"baz\""
    );
}

#[test]
fn multi_step_parses_and_serializes_back_to_multi_step() {
    // Parity trap 5: the wire value carries a hyphen.
    let parsed = parse_curriculum(&fixture("multi-step"));
    assert_eq!(triples(&parsed), Vec::new());
    let topic = &parsed.units[0].unit.topics[0];
    assert_eq!(topic.answer_kind, AnswerKind::MultiStep);

    let json = serde_json::to_value(topic).unwrap();
    assert_eq!(json["answer_kind"], serde_json::json!("multi-step"));
    let yaml = serde_norway::to_string(topic).unwrap();
    assert!(
        yaml.contains("answer_kind: multi-step"),
        "yaml was {yaml:?}"
    );

    assert_eq!(
        serde_json::to_string(&AnswerKind::MultiStep).unwrap(),
        "\"multi-step\""
    );
    assert_eq!(
        serde_norway::from_str::<AnswerKind>("multi-step").unwrap(),
        AnswerKind::MultiStep
    );
    assert_eq!(AnswerKind::MultiStep.to_string(), "multi-step");
}

#[test]
fn the_model_types_deny_unknown_fields() {
    // The walk of the loader reports the extra key first; this pins the derive
    // behind it, so a dropped `deny_unknown_fields` cannot pass unseen.
    let text = "id: alpha\nname: Alpha\ndifficulty: 0.1\nanswer_kind: numeric\n\
                expected_time_secs: 60\nbogus: 1\n";
    let error = serde_norway::from_str::<Topic>(text).expect_err("the unknown key is rejected");
    assert!(
        error.to_string().starts_with("unknown field `bogus`"),
        "error was {error}"
    );
}

#[test]
fn a_finding_drops_its_empty_fields_in_json() {
    // 1.0 `Finding.as_dict` keeps code, message and fatal and drops the rest.
    let advisory = Finding::advisory("empty_course", "course hollow has no unit files");
    assert_eq!(
        serde_json::to_string(&advisory).unwrap(),
        r#"{"code":"empty_course","message":"course hollow has no unit files","fatal":false}"#
    );
    let full = Finding::new("cycle", "prerequisite cycle: a -> b -> a")
        .with_topic("a")
        .with_file("demo/01-basics.yaml")
        .with_context(vec!["a".to_owned(), "b".to_owned()]);
    assert_eq!(
        serde_json::to_string(&full).unwrap(),
        r#"{"code":"cycle","message":"prerequisite cycle: a -> b -> a","topic":"a","file":"demo/01-basics.yaml","context":["a","b"],"fatal":true}"#
    );
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
    let names: Vec<&str> = parsed.units.iter().map(|u| u.file_name.as_str()).collect();
    assert_eq!(
        names,
        vec!["01-a.yaml", "02-b.yaml", "10-c.yaml", "Z-upper.yaml"],
        "digits sort before an upper-case letter"
    );
    let ids: Vec<&str> = parsed
        .units
        .iter()
        .map(|u| u.unit.topics[0].id.as_str())
        .collect();
    assert_eq!(ids, vec!["first", "second", "third", "fourth"]);
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

// --------------------------------------------------------------------------- //
// Numbers out of range
// --------------------------------------------------------------------------- //

#[test]
fn the_non_finite_literals_are_rejected_numeric_forms() {
    // Review findings 3 to 6, 8, 10 and 14. `.nan`, `.inf` and `-.inf` are YAML
    // 1.1 spellings that no plain decimal number writes, so the numeric-literal
    // rule of spec section 7 refuses all three. 1.0 reads the three values and
    // reports the range instead (`Input should be less than or equal to 1` for
    // `.nan` and `.inf`, the lower bound for `-.inf`); the range messages stay
    // pinned on the overflow fixture below, where the spelling is accepted.
    let parsed = parse_curriculum(&fixture("loader-nonfinite"));
    assert_eq!(
        triples(&parsed),
        vec![
            (
                "schema",
                "c/00.yaml:7: numeric literal form '.nan' is not accepted; write a plain decimal \
                 number",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "c/00.yaml:12: numeric literal form '.inf' is not accepted; write a plain decimal \
                 number",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "c/00.yaml:17: numeric literal form '.nan' is not accepted; write a plain decimal \
                 number",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "c/00.yaml:20: numeric literal form '-.inf' is not accepted; write a plain \
                 decimal number",
                Some("c/00.yaml"),
            ),
        ]
    );
    assert!(parsed.units.is_empty(), "the file is dropped");
    assert!(parsed.findings[0].fatal);
}

#[test]
fn a_decimal_literal_that_overflows_f64_keeps_the_1_0_range_messages() {
    // Review findings 3 and 5. `1.0e+400` is a plain decimal float with a signed
    // exponent, so the numeric-literal rule accepts the spelling; the value
    // overflows to an infinity. The three messages below are the 1.0 output for
    // this fixture: pydantic reports the upper bound for `+inf` in a `difficulty`
    // field, the lower bound for `-inf`, and `Input should be a finite number`
    // in an `expected_time_secs` field.
    let parsed = parse_curriculum(&fixture("loader-overflow-float"));
    assert_eq!(
        triples(&parsed),
        vec![
            (
                "schema",
                "topics.0.difficulty: Input should be less than or equal to 1",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.1.difficulty: Input should be greater than or equal to 0",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.2.expected_time_secs: Input should be a finite number",
                Some("c/00.yaml"),
            ),
        ]
    );
    assert!(parsed.units.is_empty(), "the file is dropped");
}

// --------------------------------------------------------------------------- //
// File discovery: dot-prefixed and symlinked unit files
// --------------------------------------------------------------------------- //

#[test]
fn a_dot_prefixed_and_a_symlinked_unit_file_both_load() {
    // Review finding 2. 1.0 globs with `pathlib.Path.glob`, which returns a
    // dot-prefixed name and follows a symlink. A 1.0 load of this fixture reads
    // 3 unit files and the topics `hid`, `plain` and `linked`. A dot sorts
    // before a digit, so the hidden file is first.
    let parsed = parse_curriculum(&fixture("loader-hidden-and-symlinked-units"));
    assert_eq!(triples(&parsed), Vec::new());
    let names: Vec<&str> = parsed.units.iter().map(|u| u.file_name.as_str()).collect();
    assert_eq!(
        names,
        vec![".00-hidden.yaml", "01-plain.yaml", "02-linked.yaml"]
    );
    let ids: Vec<&str> = parsed
        .units
        .iter()
        .map(|u| u.unit.topics[0].id.as_str())
        .collect();
    assert_eq!(ids, vec!["hid", "plain", "linked"]);
}

// --------------------------------------------------------------------------- //
// The lax scalars of 1.0 that 2.0 keeps
// --------------------------------------------------------------------------- //

#[test]
fn a_whole_number_float_is_an_integer() {
    // Review finding 4. 1.0 pydantic validates in lax mode, so
    // `expected_time_secs: 60.0` is 60 and `order: 1.0` is 1. A 1.0 lint of this
    // fixture writes no parse-stage finding. Spec section 7, "2.0 strictness".
    let parsed = parse_curriculum(&fixture("loader-lax-numbers"));
    assert_eq!(triples(&parsed), Vec::new());
    let catalog = parsed.catalog.clone().expect("courses.yaml loads");
    assert_eq!(catalog.courses[0].order, 1);
    assert_eq!(parsed.units[0].unit.topics[0].expected_time_secs, 60);
}

#[test]
fn a_utf8_bom_is_accepted() {
    // Review finding 20. The BOM shifts the first key to column 3, which ends
    // the document for libyaml. Python removes it before the parser runs, so 1.0
    // reads the file with no finding. Spec section 7, "2.0 strictness".
    let parsed = parse_curriculum(&fixture("loader-bom"));
    assert_eq!(triples(&parsed), Vec::new());
    assert_eq!(parsed.units.len(), 1);
    assert_eq!(parsed.units[0].unit.topics[0].id.as_str(), "a");
}

#[test]
fn a_yaml_1_1_scalar_in_a_string_field_stays_a_string() {
    // Review finding 17. 1.0 refuses this fixture with three `schema` findings
    // (`topics.0.name`, `topics.1.name` and
    // `topics.1.diagnostic_exemplar.answer`: `Input should be a valid string`),
    // because PyYAML resolves `no` to a boolean and `2020-01-01` to a date. 2.0
    // reads YAML 1.2, where both stay the text the author wrote, and keeps them.
    // Spec section 7, "2.0 strictness".
    let parsed = parse_curriculum(&fixture("loader-yaml-1-1-strings"));
    assert_eq!(triples(&parsed), Vec::new());
    let topics = &parsed.units[0].unit.topics;
    assert_eq!(topics[0].name, "no");
    assert_eq!(topics[1].name, "2020-01-01");
    let exemplar = topics[1]
        .diagnostic_exemplar
        .as_ref()
        .expect("the fixture writes a diagnostic exemplar");
    assert_eq!(exemplar.answer, "no");
}

// --------------------------------------------------------------------------- //
// The YAML 1.1 forms 2.0 refuses
// --------------------------------------------------------------------------- //

#[test]
fn a_duplicate_mapping_key_is_a_schema_finding() {
    // Review finding 8. PyYAML keeps the last value and 1.0 loads the file with
    // no finding; `serde_norway` refuses the document. 2.0 names the form and
    // the line instead of passing the parser text through.
    let parsed = parse_curriculum(&fixture("loader-duplicate-key"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "schema",
            "c/00.yaml: duplicate mapping key 'name' at line 5",
            Some("c/00.yaml"),
        )]
    );
    assert!(parsed.findings[0].fatal);
}

#[test]
fn a_duplicate_key_in_the_root_mapping_names_its_line() {
    // Review finding 7. The parser writes a location for a duplicate inside a
    // nested mapping and none for one in the root mapping, so the root case
    // reads the line out of the text. The fixture repeats `topics:` at the root;
    // the second block starts on line 10. 1.0 keeps the last block and loads the
    // file with the topic `b`.
    let parsed = parse_curriculum(&fixture("loader-duplicate-root-key"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "schema",
            "c/00.yaml: duplicate mapping key 'topics' at line 10",
            Some("c/00.yaml"),
        )]
    );
    assert!(parsed.findings[0].fatal);
    assert!(parsed.units.is_empty(), "the file is dropped");
}

#[test]
fn a_unit_file_name_that_is_not_valid_utf8_is_a_yaml_finding() {
    // Review finding 13. Python `pathlib.Path.glob` decodes a directory name
    // with `surrogateescape`, so 1.0 matches `*.yaml` on the name and reads the
    // file: a 1.0 load of this tree reads 2 unit files and the topics `latin`
    // and `plain`. 2.0 holds every file name as a `String`, so it reports the
    // drop instead of skipping the file in silence. The tree is built here,
    // because git and the fixture tools would have to carry the raw byte.
    let root = std::env::temp_dir().join(format!("cadus-not-utf8-{}", std::process::id()));
    let course = root.join("c");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&course).expect("the temporary tree is writable");
    std::fs::write(
        root.join("courses.yaml"),
        "courses:\n- id: c\n  name: C\n  order: 1\n",
    )
    .expect("courses.yaml is writable");
    let unit = |id: &str| {
        format!(
            "unit: u\ncourse: c\nmodule: M\ntopics:\n  - id: {id}\n    name: A\n    \
             difficulty: 0.3\n    answer_kind: numeric\n    expected_time_secs: 30\n"
        )
    };
    std::fs::write(course.join("00-plain.yaml"), unit("plain")).expect("the unit file is writable");
    // `\xff` is no UTF-8 sequence at all, so the name has no `str` form.
    let mut raw = b"01-caf\xff.yaml".to_vec();
    let name = std::ffi::OsString::from_vec(std::mem::take(&mut raw));
    std::fs::write(course.join(&name), unit("latin")).expect("the unit file is writable");

    let parsed = parse_curriculum(&root);
    assert_eq!(
        triples(&parsed),
        vec![(
            "yaml",
            "c/01-caf\u{fffd}.yaml: file name is not valid UTF-8",
            Some("c/01-caf\u{fffd}.yaml"),
        )]
    );
    assert!(parsed.findings[0].fatal);
    let ids: Vec<&str> = parsed
        .units
        .iter()
        .map(|u| u.unit.topics[0].id.as_str())
        .collect();
    assert_eq!(ids, vec!["plain"], "the other file still loads");
    std::fs::remove_dir_all(&root).expect("the temporary tree is removable");
}

#[test]
fn an_integer_literal_outside_i64_is_one_message_on_every_path() {
    // Review finding 6. A literal above `i64::MAX` takes three paths inside the
    // parser: 19 or 20 digits fit a `u64`, 39 digits fit no integer type and
    // arrive as an `f64`, and 320 digits overflow the `f64` too and arrive as a
    // string. 1.0 reads all three as Python integers (18446744073709551616 and
    // up), so no pydantic text fits them; spec section 7 fixes one 2.0 message
    // for every one of them.
    let parsed = parse_curriculum(&fixture("loader-integer-forms"));
    assert_eq!(
        triples(&parsed),
        vec![
            (
                "schema",
                "topics.0.expected_time_secs: integer literal outside the 64-bit range",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.1.expected_time_secs: integer literal outside the 64-bit range",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.2.expected_time_secs: integer literal outside the 64-bit range",
                Some("c/00.yaml"),
            ),
        ]
    );
}

// --------------------------------------------------------------------------- //
// The numeric literal forms 2.0 refuses
// --------------------------------------------------------------------------- //

#[test]
fn every_rejected_numeric_literal_form_names_the_form_and_the_file() {
    // Review findings 3, 4, 5, 8, 10 and 14. 2.0 accepts a plain decimal integer
    // and a plain decimal float in `order`, `difficulty`, `expected_time_secs`
    // and `weight`, and refuses every other spelling (spec section 7, "2.0
    // strictness"). Each unit file below writes one refused spelling.
    //
    // 1.0 reads the tree the other way: it loads `08` as 8, `060` as 48, `0b101`
    // as 5, `0x1F` as 31, `1_000` as 1000, `1:30` as 90 and `0.7_5` as 0.75, and
    // it drops the three files that write `1e3`, `1.0e2` and `0o17` with
    // `Input should be a valid integer, unable to parse string as an integer`.
    // The two loaders therefore disagree on every file of this tree, which is
    // why 2.0 names the form and the fix in its own words.
    let parsed = parse_curriculum(&fixture("loader-numeric-forms"));
    let rejected = |file: &'static str, line: u32, raw: &str| {
        (
            "schema".to_owned(),
            format!(
                "c/{file}:{line}: numeric literal form '{raw}' is not accepted; \
                 write a plain decimal number"
            ),
            Some(format!("c/{file}")),
        )
    };
    let found: Vec<(String, String, Option<String>)> = parsed
        .findings
        .iter()
        .map(|f| (f.code.clone(), f.message.clone(), f.file.clone()))
        .collect();
    assert_eq!(
        found,
        vec![
            rejected("00-leading-zero-08.yaml", 9, "08"),
            rejected("01-leading-zero-060.yaml", 9, "060"),
            rejected("02-exponent-1e3.yaml", 9, "1e3"),
            rejected("03-exponent-1-0e2.yaml", 9, "1.0e2"),
            rejected("04-octal-0o17.yaml", 9, "0o17"),
            rejected("05-binary-0b101.yaml", 9, "0b101"),
            rejected("06-hex-0x1F.yaml", 9, "0x1F"),
            rejected("07-underscore-1_000.yaml", 9, "1_000"),
            rejected("08-underscore-float-0-7_5.yaml", 7, "0.7_5"),
            rejected("09-sexagesimal-1-30.yaml", 9, "1:30"),
            rejected("10-nan.yaml", 7, ".nan"),
            rejected("11-inf.yaml", 7, ".inf"),
            // The flow form `{id: a, weight: 0.7_5, key: false}`, with a comment
            // behind the closing brace.
            rejected("12-flow-weight.yaml", 16, "0.7_5"),
        ]
    );
    assert!(parsed.units.is_empty(), "every file is dropped");
    assert!(parsed.findings.iter().all(|finding| finding.fatal));
}

#[test]
fn a_rejected_numeric_literal_in_the_catalog_drops_the_catalog() {
    // The same rule on the `order` key of `courses.yaml`. 1.0 drops the whole
    // tree here too, with `courses.0.order: Input should be a valid integer,
    // unable to parse string as an integer`.
    let parsed = parse_curriculum(&fixture("loader-numeric-forms-order"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "schema",
            "courses.yaml:4: numeric literal form '1e3' is not accepted; write a plain decimal \
             number",
            Some("courses.yaml"),
        )]
    );
    assert!(parsed.catalog.is_none(), "the catalog is dropped");
    assert!(parsed.units.is_empty());
}

#[test]
fn the_accepted_numeric_forms_load_with_the_1_0_values() {
    // The other half of the rule: a plain decimal integer, a plain decimal float
    // and a plain decimal float with a signed exponent. A 1.0 load of this
    // fixture reads `difficulty` 0.15 and 0.75, `expected_time_secs` 60 and
    // 1200, `weight` 0.0 and `order` 2, and writes no parse-stage finding.
    let parsed = parse_curriculum(&fixture("loader-numeric-accepted"));
    assert_eq!(triples(&parsed), Vec::new());
    let catalog = parsed.catalog.clone().expect("courses.yaml loads");
    assert_eq!(catalog.courses[0].order, 2);
    let topics = &parsed.units[0].unit.topics;
    assert_eq!(topics[0].difficulty, 0.15);
    assert_eq!(topics[0].expected_time_secs, 60);
    assert_eq!(topics[1].difficulty, 0.75);
    assert_eq!(topics[1].expected_time_secs, 1200);
    assert_eq!(topics[1].prerequisites[0].weight, 0.0);
}

#[test]
fn a_numeric_value_on_a_continuation_line_is_not_scanned() {
    // The documented limit of the pre-scan (spec section 7, "2.0 strictness").
    // The fixture writes `difficulty:` on one line and `0.7_5` on the next, so
    // the line scan sees no value and the type check reports the scalar the
    // YAML 1.2 parser made of it. 1.0 reads the value 0.75 and loads the file.
    let parsed = parse_curriculum(&fixture("loader-numeric-continuation"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "schema",
            "topics.0.difficulty: Input should be a valid number, unable to parse string as a \
             number",
            Some("c/00.yaml"),
        )]
    );
}

#[test]
fn an_integer_past_the_unsigned_range_is_the_same_schema_finding() {
    // Review finding 21. `18446744073709551616` fits no `serde_norway` number,
    // so the parser refuses the whole document. The port reports the same code,
    // the same dotted location and the same message as the literal one step
    // below the range (the test above), and not the parser's own text.
    let parsed = parse_curriculum(&fixture("loader-huge-integer"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "schema",
            "topics.0.expected_time_secs: integer literal outside the 64-bit range",
            Some("c/00.yaml"),
        )]
    );
}

#[test]
fn a_merge_key_is_one_schema_finding() {
    // Review finding 19. PyYAML flattens `<<` into the mapping and 1.0 loads
    // both topics. 2.0 refuses the form, and reports it once: the merged fields
    // are absent only because of the merge key, so a "Field required" finding
    // for each of them names a phantom defect.
    let parsed = parse_curriculum(&fixture("loader-merge-key"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "schema",
            "topics.1.<<: merge keys are not accepted; write the fields out",
            Some("c/00.yaml"),
        )]
    );
}

#[test]
fn the_scalar_forms_1_0_coerces_carry_the_2_0_message() {
    // Review findings 16 and 27. 1.0 validates in pydantic lax mode, so it reads
    // `"0.3"` as 0.3, `"30"` as 30, `1` as true, `"true"` as true, and `true` as
    // 1.0 in a float field. 2.0 refuses each one, and refuses it in its own
    // words: pydantic's wording is reserved for the values pydantic also
    // refuses. The two messages for `topics.4.core` and `topics.5.core` below
    // are the 1.0 output for this fixture; 1.0 reports nothing for the other
    // five topics.
    let parsed = parse_curriculum(&fixture("loader-strict-scalars"));
    assert_eq!(
        triples(&parsed),
        vec![
            (
                "schema",
                "topics.0.difficulty: string '0.3' is not accepted; write the number unquoted",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.1.expected_time_secs: string '30' is not accepted; write 30",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.2.core: number 1 is not accepted; write true or false",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.3.core: string 'true' is not accepted; write true or false",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.4.core: Input should be a valid boolean, unable to interpret input",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.5.core: Input should be a valid boolean",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.6.difficulty: boolean true is not accepted; write a number",
                Some("c/00.yaml"),
            ),
        ]
    );
}

// --------------------------------------------------------------------------- //
// The checked-in tree writes no YAML 1.1 form
// --------------------------------------------------------------------------- //

/// Every `*.yaml` file under a directory, at any depth.
fn yaml_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).expect("the tree is readable") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "yaml")
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// The plain scalar one line writes, or `None` when the line writes a quoted
/// scalar, a block, a comment, or no value at all.
///
/// This walk is deliberately independent of the loader: it reads the bytes an
/// author wrote, so it sees the difference between `answer: no` and
/// `answer: "no"` that the parsed document no longer holds.
fn plain_scalar(line: &str) -> Option<&str> {
    let mut rest = line.trim();
    while let Some(tail) = rest.strip_prefix("- ") {
        rest = tail.trim_start();
    }
    if rest.starts_with('#') {
        return None;
    }
    let value = match rest.split_once(": ") {
        Some((_, value)) => value.trim(),
        None if rest.ends_with(':') => return None,
        None => rest,
    };
    if value.is_empty() || value.starts_with(['\'', '"', '|', '>', '&', '*', '#', '{', '[']) {
        return None;
    }
    Some(value)
}

/// True for a plain scalar that YAML 1.1 resolves to a boolean or an integer and
/// YAML 1.2 leaves as a string (spec section 7, "2.0 strictness").
fn is_yaml_1_1_form(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if matches!(lower.as_str(), "y" | "n" | "yes" | "no" | "on" | "off") {
        return true;
    }
    let digits = lower.trim_start_matches(['-', '+']);
    if digits.is_empty() {
        return false;
    }
    let octal = digits.len() > 1
        && digits.starts_with('0')
        && digits[1..].chars().all(|c| ('0'..='7').contains(&c));
    let underscored =
        digits.contains('_') && digits.chars().all(|c| c.is_ascii_digit() || c == '_');
    let sexagesimal =
        digits.contains(':') && digits.chars().all(|c| c.is_ascii_digit() || c == ':');
    octal || underscored || sexagesimal
}

#[test]
fn the_checked_in_tree_uses_no_yaml_1_1_form() {
    // The guard for the pinned choice of spec section 7, "2.0 strictness": the
    // tree must load the same way under 1.0 and under 2.0, so it may write no
    // form the two versions read differently. The positive control below runs
    // first, so a scan that stopped working cannot report a clean tree.
    for line in [
        "  core: yes",
        "  drill: Off",
        "  - N",
        "  expected_time_secs: 030",
        "  expected_time_secs: 1_200",
        "  expected_time_secs: 1:30",
    ] {
        let value = plain_scalar(line).expect("the line writes a plain scalar");
        assert!(is_yaml_1_1_form(value), "{line} writes a YAML 1.1 form");
    }
    for line in [
        "  answer: \"no\"",
        "  answer: 'yes'",
        "  core: true",
        "  expected_time_secs: 30",
        "  difficulty: 0.3",
        "  problem: |",
        "  # a comment",
    ] {
        let clean = plain_scalar(line).is_none_or(|value| !is_yaml_1_1_form(value));
        assert!(clean, "{line} writes no YAML 1.1 form");
    }

    let files = yaml_files(&curriculum_root());
    // Spec section 1: 88 unit files plus `courses.yaml`.
    assert_eq!(files.len(), 89, "YAML files in the tree");

    let mut hits: Vec<String> = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).expect("a unit file is readable");
        let name = path.display().to_string();
        assert!(!text.starts_with('\u{feff}'), "{name} starts with a BOM");
        for (index, line) in text.lines().enumerate() {
            let number = index + 1;
            if line.trim_start().starts_with("<<") {
                hits.push(format!("{name}:{number}: merge key"));
            }
            if let Some(value) = plain_scalar(line)
                && is_yaml_1_1_form(value)
            {
                hits.push(format!("{name}:{number}: {value}"));
            }
        }
    }
    assert_eq!(hits, Vec::<String>::new(), "YAML 1.1 forms in the tree");
}

/// True for the two numeric spellings 2.0 accepts: a plain decimal integer with
/// no leading zero, and a plain decimal float with an optional signed exponent.
///
/// This is a second, independent reading of the rule of spec section 7. It is
/// written against the text of the rule, not against the loader.
fn is_plain_decimal_form(value: &str) -> bool {
    let body = value.strip_prefix('-').unwrap_or(value);
    let digits = |text: &str| !text.is_empty() && text.chars().all(|c| c.is_ascii_digit());
    match body.split_once('.') {
        None => digits(body) && (body == "0" || !body.starts_with('0')),
        Some((whole, rest)) => {
            let (fraction, exponent) = match rest.split_once(['e', 'E']) {
                None => (rest, None),
                Some((fraction, exponent)) => (fraction, Some(exponent)),
            };
            let exponent_ok = match exponent {
                None => true,
                Some(exponent) => {
                    let signed = exponent.strip_prefix(['+', '-']);
                    signed.is_some_and(digits)
                }
            };
            digits(whole) && digits(fraction) && exponent_ok
        }
    }
}

#[test]
fn the_checked_in_tree_writes_only_plain_decimal_numbers() {
    // The guard for the numeric-literal rule of spec section 7, "2.0
    // strictness". Every other spelling of `order`, `difficulty`,
    // `expected_time_secs` and `weight` is a form the two YAML versions read
    // differently, or read as no number at all, so the tree may write none of
    // them. The positive control runs first, so a scan that stopped working
    // cannot report a clean tree.
    for form in ["1", "0", "-2", "0.3", "60.0", "1.5e-1", "-1.0e+30"] {
        assert!(is_plain_decimal_form(form), "{form} is a plain decimal");
    }
    for form in [
        "08", "060", "00", "1e3", "1.0e2", "0o17", "0b101", "0x1F", "1_000", "0.7_5", "1:30",
        ".nan", ".inf", "-.inf", "1.", ".5", "+30",
    ] {
        assert!(!is_plain_decimal_form(form), "{form} is no plain decimal");
    }

    const KEYS: [&str; 4] = ["order", "difficulty", "expected_time_secs", "weight"];
    let mut hits: Vec<String> = Vec::new();
    for path in yaml_files(&curriculum_root()) {
        let text = std::fs::read_to_string(&path).expect("a unit file is readable");
        let name = path.display().to_string();
        for (index, line) in text.lines().enumerate() {
            let line = line.split_once(" #").map_or(line, |(head, _)| head);
            let mut items: Vec<&str> =
                vec![line.trim_start().trim_start_matches("- ").trim_start()];
            if let Some(start) = line.find('{') {
                let flow = &line[start + 1..];
                let flow = flow.split_once('}').map_or(flow, |(head, _)| head);
                items.extend(flow.split(',').map(str::trim));
            }
            for item in items {
                for key in KEYS {
                    let Some(value) = item
                        .strip_prefix(key)
                        .and_then(|rest| rest.strip_prefix(':'))
                        .map(str::trim)
                    else {
                        continue;
                    };
                    if !is_plain_decimal_form(value) {
                        hits.push(format!("{name}:{}: {key}: {value}", index + 1));
                    }
                }
            }
        }
    }
    assert_eq!(hits, Vec::<String>::new(), "numeric forms in the tree");
}
