//! Explicit form, multiplicity, interval endpoints, and malformed-input checks.
#![allow(clippy::unwrap_used)]

use cadus_core::answer::{AnswerContract, AnswerPart, NumericForm, Outcome, check_contract};

fn decide(policy: &AnswerContract, expected: &str, learner: &str, correct: bool) {
    let outcome = check_contract(expected, learner, policy.clone());
    assert!(
        matches!(&outcome, Outcome::Decided(verdict) if verdict.correct == correct),
        "{expected} vs {learner}: {outcome:?}"
    );
}

#[test]
fn numeric_forms_preserve_value_and_required_representation() {
    let fraction = AnswerContract::RequiredForm {
        form: NumericForm::ReducedFraction,
    };
    decide(&fraction, "1/2", r"\frac{1}{2}", true);
    decide(&fraction, "-1/2", "-1/2", true);
    for learner in ["2/4", "0.5", "1/-2", "1/2 + 0"] {
        decide(&fraction, "1/2", learner, false);
    }
    assert!(fraction.validate_expected("2/4").is_err());
    assert!(matches!(
        check_contract("1/2", "?", fraction.clone()),
        Outcome::Undecidable(_)
    ));
    let integer = AnswerContract::RequiredForm {
        form: NumericForm::Integer,
    };
    decide(&integer, "2", "2.0", false);
    decide(&integer, "2", "4/2", false);
    let decimal = AnswerContract::RequiredForm {
        form: NumericForm::Decimal,
    };
    decide(&decimal, "0.5", "0.50", true);
    decide(&decimal, "0.5", "1/2", false);
    decide(&decimal, "0.5", "0.49", false);
}

