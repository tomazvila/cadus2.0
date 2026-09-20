//! The `function` contract: key validation and the verdict of each table row.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::contract::function::{FunctionSpec, grade};
use cadus_core::answer::{AnswerContract, Canon, Outcome, check_contract, normalize, parse};

const X: &str = r#"{"kind":"function","vars":["x"]}"#;
const X_CONSTANT: &str = r#"{"kind":"function","vars":["x"],"up_to_constant":true}"#;
const XY: &str = r#"{"kind":"function","vars":["x","y"]}"#;
const T: &str = r#"{"kind":"function","vars":["t"]}"#;
const FEW_POINTS: &str = "the authored function has fewer than six finite sample points";
/// A positive domain for the identities that hold on the positive numbers only
/// (D45 makes the default domain symmetric, where `2 ln(x)` is not finite).
const X_POSITIVE: &str = r#"{"kind":"function","vars":["x"],"domain":{"x":["1/4","11/4"]}}"#;

fn contract(doc: &str) -> AnswerContract {
    serde_json::from_str(doc).unwrap()
}

/// `Ok(correct)` for a verdict, `Err(reason)` for no verdict.
fn verdict(doc: &str, expected: &str, learner: &str) -> Result<bool, &'static str> {
    match check_contract(expected, learner, contract(doc)) {
        Outcome::Decided(verdict) => {
            assert!(!verdict.notation);
            Ok(verdict.correct)
        }
        Outcome::Undecidable(refusal) => Err(refusal.reason),
    }
}

fn key_refusal(doc: &str, key: &str) -> &'static str {
    contract(doc).validate_expected(key).unwrap_err().reason
}

#[test]
fn equal_formulas_in_a_different_form_are_correct() {
    for (expected, learner) in [
        ("sin(2x)", "2 sin(x) cos(x)"),
        ("tan(x)", "sin(x)/cos(x)"),
        ("sec(x)^2", "1/cos(x)^2"),
        ("sec(x)^2", "1 + tan(x)^2"),
        ("(x+2)/(x+1)^2", "1/(x+1) + 1/(x+1)^2"),
        ("x+1", "(x^2-1)/(x-1)"),
        ("ln(x+1)", "ln(abs(x+1))"),
        ("x/sqrt(x^2+9)", "x/sqrt(x^2+9)"),
        ("x/sqrt(x^2+9)", "x*(x^2+9)^(-1/2)"),
    ] {
        assert_eq!(
            verdict(X, expected, learner),
            Ok(true),
            "{expected} / {learner}"
        );
    }
    // `ln(x^2)` needs a positive domain: the symmetric default of D45 makes
    // `2 ln(x)` not finite at the negative points, and that is "wrong".
    assert_eq!(verdict(X_POSITIVE, "ln(x^2)", "2 ln(x)"), Ok(true));
    assert_eq!(verdict(X_POSITIVE, "ln(x^2)", "2*ln(abs(x))"), Ok(true));
    assert_eq!(verdict(T, "y = t*e^(-t)", "t e^(-t)"), Ok(true));
    assert_eq!(verdict(T, "y = t*e^(-t)", "y = t/e^t"), Ok(true));
    assert_eq!(verdict(T, "t*e^(-t)", "z = t/e^t"), Ok(true));
}

#[test]
fn different_formulas_are_wrong() {
    for (expected, learner) in [
        ("x^2", "x^2 + 0.001"),
        ("x^2", "x^2 + 7"),
        ("x^2", "t^2"),
        ("x^2", "x^2 + C"),
        ("x/sqrt(x^2+9)", "2*(x/sqrt(x^2+9)) + x"),
        ("0.3679", "e^(-1)"),
        // D43: the scale of rule (4) is the key value at that point only.
        ("x", "x + 10^20"),
        ("e^(10x)", "e^(10x) + x"),
    ] {
        assert_eq!(
            verdict(X, expected, learner),
            Ok(false),
            "{expected} / {learner}"
        );
    }
    assert_eq!(verdict(XY, "x*y^2", "x^2*y"), Ok(false));
    assert_eq!(verdict(XY, "x*y^2", "y*x*y"), Ok(true));
    // A blank answer is wrong before the arm of this contract.
    assert_eq!(verdict(X, "x^2", "   "), Ok(false));
}

