//! Closed-label sign selection and bounded multipart preserve existing writers.
#![allow(clippy::unwrap_used)]
use cadus_core::{
    answer::{AnswerContract, canonical_form},
    template::{Bindings, Scalar, answer_for_contract, parse_answer_expr},
};
use serde_json::{Value, json};

fn label() -> AnswerContract {
    serde_json::from_value(json!({"kind":"label","options":[["downward"],["upward"]]})).unwrap()
}

fn bindings(a: i32) -> Bindings {
    [
        ("a", Scalar::Int(i64::from(a))),
        ("d", Scalar::Text("downward".into())),
        ("u", Scalar::Text("upward".into())),
        ("q", Scalar::Text("outside".into())),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.value()))
    .collect()
}

fn evaluated(expr: &str, a: i32, contract: &AnswerContract) -> Result<String, String> {
    let ast = parse_answer_expr(expr).map_err(|e| e.to_string())?;
    answer_for_contract(&ast, &bindings(a), Some(contract))
        .map(|a| a.text)
        .map_err(|e| e.to_string())
}

#[test]
fn all_signs_select_only_closed_labels_including_nested_choices() {
    for (a, expected) in [(-7, "downward"), (0, "upward"), (9, "upward")] {
        assert_eq!(
            evaluated("signcase(a,[d,u,u])", a, &label()).unwrap(),
            expected
        );
    }
    assert_eq!(
        evaluated("signcase(a,[d,signcase(a,[d,u,d]),u])", 0, &label()).unwrap(),
        "upward"
    );
    for expr in [
        "signcase(a,[d,u])",
        "signcase(a,(d,u))",
        "signcase(d,[d,u,u])",
        "signcase(a,[q,q,q])",
        "signcase(a,[0,1,2])",
        "signcase(a,[d,u,u,q])",
    ] {
        assert!(evaluated(expr, 1, &label()).is_err(), "{expr}");
    }
}

fn multipart(count: usize) -> AnswerContract {
    let parts: Vec<Value> = (0..count)
        .map(|i| {
            json!({"name":format!("p{i}"),
        "contract":{"kind":"exact"}})
        })
        .collect();
    serde_json::from_value(json!({"kind":"multipart","parts":parts})).unwrap()
}

#[test]
fn multipart_accepts_only_one_to_sixteen_exactly_matching_flat_parts() {
    for count in [1, 2, 3, 5, 16] {
        let args = (1..=count)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let expr = format!("multipart({args})");
        let result = evaluated(&expr, 1, &multipart(count)).unwrap();
        assert_eq!(result.split(';').count(), count);
        assert!(evaluated(&expr, 1, &multipart(if count == 1 { 2 } else { 1 })).is_err());
        assert!(
            canonical_form(&expr).is_err(),
            "template syntax leaked into learner grammar"
        );
    }
    for count in [0, 17] {
        let expr = format!("multipart({})", vec!["1"; count].join(","));
        assert!(parse_answer_expr(&expr).is_err());
    }
    assert!(parse_answer_expr("gcd(1,2,3)").is_err());
    assert!(parse_answer_expr("signcase(1,[1,2,3],4)").is_err());
}

#[test]
fn existing_numeric_and_structured_writers_keep_their_text() {
    let cases = [
        ("signcase(a,[11,12,13])", json!({"kind":"exact"}), "13"),
        ("a+3", json!({"kind":"exact"}), "10"),
        (
            "u",
            json!({"kind":"label","options":[["upward"],["downward"]]}),
            "upward",
        ),
        (
            "equalitylabel(a,7)",
            json!({"kind":"label","options":[["yes"],["no"]]}),
            "yes",
        ),
        (
            "divisibilitylabel(a,2)",
            json!({"kind":"label","options":[["yes"],["no"]]}),
            "no",
        ),
        (
            "primeclass(a)",
            json!({"kind":"label","options":[["prime"],["composite"],["neither"]]}),
            "prime",
        ),
        (
            "linearclass((a,3),(2,4))",
            json!({"kind":"label","options":[["one solution"],["all real numbers"],["no solution"]]}),
            "one solution",
        ),
        (
            "a",
            json!({"kind":"unit","unit":"m","quantity":"length"}),
            "7 m",
        ),
        ("a/2", json!({"kind":"reduced_ratio"}), "7:2"),
        (
            "quotientremainder(a,1)",
            json!({"kind":"quotient_remainder","divisor":3}),
            "7 R1",
        ),
    ];
    for (expr, contract, expected) in cases {
        let contract = serde_json::from_value(contract).unwrap();
        assert_eq!(evaluated(expr, 7, &contract).unwrap(), expected, "{expr}");
    }
}
