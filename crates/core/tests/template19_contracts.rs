//! Typed ordering, solution-set and classification adversaries for template19.
#![allow(clippy::unwrap_used)]

use cadus_core::answer::{AnswerContract, Outcome, canonical_form, check_contract};
use cadus_core::template::{Bindings, Scalar, Value, answer_for_contract, parse_answer_expr};

fn write(expression: &str, contract: Option<&AnswerContract>) -> Result<String, String> {
    let ast = parse_answer_expr(expression).map_err(|error| error.reason.to_owned())?;
    answer_for_contract(&ast, &Bindings::new(), contract)
        .map(|answer| answer.text)
        .map_err(|error| error.to_string())
}

fn accepted(expected: &str, learner: &str, contract: &AnswerContract) -> bool {
    matches!(
        check_contract(expected, learner, contract.clone()),
        Outcome::Decided(verdict) if verdict.correct
    )
}

#[test]
fn ascending_writer_sorts_exact_values_under_its_specific_contract() {
    let policy = AnswerContract::AscendingChain;
    for (expression, expected) in [
        ("ascendingchain([2,-3,0])", "-3 < 0 < 2"),
        ("ascendingchain([1/3,-2/5,0])", "-2/5 < 0 < 1/3"),
        ("ascendingchain([0,-1])", "-1 < 0"),
    ] {
        let answer = write(expression, Some(&policy)).unwrap();
        assert_eq!(answer, expected);
        assert!(accepted(expected, &answer, &policy));
    }
    let boundary = (0..16)
        .rev()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    assert!(write(&format!("ascendingchain([{boundary}])"), Some(&policy)).is_ok());
}

#[test]
fn ascending_writer_refuses_wrong_policy_and_malformed_or_duplicate_inputs() {
    let expression = "ascendingchain([2,-3,0])";
    assert!(write(expression, None).is_err());
    for policy in [
        AnswerContract::Exact,
        AnswerContract::Set,
        AnswerContract::ReducedRatio,
    ] {
        assert!(write(expression, Some(&policy)).is_err());
    }
    let policy = AnswerContract::AscendingChain;
    for expression in [
        "ascendingchain()",
        "ascendingchain(2,-3,0)",
        "ascendingchain((2,-3,0))",
        "ascendingchain([1])",
        "ascendingchain([1,1])",
        "ascendingchain([1/2,0.5])",
        "ascendingchain([sqrt(2),0])",
        "ascendingchain([x,0])",
        "ascendingchain([1,2],[3,4])",
        "ascendingchain([0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16])",
    ] {
        assert!(write(expression, Some(&policy)).is_err(), "{expression}");
    }
}

#[test]
fn ordering_contract_rejects_wrong_order_missing_values_and_broader_relations() {
    let policy = AnswerContract::AscendingChain;
    assert!(accepted("-3 < 0 < 2", "-6/2 < 0 < 4/2", &policy));
    for learner in [
        "-3 < 2 < 0",
        "-3 < 0",
        "-3 < 0 < 2 < 4",
        "-3 <= 0 <= 2",
        "{-3,0,2}",
        "(-3,0,2)",
        "[-3,0,2]",
        "true",
        "2",
        "-3 < 0 < 0 < 2",
    ] {
        assert!(!accepted("-3 < 0 < 2", learner, &policy), "{learner}");
    }
}

#[test]
fn radical_sign_branch_keeps_empty_and_singleton_solution_sets() {
    let policy = AnswerContract::Set;
    for (rhs, expected) in [
        (-4, "{}"),
        (-2, "{}"),
        (0, "{-3/2}"),
        (2, "{1/2}"),
        (4, "{13/2}"),
    ] {
        let expression =
            format!("signcase(({rhs}),[{{}},{{(({rhs})**2-3)/2}},{{(({rhs})**2-3)/2}}])");
        let answer = write(&expression, Some(&policy)).unwrap();
        assert!(accepted(expected, &answer, &policy), "{rhs}: {answer}");
    }
    for learner in ["{13/2}", "{-13/2,13/2}", "{0}", "0", "all real numbers"] {
        assert!(
            !accepted("{}", learner, &policy),
            "extraneous negative-root answer: {learner}"
        );
    }
    for learner in ["{}", "{-1/2,1/2}", "{1/2,3}", "1/2", "(1/2)", "[1/2]"] {
        assert!(
            !accepted("{1/2}", learner, &policy),
            "broadened or untyped answer: {learner}"
        );
    }
    assert!(accepted("{}", "{ }", &policy));
    assert!(accepted("{1/2}", "{0.5}", &policy));
    for malformed in ["{", "}", "{,}", "{1,}", "{1"] {
        assert!(canonical_form(malformed).is_err(), "{malformed}");
    }
}

#[test]
fn triangle_signcase_computes_each_closed_choice_and_rejects_broad_labels() {
    let policy = AnswerContract::Label {
        options: ["acute", "right", "obtuse"]
            .map(|label| vec![label.to_owned()])
            .to_vec(),
    };
    let mut bindings = Bindings::from([
        ("u".to_owned(), Value::Text("acute".to_owned())),
        ("r".to_owned(), Value::Text("right".to_owned())),
        ("o".to_owned(), Value::Text("obtuse".to_owned())),
    ]);
    let ast = parse_answer_expr("signcase(25-b**2,[o,r,u])").unwrap();
    for (side, expected) in [(4, "acute"), (5, "right"), (6, "obtuse")] {
        bindings.insert("b".to_owned(), Scalar::Int(side).value());
        let answer = answer_for_contract(&ast, &bindings, Some(&policy)).unwrap();
        assert_eq!(answer.text, expected);
        for wrong in [
            "acute or right or obtuse",
            "triangle",
            "yes",
            "25",
            "right triangle",
        ] {
            assert!(!accepted(expected, wrong, &policy));
        }
        for other in ["acute", "right", "obtuse"] {
            assert_eq!(accepted(expected, other, &policy), expected == other);
        }
    }
}