#[test]
fn up_to_constant_accepts_one_constant_of_difference_only() {
    for learner in [
        "x^2/2 + c",
        "x^2/2",
        "x^2/2 + 7",
        "x^2/2 + K",
        "x^2/2 - k + 3",
    ] {
        assert_eq!(
            verdict(X_CONSTANT, "x^2/2 + C", learner),
            Ok(true),
            "{learner}"
        );
    }
    for learner in ["x^2/2 + x", "x^2", "x^2/2 + a", "x^2/2 + ln(C)"] {
        assert_eq!(
            verdict(X_CONSTANT, "x^2/2 + C", learner),
            Ok(false),
            "{learner}"
        );
    }
    // D34: the rule holds with each constant name at 0 and again at 1.
    for learner in ["x^2/2 + C^2", "x^2/2 - C", "x^2/2 + 2C"] {
        assert_eq!(
            verdict(X_CONSTANT, "x^2/2 + C", learner),
            Ok(true),
            "{learner}"
        );
    }
    for learner in ["x^2/2 + C*x", "C*x^2/2", "x^2/2 + k*x + c"] {
        assert_eq!(
            verdict(X_CONSTANT, "x^2/2 + C", learner),
            Ok(false),
            "{learner}"
        );
    }
    // A known limit: `ln(C*x)` is `ln(x)` plus a constant, but with the constant
    // at 0 the learner value is not finite, so rule (3) gives "wrong".
    assert_eq!(verdict(X_CONSTANT, "ln(x) + C", "ln(C*x)"), Ok(false));
    assert_eq!(verdict(X_CONSTANT, "ln(x) + C", "ln(2x) + c"), Ok(true));
    // A listed variable named `C` is a variable and not a constant.
    let listed = r#"{"kind":"function","vars":["x","C"],"up_to_constant":true}"#;
    assert_eq!(verdict(listed, "x + C", "x + C + 1"), Ok(true));
    assert_eq!(verdict(listed, "x + C", "x + 2C"), Ok(false));
}

#[test]
fn an_answer_that_is_not_one_formula_gets_no_verdict() {
    for learner in [
        "(1, 2)",
        "{x}",
        "x > 2",
        "[x, 1]",
        "y = (1, 2)",
        "1 < x < 2",
    ] {
        assert_eq!(
            verdict(X, "x/sqrt(x^2+9)", learner),
            Err("a function answer must be one expression"),
            "{learner}"
        );
    }
    let parser_reason = parse(&normalize("x/sqrt(x^2+9").source).unwrap_err().reason;
    assert_eq!(
        verdict(X, "x/sqrt(x^2+9)", "x/sqrt(x^2+9"),
        Err(parser_reason)
    );
    // A bad key gives no verdict for each answer.
    assert_eq!(verdict(X, "x/sqrt(x^2+9", "x"), Err(parser_reason));
    assert_eq!(verdict(X, "ln(x - 5)", "ln(x - 5)"), Err(FEW_POINTS));
}

#[test]
fn a_key_with_fewer_than_six_finite_points_is_refused() {
    assert_eq!(key_refusal(X, "ln(x - 5)"), FEW_POINTS);
    assert_eq!(key_refusal(X, "sqrt(x - 2)"), FEW_POINTS);
    assert_eq!(key_refusal(X, "1/(x - x)"), FEW_POINTS);
    let moved = r#"{"kind":"function","vars":["x"],"domain":{"x":["6","9"]}}"#;
    assert!(contract(moved).validate_expected("ln(x - 5)").is_ok());
    assert_eq!(
        verdict(moved, "ln(x - 5)", "ln(x-5)/2 + ln(sqrt(x-5))"),
        Ok(true)
    );
    // Two points that are not finite are permitted (0.40625 and 0.71875).
    assert!(contract(X).validate_expected("ln(x - 0.75)").is_ok());
    // Three are not.
    assert_eq!(key_refusal(X, "ln(x - 1.04)"), FEW_POINTS);
}

