//! U1 acceptance, part 2: the curriculum types (C5, D2).
//!
//! Every value below is a literal from `docs/reference/curriculum-1.0-spec.md`
//! section 1 or from the 1.0 pydantic messages. No expected value comes from
//! the code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::curriculum::{
    AnkiType, AnswerKind, Course, Finding, Slug, Topic, parse_curriculum,
};
use common::parsed::triples;
use common::paths::fixture;

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
// The lax integer reader and the wire values
// --------------------------------------------------------------------------- //

/// The typed reader takes an integer, a negative integer, and a float with no
/// fractional part, and refuses every other number with the pydantic text.
#[test]
fn the_lax_integer_reader_accepts_whole_numbers_only() {
    let read = |order: &str| {
        serde_norway::from_str::<Course>(&format!("id: c\nname: C\norder: {order}\n"))
    };
    assert_eq!(read("7").map(|course| course.order).ok(), Some(7));
    assert_eq!(read("-3").map(|course| course.order).ok(), Some(-3));
    assert_eq!(read("60.0").map(|course| course.order).ok(), Some(60));
    let fraction = read("1.5").err().map(|error| error.to_string());
    assert_eq!(
        fraction.as_deref(),
        Some(
            "order: invalid value: floating point `1.5`, expected an integer, or a float with no fractional part at line 3 column 8"
        )
    );
    let huge = read("18446744073709551615")
        .err()
        .map(|error| error.to_string());
    assert_eq!(
        huge.as_deref(),
        Some(
            "order: invalid value: integer `18446744073709551615`, expected an integer, or a float with no fractional part at line 3 column 8"
        )
    );
}

/// Every wire value of the two enums reads back, and writes the same text.
#[test]
fn the_enum_wire_values_read_and_write_the_same_text() {
    assert_eq!(AnkiType::Basic.to_string(), "basic");
    assert_eq!(AnkiType::Cloze.to_string(), "cloze");
    assert_eq!(AnkiType::default(), AnkiType::Basic);
    assert_eq!(AnswerKind::Proof.to_string(), "proof");
    let slug = Slug::new(" kebab-case ").unwrap();
    assert_eq!(<Slug as AsRef<str>>::as_ref(&slug), "kebab-case");
    assert_eq!(slug.to_string(), "kebab-case");
}

/// A finding read from JSON without a `fatal` field is fatal, so an unset flag
/// never hides a dropped topic.
#[test]
fn a_finding_without_a_fatal_field_reads_as_fatal() {
    let finding: Finding =
        serde_json::from_str(r#"{"code":"schema","message":"x: Field required"}"#).unwrap();
    assert!(finding.fatal);
    assert_eq!(finding, Finding::new("schema", "x: Field required"));
    let advisory: Finding =
        serde_json::from_str(r#"{"code":"empty_course","message":"m","fatal":false}"#).unwrap();
    assert_eq!(advisory, Finding::advisory("empty_course", "m"));
}
