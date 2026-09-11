//! Bidirectional interval conversion and adversarial fail-closed boundaries.
#![allow(clippy::unwrap_used)]
use cadus_core::{
    answer::{AnswerContract, Outcome, check_contract},
    template::{
        domain::Value,
        eval::{answer_for_contract, parse_answer_expr},
    },
};
use std::collections::BTreeMap;

fn output(expr: &str, source: &str, contract: AnswerContract) -> Result<String, String> {
    let ast = parse_answer_expr(expr).map_err(|e| e.to_string())?;
    let bindings = BTreeMap::from([("s".to_owned(), Value::Text(source.to_owned()))]);
    answer_for_contract(&ast, &bindings, Some(&contract))
        .map(|answer| answer.text)
        .map_err(|e| e.to_string())
}

#[test]
fn single_rays_convert_both_ways_with_exact_endpoint_membership() {
    for (op, interval) in [
        ("<", "(-∞, -7/3)"),
        ("<=", "(-∞, -7/3]"),
        (">", "(-7/3, ∞)"),
        (">=", "[-7/3, ∞)"),
    ] {
        let inequality = format!("x {op} -7/3");
        for (source, expected) in [(&inequality[..], interval), (interval, &inequality[..])] {
            let actual = output(
                "convertnotation(s)",
                source,
                AnswerContract::InequalityUnion,
            )
            .unwrap();
            assert_eq!(actual, expected);
            assert!(matches!(check_contract(source, &actual,
                AnswerContract::InequalityUnion), Outcome::Decided(v) if v.correct));
            assert!(
                matches!(check_contract(&actual.replace("-7/3", "-8/3"), &actual,
                AnswerContract::InequalityUnion), Outcome::Decided(v) if !v.correct)
            );
            assert!(output("convertnotation(s)", source, AnswerContract::Exact).is_err());
        }
    }
}

#[test]
fn unions_convert_both_ways_preserving_every_endpoint_combination() {
    for lc in 0..=1 {
        for hc in 0..=1 {
            let interval = format!(
                "(-∞, -7/3{} ∪ {}11/2, ∞)",
                if lc == 1 { "]" } else { ")" },
                if hc == 1 { "[" } else { "(" }
            );
            let inequality = format!(
                "x {} -7/3 or x {} 11/2",
                if lc == 1 { "<=" } else { "<" },
                if hc == 1 { ">=" } else { ">" }
            );
            for (source, expected) in [(&interval, &inequality), (&inequality, &interval)] {
                assert_eq!(
                    output(
                        "convertnotation(s)",
                        source,
                        AnswerContract::InequalityUnion
                    )
                    .unwrap(),
                    *expected
                );
            }
        }
    }
}

#[test]
fn conversion_writer_supports_required_output_notation() {
    for (source, expected) in [
        ("x < -23", "(-∞, -23)"),
        ("(-∞, -29)", "x < -29"),
        ("x < -19 or x > 13", "(-∞, -19) ∪ (13, ∞)"),
        ("(-∞, -17] ∪ (11, ∞)", "x <= -17 or x > 11"),
        (r"x\le -19", "(-∞, -19]"),
        (
            r"\left(-\infty,-2\right)\cup\left[3,\infty\right)",
            "x < -2 or x >= 3",
        ),
    ] {
        let contract = AnswerContract::RequiredInequalityNotation;
        let actual = output("convertnotation(s)", source, contract.clone()).unwrap();
        assert_eq!(actual, expected);
        assert!(matches!(
            check_contract(expected, &actual, contract.clone()),
            Outcome::Decided(verdict) if verdict.correct
        ));
        assert!(matches!(
            check_contract(expected, source, contract),
            Outcome::Decided(verdict) if !verdict.correct
        ));
    }
}

#[test]
fn malformed_shapes_domains_and_contracts_are_refused() {
    for source in [
        "",
        "[1,2]",
        "x < y",
        "y < 2",
        "x < 1 or y > 2",
        "x > sqrt(2)",
        "x > 1/0",
        "[∞, 1]",
        "(-∞, ∞)",
        "x < 2 or x > 1",
        "(3,2)",
        "(0,1) ∪ (2,3) ∪ (4,5)",
        "yes",
        "x=3",
        "x < 2; other=7",
    ] {
        assert!(
            output(
                "convertnotation(s)",
                source,
                AnswerContract::InequalityUnion
            )
            .is_err(),
            "{source}"
        );
    }
    for expr in [
        "convertnotation(1)",
        "convertnotation(x)",
        "convertnotation([s])",
        "convertnotation(s,s)",
        "convertnotation()",
    ] {
        assert!(
            output(expr, "x < 1", AnswerContract::InequalityUnion).is_err(),
            "{expr}"
        );
    }
    assert!(
        output(
            "convertnotation(s)",
            &" ".repeat(513),
            AnswerContract::InequalityUnion
        )
        .is_err()
    );
}

#[test]
fn boundary_styles_use_only_closed_mathematical_vocabulary() {
    for (name, labels) in [
        ("boundarycircle", ["open", "closed"]),
        ("boundarystyle", ["dashed", "solid"]),
    ] {
        let contract = AnswerContract::Label {
            options: labels.iter().map(|s| vec![s.to_string()]).collect(),
        };
        for (index, op) in ["<", "<=", ">", ">="].iter().enumerate() {
            assert_eq!(
                output(&format!("{name}(s)"), op, contract.clone()).unwrap(),
                labels[index % 2]
            );
            assert!(output(&format!("{name}(s)"), op, AnswerContract::Exact).is_err());
        }
        for op in ["=", "!=", "<= or >", "left", ""] {
            assert!(output(&format!("{name}(s)"), op, contract.clone()).is_err());
        }
        for expr in [
            format!("{name}([s])"),
            format!("{name}(s,s)"),
            format!("{name}(4)"),
        ] {
            assert!(output(&expr, "<", contract.clone()).is_err());
        }
    }
}
