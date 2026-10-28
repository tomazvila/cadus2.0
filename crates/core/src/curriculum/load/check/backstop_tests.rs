//! The typed build behind the walk.

use super::*;
use crate::curriculum::model::Course;

/// The typed build behind the walk is a backstop: when the walk finds
/// nothing and the types still refuse the document, the finding names the
/// file as the location.
#[test]
fn a_document_the_types_refuse_after_a_silent_walk_is_a_schema_finding() {
    let document: Value = serde_norway::from_str("id: c\nname: C\norder: []\n").unwrap();
    let findings = validate::<Course, _>(&document, "courses.yaml", |_, _| {})
        .err()
        .unwrap_or_default();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "schema");
    assert_eq!(findings[0].file.as_deref(), Some("courses.yaml"));
    assert_eq!(
        findings[0].message,
        "courses.yaml: invalid type: sequence, expected an integer, or a float with no \
         fractional part"
    );
}