#[test]
fn the_other_key_refusals() {
    let one_expression = "the authored function must be one expression";
    for key in ["(1, 2)", "x > 2", "{x, 2}", "y = (x, 2)", "3 cm"] {
        assert_eq!(key_refusal(X, key), one_expression, "{key}");
    }
    let outside = "the authored function uses a name outside its variables";
    for key in ["t^2", "x*y", "x^2/2 + C", "y = x + a"] {
        assert_eq!(key_refusal(X, key), outside, "{key}");
    }
    assert!(contract(X_CONSTANT).validate_expected("x^2/2 + C").is_ok());
    assert_eq!(key_refusal(X_CONSTANT, "x^2/2 + D"), outside);
    // The label of a key is not a variable of the formula.
    assert!(contract(X).validate_expected("y = x^2").is_ok());
}

#[test]
fn a_key_with_log_or_with_e_notation_is_refused() {
    let log = "the authored function must use ln, because log has two readings";
    for key in ["log(x)", "x*log(x+1)", "\\log(x)", "LOG(x)"] {
        assert_eq!(key_refusal(X, key), log, "{key}");
    }
    let e_notation =
        "the authored function must use 10^(n), because the form 1e-5 reads as 1*e - 5";
    for key in ["1e-5 + x", "x + 2E3", "3e+2*x", "x*1.5e2"] {
        assert_eq!(key_refusal(X, key), e_notation, "{key}");
    }
    for key in ["ln(x)", "2e^x", "x + 3e", "e^(2x)", "2 e - 5x", "x*e"] {
        assert!(contract(X).validate_expected(key).is_ok(), "{key}");
    }
    // A learner can write `log`: the evaluator reads it as `ln`.
    assert_eq!(verdict(X, "ln(x)", "log(x)"), Ok(true));
}

#[test]
fn the_return_value_is_the_canonical_form_or_a_label() {
    let value = contract(X).validate_expected("y = x^2").unwrap();
    assert!(matches!(value, Canon::Assign { ref var, .. } if var == "y"));
    assert_eq!(
        contract(X).validate_expected("x*x").unwrap(),
        contract(X).validate_expected("x^2").unwrap()
    );
    // The canonical form refuses a quotient with a zero divisor; the key has 8 finite points.
    let key = "x + 0*ln(x)/ln(x)";
    if let Canon::Label(label) = contract(X).validate_expected(key).unwrap() {
        assert_eq!(label, normalize(key).string_key);
    }
}

#[test]
fn the_tolerance_is_relative_above_one_and_absolute_below_one() {
    // |a| = 1e6: the limit is 1e-3.
    assert_eq!(verdict(X, "1000000 + 0*x", "1000000.0005"), Ok(true));
    assert_eq!(verdict(X, "1000000 + 0*x", "1000000.002"), Ok(false));
    // |a| < 1: the limit is 1e-9.
    assert_eq!(verdict(X, "1/2", "0.5000000005"), Ok(true));
    assert_eq!(verdict(X, "1/2", "0.500000002"), Ok(false));
    // With `up_to_constant` the spread of the differences has the same limits.
    assert_eq!(
        verdict(X_CONSTANT, "1000000x", "1000000.0000000001x + 3"),
        Ok(true)
    );
    assert_eq!(
        verdict(X_CONSTANT, "1000000x", "1000000.01x + 3"),
        Ok(false)
    );
    assert_eq!(
        verdict(X_CONSTANT, "x/10", "x/10 + 0.0000000001x"),
        Ok(true)
    );
    assert_eq!(verdict(X_CONSTANT, "x/10", "x/10 + 0.00000001x"), Ok(false));
}

#[test]
fn a_point_where_the_key_is_not_finite_has_no_effect() {
    // The learner is not finite where the key is finite: wrong.
    assert_eq!(verdict(X, "x", "e^(ln(x-1)) + 1"), Ok(false));
    assert_eq!(verdict(X_CONSTANT, "x + C", "e^(ln(x-1)) + 1"), Ok(false));
    // The two are not finite at the same 2 points: correct.
    assert_eq!(verdict(X, "ln(x - 0.75)", "2*ln(sqrt(x - 0.75))"), Ok(true));
    // The learner is finite where the key is not: those points have no effect.
    assert_eq!(verdict(X, "sqrt(x - 0.75)^2", "x - 0.75"), Ok(true));
}

