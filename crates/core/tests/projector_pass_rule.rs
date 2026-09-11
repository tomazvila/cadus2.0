//! The lesson pass rule reads its configuration (D-F7 part 1, audit finding k).
//!
//! `lesson.kp_pass` parses into a [`PassRule`] at config load, and the knowledge-point
//! gate reads that rule. The default string keeps the 1.0 verdicts, and the config hash
//! of the default config does not move.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use cadus_core::config::{Config, ConfigError, LessonConfig};
use cadus_core::projector::{PassRule, kp_failed, kp_passed};

/// Build a rule from a string that the grammar accepts.
fn rule(text: &str) -> PassRule {
    PassRule::parse(text).expect("the rule parses")
}

/// The config text with another `kp_pass` string in it.
fn config_text(kp_pass: &str) -> String {
    let preimage = Config::default().hash_preimage().expect("the config dumps");
    let replaced = preimage.replace(
        r#""kp_pass":"2consec|3of4""#,
        &format!(r#""kp_pass":"{kp_pass}""#),
    );
    assert_ne!(replaced, preimage, "the replacement lands");
    replaced
}

// --------------------------------------------------------------------------- //
// The default string
// --------------------------------------------------------------------------- //

#[test]
fn the_default_string_parses_to_the_default_rule() {
    let config = Config::default();
    assert_eq!(config.lesson.kp_pass(), "2consec|3of4");
    assert_eq!(rule("2consec|3of4"), PassRule::default());
    assert_eq!(config.lesson.pass_rule(), &PassRule::default());
}

#[test]
fn the_default_config_hash_does_not_move() {
    // Trap T16. The parsed rule never serializes, so the preimage is the 1.0 field
    // set and the digest is the value the audit names.
    let config = Config::default();
    let preimage = config.hash_preimage().unwrap();
    assert!(!preimage.contains("pass_rule"), "{preimage}");
    assert_eq!(config.config_hash().unwrap(), "797575e985c12149");
}

// --------------------------------------------------------------------------- //
// The grammar
// --------------------------------------------------------------------------- //

#[test]
fn three_consec_reads_the_tail_and_nothing_else() {
    let three = rule("3consec");
    assert!(!three.passed(&[true, true]));
    assert!(three.passed(&[false, true, true, true]));
    // The three correct answers sit at the TAIL. A later miss undoes the pass.
    assert!(!three.passed(&[true, true, true, false]));
    assert!(!three.passed(&[true, false, true, true]));
    // The whole sequence is shorter than the term asks for.
    assert!(!three.passed(&[]));
}

#[test]
fn four_of_five_reads_the_first_five_and_nothing_else() {
    let four = rule("4of5");
    assert!(!four.passed(&[true, true, true, true]));
    assert!(four.passed(&[true, true, false, true, true]));
    assert!(!four.passed(&[true, true, false, false, true]));
    // A sixth correct answer is outside the head the term reads.
    assert!(!four.passed(&[true, true, false, false, true, true]));
    // One term, so a pair at the tail passes nothing.
    assert!(!four.passed(&[false, false, false, true, true]));
}

#[test]
fn two_consec_or_four_of_five_passes_on_either_term() {
    let either = rule("2consec|4of5");
    // The first term alone.
    assert!(either.passed(&[false, false, false, true, true]));
    // The second term alone.
    assert!(either.passed(&[true, true, true, false, true, false]));
    // Neither term.
    assert!(!either.passed(&[true, false, true, false, true, false]));
    // The order of the terms does not change the verdict.
    assert_eq!(
        rule("4of5|2consec").passed(&[false, false, false, true, true]),
        either.passed(&[false, false, false, true, true])
    );
}

#[test]
fn whitespace_around_a_term_is_not_significant() {
    assert_eq!(rule(" 2consec | 3of4 "), PassRule::default());
}

// --------------------------------------------------------------------------- //
// The configuration errors
// --------------------------------------------------------------------------- //

#[test]
fn an_empty_rule_is_a_configuration_error() {
    assert_eq!(PassRule::parse("").unwrap_err(), ConfigError::EmptyPassRule);
    assert_eq!(
        PassRule::parse("   ").unwrap_err(),
        ConfigError::EmptyPassRule
    );
    let message = ConfigError::EmptyPassRule.to_string();
    assert!(message.contains("`lesson.kp_pass` is empty"), "{message}");
}

#[test]
fn a_bad_term_names_itself_in_the_error() {
    for term in [
        "2consecutive",
        "consec",
        "0consec",
        "-2consec",
        "3of",
        "of4",
        "3of4of5",
        "5of4",
        "3 of 4",
        "99999999999999999999999999consec",
    ] {
        let error = PassRule::parse(term).unwrap_err();
        assert_eq!(
            error,
            ConfigError::PassRuleTerm {
                term: term.to_owned()
            },
            "{term}"
        );
        assert!(error.to_string().contains(term), "{term}");
    }
}

#[test]
fn a_bad_term_beside_a_good_one_fails_the_whole_rule() {
    let error = PassRule::parse("2consec|3of").unwrap_err();
    assert_eq!(
        error,
        ConfigError::PassRuleTerm {
            term: "3of".to_owned()
        }
    );
}

// --------------------------------------------------------------------------- //
// The config load
// --------------------------------------------------------------------------- //

#[test]
fn the_config_load_parses_the_rule_once() {
    let config: Config = serde_json::from_str(&config_text("3consec")).unwrap();
    assert_eq!(config.lesson.kp_pass(), "3consec");
    assert_eq!(config.lesson.pass_rule(), &rule("3consec"));
    // The gate reads the loaded rule, not the 1.0 default.
    assert!(!kp_passed(&[true, true], config.lesson.pass_rule()));
    assert!(kp_passed(&[true, true, true], config.lesson.pass_rule()));
    // `fail_after` is 5 and the loaded rule has not passed at five answers.
    assert!(kp_failed(&[true, true, false, true, true], &config));
    // A rule that moves moves the config hash.
    assert_ne!(config.config_hash().unwrap(), "797575e985c12149");
}

#[test]
fn the_config_load_refuses_a_bad_rule() {
    let error = serde_json::from_str::<Config>(&config_text("2consecutive"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("2consecutive"), "{error}");
}

#[test]
fn the_lesson_constructor_parses_the_rule() {
    let lesson = LessonConfig::new("2consec|3of4", 5, 1).expect("the rule parses");
    assert_eq!(lesson, LessonConfig::default());
    assert_eq!(
        LessonConfig::new("nope", 5, 1).unwrap_err(),
        ConfigError::PassRuleTerm {
            term: "nope".to_owned()
        }
    );
}
