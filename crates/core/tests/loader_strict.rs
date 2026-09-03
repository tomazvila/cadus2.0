//! U1 acceptance, part 3: the numbers out of range, the file discovery of
//! hidden and linked units, the lax scalars 2.0 keeps, and the YAML 1.1 forms
//! 2.0 refuses (spec section 7, "2.0 strictness").
//!
//! Every message below is a literal from `docs/reference/curriculum-1.0-spec.md`
//! section 7 or from a run of the 1.0 loader over the fixture.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::os::unix::ffi::OsStringExt;

use cadus_core::curriculum::parse_curriculum;
use common::parsed::{first_topic_ids, triples, unit_names};
use common::paths::fixture;

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
    assert_eq!(
        unit_names(&parsed),
        vec![".00-hidden.yaml", "01-plain.yaml", "02-linked.yaml"]
    );
    assert_eq!(first_topic_ids(&parsed), vec!["hid", "plain", "linked"]);
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
    assert_eq!(
        first_topic_ids(&parsed),
        vec!["plain"],
        "the other file still loads"
    );
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