#[test]
fn grade_gives_wrong_for_a_key_with_fewer_than_six_finite_points() {
    let spec = FunctionSpec::new(&["x".to_string()], false, &Default::default()).unwrap();
    let tree = |text: &str| parse(&normalize(text).source).unwrap();
    // `validate_expected` refuses this key. `grade` is the last defense.
    let key = tree("ln(x - 5)");
    assert!(!grade(&key, &tree("2*ln(sqrt(x - 5))"), &spec).correct);
    assert!(!grade(&key, &tree("7"), &spec).correct);
    // Equal canonical forms are correct before the points are in use.
    assert!(grade(&key, &tree("ln(x-5)"), &spec).correct);
    assert!(grade(&tree("y = x^2"), &tree("z = x*x"), &spec).correct);
}

#[test]
fn a_domain_with_a_sample_point_near_zero_is_refused() {
    let reason = "a function contract domain must keep each sample point 1/100 or more from zero";
    for bounds in [
        r#"["1/100000000","1/10000000"]"#,
        r#"["-1/16","15/16"]"#,
        r#"["0","1/10"]"#,
        r#"["-1/1000","1/1000"]"#,
    ] {
        let doc = format!(r#"{{"kind":"function","vars":["x"],"domain":{{"x":{bounds}}}}}"#);
        let refusal = serde_json::from_str::<AnswerContract>(&doc).unwrap_err();
        assert!(refusal.to_string().contains(reason), "{bounds}: {refusal}");
    }
    // The cancellation key is correct on each domain that the rule accepts.
    for bounds in [r#"["1/100","2/100"]"#, r#"["0","1"]"#, r#"["-1","1"]"#] {
        let doc = format!(r#"{{"kind":"function","vars":["x"],"domain":{{"x":{bounds}}}}}"#);
        assert_eq!(
            verdict(&doc, "(1 - cos(x))/x^2", "2 sin(x/2)^2/x^2"),
            Ok(true),
            "{bounds}"
        );
    }
    assert_eq!(verdict(X_CONSTANT, "x^2/2 + C", "x^2/2"), Ok(true));
}

#[test]
fn a_pole_at_an_irrational_point_is_finite_in_f64() {
    // `cos(pi/2)` is 6.1e-17 in `f64`, not 0. The key is accepted; this is a
    // known limit of the variant (docs/DECISIONS.md).
    assert!(contract(X).validate_expected("1/cos(pi/2) + x").is_ok());
    assert!(contract(X).validate_expected("tan(pi/2)*x").is_ok());
    // A pole at a rational point is not finite, but it is not a sample point.
    assert!(contract(X).validate_expected("1/(x - 1)").is_ok());
}

#[test]
fn the_golden_function_keys_validate_and_refuse_their_mutant() {
    // The two `function` items of FLOW/spec/golden/calc-chain-rule.row.json.
    for key in ["x/sqrt(x^2+9)", "-3/(3x+1)^2"] {
        assert!(contract(X).validate_expected(key).is_ok(), "{key}");
        assert_eq!(verdict(X, key, key), Ok(true));
        assert_eq!(verdict(X, key, &format!("2*({key}) + x")), Ok(false));
    }
}

#[test]
fn a_multipart_and_an_ordered_list_grade_each_function_part() {
    let multipart = r#"{"kind":"multipart","parts":[{"name":"fx","contract":{"kind":"function","vars":["x","y"]}},{"name":"fy","contract":{"kind":"function","vars":["x","y"]}}]}"#;
    let key = "fx = 2xy; fy = x^2";
    assert!(contract(multipart).validate_expected(key).is_ok());
    assert_eq!(verdict(multipart, key, "fy = x*x; fx = y*2x"), Ok(true));
    assert_eq!(verdict(multipart, key, "fx = 2xy; fy = x^3"), Ok(false));
    assert_eq!(verdict(multipart, key, "fx = x^2; fy = 2xy"), Ok(false));
    assert_eq!(
        verdict(multipart, key, "fx = (1, 2); fy = x^2"),
        Err("a function answer must be one expression")
    );
    let list = r#"{"kind":"list","ordered":true,"member":{"kind":"function","vars":["x"]}}"#;
    assert!(contract(list).validate_expected("2x, x^2").is_ok());
    assert_eq!(verdict(list, "2x, x^2", "x + x, x*x"), Ok(true));
    assert_eq!(verdict(list, "2x, x^2", "x*x, x + x"), Ok(false));
}