#[test]
fn required_single_power_preserves_one_literal_base_and_exponent() {
    let policy = AnswerContract::RequiredSinglePower;
    assert_eq!(
        serde_json::to_string(&policy).unwrap(),
        r#"{"kind":"required_single_power"}"#
    );
    assert!(
        serde_json::from_str::<AnswerContract>(r#"{"kind":"required_single_power","base":"2"}"#)
            .is_err()
    );
    for (expected, learner) in [
        ("2^7", "2^7"),
        ("2^7", "2^{7}"),
        ("2^7", "(2) ^ (7)"),
        ("(1/2)^3", r"(\frac{1}{2})^{3}"),
        ("7^1", "7^1"),
        ("7^0", "7^0"),
        ("0^3", "0^3"),
        ("1^8", "1^8"),
        ("(-2)^3", "(-2)^3"),
        ("2^(-3)", "2^{-3}"),
    ] {
        decide(&policy, expected, learner, true);
    }
    for (expected, learner) in [
        ("2^7", "128"),
        ("2^7", "2^3 * 2^4"),
        ("2^7", "1 * 2^7"),
        ("2^7", "(2^1)^7"),
        ("2^7", "(1+1)^7"),
        ("2^7", "(4/2)^7"),
        ("2^6", "(2^2)^3"),
        ("2^6", "4^3"),
        ("2^7", "3^7"),
        ("2^7", "2^6"),
        ("7^1", "7"),
        ("7^0", "1"),
        ("(-2)^3", "-2^3"),
    ] {
        decide(&policy, expected, learner, false);
    }
    for invalid in ["128", "(2^1)^7", "(1+1)^7", "(4/2)^7"] {
        assert!(policy.validate_expected(invalid).is_err(), "{invalid}");
    }
    for (expected, learner) in [("2^7", "2^"), ("0^0", "0^0"), ("0^(-1)", "0^(-1)")] {
        assert!(matches!(
            check_contract(expected, learner, policy.clone()),
            Outcome::Undecidable(_)
        ));
    }
}

fn list(ordered: bool, member: AnswerContract) -> AnswerContract {
    AnswerContract::List {
        ordered,
        member: Box::new(member),
    }
}

#[test]
fn homogeneous_lists_separate_order_and_multiplicity() {
    let ordered = list(true, AnswerContract::Exact);
    decide(&ordered, "13 and 14", "[13,14]", true);
    decide(&ordered, "13 and 14", "14 and 13", false);
    decide(&ordered, "13 and 14", "13 and 14 and 14", false);
    decide(
        &list(false, AnswerContract::Exact),
        "[1,2,2]",
        "2 and 1 and 2",
        true,
    );
    decide(
        &list(false, AnswerContract::Exact),
        "[1,2,2]",
        "[1,2]",
        false,
    );
    decide(
        &list(true, AnswerContract::Coordinates { arity: 2 }),
        "[(1,2), (3,4)]",
        "[(1,2),(3,4)]",
        true,
    );
    let approximate = list(true, AnswerContract::Approx { decimals: 2 });
    decide(&approximate, "1/3 and 2/3", "0.33 and 0.67", true);
    decide(&approximate, "1/3 and 2/3", "0.33 and 0.66", false);
}

#[test]
fn malformed_lists_and_unsupported_member_policies_are_refused() {
    let policy = list(true, AnswerContract::Exact);
    for learner in ["[1,]", "1 and ", "[(1,2]", "[]", "garbage"] {
        assert!(matches!(
            check_contract("1 and 2", learner, policy.clone()),
            Outcome::Undecidable(_)
        ));
    }
    assert!(
        list(
            false,
            AnswerContract::Tolerance {
                tolerance: "1/10".into()
            }
        )
        .validate()
        .is_err()
    );
    assert!(list(true, AnswerContract::None).validate().is_err());
    assert!(
        list(false, AnswerContract::RequiredInequalityNotation)
            .validate()
            .is_err()
    );
    assert!(
        list(false, AnswerContract::RequiredSinglePower)
            .validate()
            .is_err()
    );
    assert!(
        list(false, AnswerContract::RequiredNormalizedScientificNotation)
            .validate()
            .is_err()
    );
    assert!(
        list(true, list(true, AnswerContract::Exact))
            .validate()
            .is_err()
    );
    assert!(policy.validate_expected(&vec!["1"; 33].join(", ")).is_err());
}

#[test]
fn inequality_unions_normalize_overlap_and_preserve_holes() {
    let policy = AnswerContract::InequalityUnion;
    for (expected, learner) in [
        ("x < 2 or x > 3", "3 < x or 2 > x"),
        ("0 < x < 2 or 1 < x < 3", "0 < x < 3"),
        ("x < 0 or x >= 0", "x < 100 or x >= 100"),
        ("0 <= x <= 1 or 1 < x < 2", "0 <= x < 2"),
        ("x < 1 or x < 2", "x < 2"),
    ] {
        decide(&policy, expected, learner, true);
    }
    for (expected, learner) in [
        ("x < 0 or x > 0", "x < 0 or x >= 0"),
        ("x < 2 or x > 3", "x <= 2 or x > 3"),
        ("x < 2 or x > 3", "y < 2 or y > 3"),
        ("x < 2", "X < 2"),
        ("0 < x < 1 or 1 < x < 2", "0 < x < 2"),
        ("x < 1/3", "x < 0.333333333333333333"),
    ] {
        decide(&policy, expected, learner, false);
    }
}

#[test]
fn required_inequality_notation_matches_the_authored_output_form() {
    let policy = AnswerContract::RequiredInequalityNotation;
    assert_eq!(
        serde_json::to_string(&policy).unwrap(),
        r#"{"kind":"required_inequality_notation"}"#
    );
    assert!(
        serde_json::from_str::<AnswerContract>(
            r#"{"kind":"required_inequality_notation","form":"interval"}"#
        )
        .is_err()
    );
    for (expected, learner) in [
        ("(-∞, -2) ∪ [3, ∞)", "[3, ∞) ∪ (-∞, -2)"),
        ("x < -2 or x >= 3", "3 <= x or -2 > x"),
        ("(-∞, 4]", "(-∞, 4]"),
        ("x > 4", "4 < x"),
        ("(-∞, 4]", r"\left(-\infty,4\right]"),
        (
            "(-∞, -2) ∪ [3, ∞)",
            r"\left(-\infty,-2\right)\cup\left[3,\infty\right)",
        ),
        ("x <= 4", r"x\le 4"),
        ("x < -2 or x >= 3", r"x<-2\lor x\ge3"),
    ] {
        decide(&policy, expected, learner, true);
    }
    for (expected, learner) in [
        ("(-∞, -23)", "x < -23"),
        ("x < -29", "(-∞, -29)"),
        ("(-∞, -19) ∪ (13, ∞)", "x < -19 or x > 13"),
        ("x <= -17 or x > 11", "(-∞, -17] ∪ (11, ∞)"),
        ("(-∞, 4]", r"x\le 4"),
        ("x <= 4", r"\left(-\infty,4\right]"),
        ("(-∞, -2) ∪ [3, ∞)", "(-∞, -3) ∪ [3, ∞)"),
        ("(-∞, -2) ∪ [3, ∞)", "(-∞, -2] ∪ [3, ∞)"),
        ("(-∞, -2) ∪ [3, ∞)", "(-∞, -2)"),
        ("x < -2 or x >= 3", "y < -2 or y >= 3"),
    ] {
        decide(&policy, expected, learner, false);
    }
    assert!(policy.validate_expected("x < 2").is_ok());
    assert!(policy.validate_expected("(-∞, 2)").is_ok());
    assert!(matches!(
        check_contract("(-∞, 2)", "(-∞, 2", policy.clone()),
        Outcome::Undecidable(_)
    ));
    assert!(matches!(
        check_contract("[1, 4]", "[1, 4] ∪ [3, 2]", policy),
        Outcome::Undecidable(_)
    ));
}

#[test]
fn inequality_unions_refuse_nonnumeric_and_mixed_unknown_boundaries() {
    let policy = AnswerContract::InequalityUnion;
    for text in [
        "x < sqrt(2)",
        "x < 1 or y > 2",
        "x < 1 or",
        "x=2 or x=3",
        "x < y",
        "x < 1/0",
        r"\leftover(-∞, 2)",
        r"x\less 2",
        "[3, 2]",
        "(3, 3)",
    ] {
        assert!(policy.validate_expected(text).is_err(), "{text}");
    }
    assert!(
        policy
            .validate_expected(&vec!["x < 1"; 17].join(" or "))
            .is_err()
    );
    for policy in [
        policy,
        AnswerContract::RequiredInequalityNotation,
        list(true, AnswerContract::Exact),
        AnswerContract::RequiredForm {
            form: NumericForm::ReducedFraction,
        },
        AnswerContract::RequiredNormalizedScientificNotation,
    ] {
        assert_eq!(
            serde_json::from_str::<AnswerContract>(&serde_json::to_string(&policy).unwrap())
                .unwrap(),
            policy
        );
    }
}

#[test]
fn required_normalized_scientific_notation_preserves_standard_form() {
    let policy = AnswerContract::RequiredNormalizedScientificNotation;
    assert_eq!(
        serde_json::to_string(&policy).unwrap(),
        r#"{"kind":"required_normalized_scientific_notation"}"#
    );
    assert!(
        serde_json::from_str::<AnswerContract>(
            r#"{"kind":"required_normalized_scientific_notation","places":2}"#
        )
        .is_err()
    );
    for (expected, learner) in [
        ("6 x 10^7", "6 x 10^7"),
        ("6 x 10^7", "6 * 10^7"),
        ("6 x 10^7", r"6 \times 10^{7}"),
        ("6 x 10^7", r"6 \cdot 10^{7}"),
        ("6 x 10^7", "6 \u{00d7} 10^7"),
        ("6 x 10^7", "6*10**7"),
        ("6 x 10^7", "(6) * (10^7)"),
        ("6 x 10^7", "+6 * 10^+7"),
        ("6 x 10^7", "6 * 10^(+7)"),
        ("6 x 10^-3", r"6 \times 10^{-3}"),
        ("-2.5 x 10^-4", "-2.50 * 10^(-4)"),
        ("6 x 10^0", "6 * 10^0"),
    ] {
        decide(&policy, expected, learner, true);
    }
    for (expected, learner) in [
        ("6 x 10^7", "7 x 10^7"),
        ("6 x 10^7", "-6 x 10^7"),
        ("6 x 10^7", "60000000"),
        ("6 x 10^7", "60 x 10^6"),
        ("6 x 10^7", "0.6 x 10^8"),
        ("6 x 10^7", "10 x 10^6"),
        ("6 x 10^7", "0 x 10^7"),
        ("6 x 10^7", "(2 x 10^3)*(3 x 10^4)"),
        ("6 x 10^7", "(3+3)*10^7"),
        ("6 x 10^7", "2*3*10^7"),
        ("6 x 10^6", "6 x 100^3"),
        ("6 x 10^1", "6 x 10"),
        ("6 x 10^0", "6"),
        ("1.5 x 10^4", "3/2 x 10^4"),
        ("6 x 10^7", "10^7 * 6"),
    ] {
        decide(&policy, expected, learner, false);
    }
    for invalid in ["60000000", "60 x 10^6", "0 x 10^4", "3/2 x 10^4"] {
        assert!(policy.validate_expected(invalid).is_err(), "{invalid}");
    }
    assert!(matches!(
        check_contract("6 x 10^7", "6 x 10^(", policy.clone()),
        Outcome::Undecidable(_)
    ));
    assert!(matches!(
        check_contract("6 x 10^7", "6 x 10^(14/2)", policy.clone()),
        Outcome::Undecidable(_)
    ));
    assert!(matches!(
        check_contract("6 x 10^7", "6 x 10^(3+4)", policy.clone()),
        Outcome::Undecidable(_)
    ));
    assert!(
        policy
            .validate_expected(&format!("6 x 10^{}", "9".repeat(100)))
            .is_err()
    );
    assert!(
        policy
            .validate_expected(&format!("1.{} x 10^2", "0".repeat(1_001)))
            .is_err()
    );
    let ordered = list(true, policy.clone());
    decide(
        &ordered,
        "6 x 10^7 and 2.5 x 10^-4",
        "+6 * 10^+7 and 2.5 * 10^(-4)",
        true,
    );
    decide(
        &ordered,
        "6 x 10^7 and 2.5 x 10^-4",
        "60000000 and 2.5 x 10^-4",
        false,
    );
    let multipart = AnswerContract::Multipart {
        parts: vec![
            AnswerPart {
                name: "large".into(),
                contract: policy.clone(),
            },
            AnswerPart {
                name: "small".into(),
                contract: policy.clone(),
            },
        ],
    };
    decide(
        &multipart,
        "large = 6 x 10^7; small = 2.5 x 10^-4",
        "small = 2.5 * 10^-4; large = +6 * 10^(+7)",
        true,
    );
    decide(
        &multipart,
        "large = 6 x 10^7; small = 2.5 x 10^-4",
        "large = 60000000; small = 2.5 x 10^-4",
        false,
    );
    decide(&AnswerContract::Exact, "6 x 10^7", "60000000", true);
}
