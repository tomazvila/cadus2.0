//! Exact two-ray writer: endpoint strictness, contracts and bounded refusal.
#![allow(clippy::unwrap_used)]
use cadus_core::{
    answer::{AnswerContract, Outcome, check_contract},
    template::eval::{answer_for_contract, parse_answer_expr},
};
use std::collections::BTreeMap;

fn output(expr: &str, contract: AnswerContract) -> Result<String, String> {
    let ast = parse_answer_expr(expr).map_err(|e| format!("{e:?}"))?;
    answer_for_contract(&ast, &BTreeMap::new(), Some(&contract))
        .map(|a| a.text)
        .map_err(|e| e.to_string())
}

#[test]
fn two_rays_are_exact_and_preserve_all_endpoint_combinations() {
    for lc in 0..=1 {
        for hc in 0..=1 {
            let expr = format!("rayunion((-7/3,{lc}),(11/2,{hc}))");
            let text = output(&expr, AnswerContract::InequalityUnion).unwrap();
            let expected = format!(
                "x {} -7/3 or x {} 11/2",
                if lc == 1 { "<=" } else { "<" },
                if hc == 1 { ">=" } else { ">" }
            );
            assert_eq!(text, expected);
            let wrong = text.replace("-7/3", "-8/3");
            assert!(
                matches!(check_contract(&text,&wrong,AnswerContract::InequalityUnion),Outcome::Decided(v) if !v.correct)
            );
            assert!(output(&expr, AnswerContract::Exact).is_err());
        }
    }
}

#[test]
fn malformed_symbolic_degenerate_and_unbounded_inputs_fail_closed() {
    for expr in [
        "rayunion(1,2)",
        "rayunion((1,0),(1,1))",
        "rayunion((2,0),(1,0))",
        "rayunion((0,2),(1,0))",
        "rayunion((0,1/2),(1,0))",
        "rayunion((x,0),(1,0))",
        "rayunion((sqrt(2),0),(3,0))",
        "rayunion((1/0,0),(2,0))",
        "rayunion((2^10000,0),(3,0))",
        "rayunion((0,0))",
    ] {
        assert!(
            output(expr, AnswerContract::InequalityUnion).is_err(),
            "{expr}"
        );
    }
}

fn labels(options: &[&str]) -> AnswerContract {
    AnswerContract::Label {
        options: options.iter().map(|s| vec![(*s).to_owned()]).collect(),
    }
}

fn with_operator(expr: &str, op: &str, contract: &AnswerContract) -> Result<String, String> {
    let ast = parse_answer_expr(expr).map_err(|e| format!("{e:?}"))?;
    let bindings = BTreeMap::from([(
        "g".to_owned(),
        cadus_core::template::domain::Value::Text(op.to_owned()),
    )]);
    answer_for_contract(&ast, &bindings, Some(contract))
        .map(|a| a.text)
        .map_err(|e| e.to_string())
}

#[test]
fn classifications_cover_every_comparison_and_refuse_foreign_labels() {
    for (index, op) in ["<", "<=", ">", ">="].iter().enumerate() {
        for (expr, contract, expected) in [
            (
                "boundaryincluded(g)",
                labels(&["yes", "no"]),
                if index % 2 == 1 { "yes" } else { "no" },
            ),
            (
                "raydirection(g)",
                labels(&["left", "right"]),
                if index < 2 { "left" } else { "right" },
            ),
            (
                "negativeabs(-3/2,g)",
                labels(&["no solution", "all real numbers"]),
                if index < 2 {
                    "no solution"
                } else {
                    "all real numbers"
                },
            ),
        ] {
            assert_eq!(with_operator(expr, op, &contract).unwrap(), expected);
            assert_eq!(
                output(
                    &expr
                        .replace("(g)", &format!("({index})"))
                        .replace(",g)", &format!(",{index})")),
                    contract.clone()
                )
                .unwrap(),
                expected
            );
            assert!(with_operator(expr, op, &AnswerContract::Exact).is_err());
            assert!(with_operator(expr, op, &labels(&["foreign", "other"])).is_err());
            for bad in ["=", "!=", "<= or >", "left", ""] {
                assert!(with_operator(expr, bad, &contract).is_err());
            }
        }
    }
}

#[test]
fn negative_radius_and_branch_shape_fail_closed() {
    let contract = labels(&["no solution", "all real numbers"]);
    for expr in [
        "negativeabs(0,g)",
        "negativeabs(1,g)",
        "negativeabs(x,g)",
        "negativeabs(-sqrt(2),g)",
        "negativeabs(-1/0,g)",
        "negativeabs(-1,4)",
        "negativeabs(-1,1/2)",
        "negativeabs(-1,-1)",
        "negativeabs(-1)",
        "boundaryincluded(g,g)",
        "raydirection([g])",
    ] {
        assert!(with_operator(expr, ">", &contract).is_err(), "{expr}");
    }
}

#[test]
fn tuple_multipart_keeps_existing_coordinate_fields_and_checks_part_count() {
    let single: AnswerContract = serde_json::from_value(serde_json::json!({
        "kind":"multipart", "parts":[{"name":"point","contract":{"kind":"coordinates","arity":2}}]
    }))
    .unwrap();
    let actual = output("multipart((1,2))", single.clone()).unwrap();
    assert!(
        matches!(check_contract("point = (1,2)",&actual,single),Outcome::Decided(v) if v.correct)
    );
    let four: AnswerContract = serde_json::from_value(serde_json::json!({
        "kind":"multipart", "parts": (["a","b","c","d"].map(|name|serde_json::json!({
            "name":name,"contract":{"kind":"label","options":[["yes"],["no"]]}
        })))
    }))
    .unwrap();
    let actual = output("multipart((boundaryincluded(0),boundaryincluded(1),boundaryincluded(2),boundaryincluded(3)))",four.clone()).unwrap();
    assert_eq!(actual, "a = no; b = yes; c = no; d = yes");
    assert!(
        output(
            "multipart((boundaryincluded(0),boundaryincluded(1),boundaryincluded(2)))",
            four
        )
        .is_err()
    );
}

#[test]
fn existing_linearclass_branches_remain_available() {
    let contract = labels(&["one solution", "no solution", "all real numbers"]);
    for (expr, expected) in [
        ("linearclass((2,3),(4,5))", "one solution"),
        ("linearclass((2,3),(2,5))", "no solution"),
        ("linearclass((2,3),(2,3))", "all real numbers"),
    ] {
        assert_eq!(output(expr, contract.clone()).unwrap(), expected);
    }
}
