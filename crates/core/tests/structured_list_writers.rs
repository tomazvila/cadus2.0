//! Bounded structured-list answer writers retain count, order, and multiplicity.
#![allow(clippy::unwrap_used)]

use cadus_core::template::{Compiled, from_body};
use serde_json::json;

fn document(expression: &str, a: u32, b: Option<u32>) -> cadus_core::template::TemplateDoc {
    let mut params = json!({"a":{"kind":"choice","values":[a]}});
    let mut sample = json!({"a":a});
    if let Some(value) = b {
        params["b"] = json!({"kind":"choice","values":[value]});
        sample["b"] = json!(value);
    }
    let body = json!({
        "v":1,"topic_id":"structured-list","answer_kind":"expression",
        "answer_contract":{"kind":"list","ordered":true,"member":{"kind":"exact"}},
        "statement":"List the values for {a}.","params":params,"constraints":[],
        "answer_expr":expression,"solution_sketch":"Apply the requested construction.",
        "hints":["Use the definition."],"distractors":[],
        "samples":[{"params":sample,"expected":"1"}]
    });
    from_body(&body.to_string()).unwrap()
}

#[test]
fn list_writers_preserve_the_requested_mathematical_structure() {
    for (expression, a, b, expected) in [
        ("factorlist(a)", 24, None, "1, 2, 3, 4, 6, 8, 12, 24"),
        ("firstmultiples(a,b)", 7, Some(4), "7, 14, 21, 28"),
        ("primefactors(a)", 72, None, "2, 2, 2, 3, 3"),
        ("repeatedfactors(a,b)", 5, Some(3), "5, 5, 5"),
    ] {
        let doc = document(expression, a, b);
        let instance = Compiled::new(&doc)
            .unwrap()
            .instantiate(doc.samples[0].bindings())
            .unwrap();
        assert_eq!(instance.answer, expected);
    }
}

#[test]
fn list_writers_refuse_empty_or_oversized_constructions() {
    for (expression, a, b) in [
        ("factorlist(a)", 0, None),
        ("primefactors(a)", 1, None),
        ("firstmultiples(a,b)", 2, Some(33)),
        ("repeatedfactors(a,b)", 2, Some(0)),
    ] {
        let doc = document(expression, a, b);
        assert!(
            Compiled::new(&doc)
                .unwrap()
                .instantiate(doc.samples[0].bindings())
                .is_err()
        );
    }
}
