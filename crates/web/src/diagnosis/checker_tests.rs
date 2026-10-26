//! The unit tests of the checker rule.

use super::checker_diagnosis;
use cadus_core::config::default_error_tags;

fn hit(key: &str, answer: &str) -> Option<(Vec<String>, String)> {
    checker_diagnosis(key, answer, &default_error_tags())
        .map(|p| (p.error_tags, p.prose.unwrap_or_default()))
}

fn tag_of(key: &str, answer: &str) -> Option<String> {
    hit(key, answer).map(|(tags, _)| tags.first().cloned().unwrap_or_default())
}

#[test]
fn a_sign_flip_is_named() {
    assert_eq!(tag_of("-7", "7").as_deref(), Some("sign-error"));
    assert_eq!(tag_of("7", "-7").as_deref(), Some("sign-error"));
    assert_eq!(tag_of("0", "-0"), None);
}

#[test]
fn one_away_is_named() {
    let (tags, prose) = hit("12", "13").unwrap();
    assert_eq!(tags, ["arithmetic-slip"]);
    assert!(prose.contains("1 more"));
    let (tags, prose) = hit("12", "11").unwrap();
    assert_eq!(tags, ["arithmetic-slip"]);
    assert!(prose.contains("1 less"));
    assert_eq!(tag_of("13.5", "12.5").as_deref(), Some("arithmetic-slip"));
    assert_eq!(tag_of("13.5", "13"), None);
}

#[test]
fn a_cut_decimal_is_named() {
    assert_eq!(tag_of("3.16", "3.1").as_deref(), Some("arithmetic-slip"));
    assert_eq!(tag_of("2/3", "0.66").as_deref(), Some("arithmetic-slip"));
    assert_eq!(tag_of("3.16", "3.3"), None);
    // Rule D6-dec grades these as correct roundings, so the rule stays silent.
    assert_eq!(tag_of("3.14", "3.1"), None);
    assert_eq!(tag_of("1/3", "0.33"), None);
}

#[test]
fn a_reversed_inequality_is_named() {
    assert_eq!(tag_of("x < 3", "x > 3").as_deref(), Some("misread-problem"));
    assert_eq!(tag_of("x <= 3", "x > 3"), None);
    assert_eq!(tag_of("x < 3", "y > 3"), None);
}

#[test]
fn a_wrong_order_is_named() {
    assert_eq!(
        tag_of("(1, 2)", "(2, 1)").as_deref(),
        Some("misread-problem")
    );
    assert_eq!(
        tag_of("[1, 2, 3]", "[3, 1, 2]").as_deref(),
        Some("misread-problem")
    );
}

#[test]
fn one_wrong_member_is_named() {
    let (tags, prose) = hit("(1, 2, 3)", "(1, 5, 3)").unwrap();
    assert_eq!(tags, ["arithmetic-slip"]);
    assert!(prose.contains("second"));
    assert_eq!(tag_of("(1, 2, 3)", "(4, 5, 3)"), None);
    assert_eq!(tag_of("(1, 2)", "(1, 2, 3)"), None);
}

#[test]
fn nothing_else_is_named() {
    assert_eq!(tag_of("42", "17"), None);
    assert_eq!(tag_of("2*x", "3*x"), None);
    assert_eq!(tag_of("12", "banana"), None);
    assert_eq!(tag_of("12", "12"), None);
}

#[test]
fn prose_names_no_value() {
    let pairs = [
        ("-7", "7"),
        ("12", "13"),
        ("12", "11"),
        ("3.16", "3.1"),
        ("x < 3", "x > 3"),
        ("(1, 2)", "(2, 1)"),
        ("(1, 2, 3)", "(1, 5, 3)"),
    ];
    for (key, answer) in pairs {
        let (_, prose) = hit(key, answer).unwrap();
        assert!(!prose.contains(key), "{key} in {prose}");
        assert!(!prose.contains(answer), "{answer} in {prose}");
    }
}

#[test]
fn an_unknown_tag_is_dropped() {
    let found = checker_diagnosis("7", "-7", &[]).unwrap();
    assert!(found.error_tags.is_empty());
    assert!(found.prose.is_some_and(|p| !p.is_empty()));
}
