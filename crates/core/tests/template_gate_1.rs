//! M4 U2 acceptance: the verification gate (A2, C6, V2).
//!
//! Every expected value in this file is a LITERAL. No message, count, or space
//! is read back from the code under test, and none is built by calling the
//! formatter the gate calls. The literals come from four places:
//!
//! - `docs/reference/serving-1.0-spec.md` section 4, the 28 rows and their
//!   rejection text, and its four rejections run live against the 1.0 code;
//! - the same specification, section 9, the pinned literals and the rejection
//!   substrings the 1.0 tests assert;
//! - `/home/deploy/dev/cadus/tests/test_problem_templates.py`, the 1.0 fixture
//!   set, ported;
//! - arithmetic worked by hand, for the satisfying counts.
//!
//! The 1.0 header at `tests/test_problem_templates.py:1136-1140` records why the
//! literals matter: an evaluator mutated `BANK_TARGET` to 1 and `GATE_SAMPLES` to
//! 1 and the file stayed green, because three tests derived their expectations
//! from the constant.
//!
//! No test here calls a model, opens a socket, or reads a clock (T1, R3).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::gate::*;

// --------------------------------------------------------------------------
// 1. The happy path
// --------------------------------------------------------------------------
#[test]
fn a_well_formed_template_is_accepted_and_the_space_is_twelve() {
    let verified = accept(&body_with(&[]), AnswerKind::Numeric, &["49", "81"]);
    // `docs/reference/serving-1.0-spec.md` section 9: the space of `a` in 1..12
    // is 12, pinned at `tests/test_problem_templates.py:148`.
    assert_eq!(verified.space, SpaceSize::Exact(12));
    assert_eq!(verified.instances_checked, 12);
    assert!(verified.exhaustive);
}

#[test]
fn with_space_size_fills_the_field_the_gate_owns() {
    let verified = accept(&body_with(&[]), AnswerKind::Numeric, &["49", "81"]);
    let filled = with_space_size(&doc_of(&body_with(&[])), &verified);
    assert_eq!(filled.space_size, Some(SpaceSize::Exact(12)));
    assert_eq!(doc_of(&body_with(&[])).space_size, None);
}

#[test]
fn every_instance_answer_of_an_accepted_template_canonicalizes() {
    // The property V2 buys: every answer the gate lets through is one the M5
    // grade path decides. The gate refuses a document that breaks it; this walks
    // the accepted document and reads the answers back through the checker.
    let doc = doc_of(&body_with(&[]));
    let compiled = Compiled::new(&doc).expect("the fixture compiles");
    let mut rng = rng_from_seed(GATE_SEED);
    let mut answers = Vec::new();
    for bindings in compiled.candidates(&mut rng).expect("the stream builds") {
        let instance = compiled.instantiate(bindings).expect("the instance builds");
        assert_eq!(
            canonical_form(&instance.answer).expect("the answer canonicalizes"),
            instance.canon
        );
        answers.push(instance.answer);
    }
    answers.sort();
    assert_eq!(
        answers,
        vec![
            "1", "100", "121", "144", "16", "25", "36", "4", "49", "64", "81", "9"
        ]
    );
}

// --------------------------------------------------------------------------
// 2. The four rejections the specification ran live against the 1.0 code
// --------------------------------------------------------------------------
#[test]
fn live_rejection_one_the_exemplar_envelope_refuses_a_negative_instance() {
    // `a` in 59..70 and `b` fixed at 63 makes {'a': 59, 'b': 63} the first tuple
    // of the walk, and `a - b` answers -4 there.
    let rejection = reject_numeric(&live_rejection_one_body());
    assert_eq!(
        rejection.message,
        "instance {'a': 59, 'b': 63} answers '-4', but every authored answer for this knowledge point is non-negative — narrow the domains so no instance goes below zero"
    );
    assert_eq!(rejection.code, "envelope-sign");
}

#[test]
fn live_rejection_two_no_worked_sample_crosses_the_corners() {
    let rejection = reject_squares(&body_with(&[
        ("statement", r#""Compute ${a} \\times {b}$.""#),
        (
            "params",
            r#"{"a": {"kind": "int", "low": 2, "high": 5},
                "b": {"kind": "int", "low": 2, "high": 4}}"#,
        ),
        ("answer_expr", r#""a*b""#),
        ("solution_sketch", r#""Multiply ${a}$ by ${b}$.""#),
        ("hints", r#"["Which number goes into which column?"]"#),
        (
            "samples",
            r#"[{"params": {"a": 2, "b": 2}, "expected": "4"},
                {"params": {"a": 5, "b": 4}, "expected": "20"}]"#,
        ),
    ]));
    assert_eq!(
        rejection.message,
        "no worked sample crosses a and b — one of them at its low end WITH the other at its high end (a=2 with b=4, or a=5 with b=2). Matching corners are exactly where a swapped-operand expression looks right"
    );
    assert_eq!(rejection.code, "crossed-corner");
}

#[test]
fn live_rejection_three_no_worked_sample_uses_the_low_end() {
    let rejection = reject_squares(&body_with(&[(
        "samples",
        r#"[{"params": {"a": 12}, "expected": "144"},
            {"params": {"a": 7}, "expected": "49"}]"#,
    )]));
    assert_eq!(
        rejection.message,
        "no worked sample uses the low end of a (1) — the edges are where an expression stops being right"
    );
    assert_eq!(rejection.code, "edge-coverage");
}

#[test]
fn live_rejection_four_the_declared_domains_produce_only_five_problems() {
    let rejection = reject_squares(&five_problems_body());
    assert_eq!(
        rejection.message,
        "the declared domains produce only 5 distinct problem(s); at least 12 are needed for randomized values and for avoidance of a recently-served problem to mean anything (Hard Rule 4)"
    );
    assert_eq!(rejection.code, "space-floor");
}

// --------------------------------------------------------------------------
// 3. The 1.0 fixture set, ported as literals
// --------------------------------------------------------------------------
#[test]
fn the_1_0_gate_rejections_port_as_literals() {
    // `tests/test_problem_templates.py:160-196`, one row per parametrized case,
    // with the 2.0 message each one earns written out.
    let cases: [(&str, &str); 8] = [
        (
            r#"{"statement": "Compute ${b}^{{2}}$."}"#,
            "text uses undeclared parameters ['b']",
        ),
        (
            r#"{"statement": "Compute ${a}^{2}$."}"#,
            "text has an unescaped brace at index 13 ('{2}$.') — literal LaTeX braces must be doubled",
        ),
        (
            r#"{"samples": []}"#,
            "a template needs worked samples to verify it",
        ),
        (
            r#"{"params": {}}"#,
            "a template needs at least one parameter",
        ),
        (
            r#"{"params": {"a": {"kind": "int", "low": 1, "high": 1000000}}}"#,
            "int domain 1..1000000 exceeds MAX_DOMAIN_SIZE",
        ),
        (
            r#"{"params": {"a": {"kind": "int", "low": 9, "high": 2}}}"#,
            "int domain 9..2 is empty",
        ),
        (
            r#"{"answer_expr": "a + b"}"#,
            "answer_expr references unknown names ['b']",
        ),
        (
            r#"{"samples": [{"params": {"q": 7}, "expected": "49"}]}"#,
            "sample binds ['q'], template declares ['a']",
        ),
    ];
    for (overrides, expected) in cases {
        let parsed: serde_json::Value =
            serde_json::from_str(overrides).expect("the override reads");
        let object = parsed.as_object().expect("the override is an object");
        let pairs: Vec<(String, String)> = object
            .iter()
            .map(|(key, value)| (key.clone(), value.to_string()))
            .collect();
        let borrowed: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let rejection = reject_squares(&body_with(&borrowed));
        assert_eq!(rejection.message, expected, "override {overrides}");
    }
}

#[test]
fn the_exponent_bomb_exceeds_the_evaluation_bound() {
    // 1.0 reads `a**99999` and refuses it with "exceeds the evaluation bound"
    // (`tests/test_problem_templates.py:174`). The 2.0 parser holds the same
    // bound, so the refusal lands one check earlier, at the grammar row.
    let rejection = reject_squares(&body_with(&[("answer_expr", r#""a**99999""#)]));
    assert_eq!(
        rejection.message,
        "answer_expr 'a**99999' exceeds the evaluation bound (1000 is the largest exponent the grammar reads)"
    );
    assert_eq!(rejection.code, "grammar");
}

#[test]
fn an_answer_expression_outside_the_grammar_is_refused_with_a_2_0_message() {
    // 1.0 refuses `a**2 + os` at its name check. The M2 grammar has no
    // multi-letter variable, so 2.0 refuses it at the parse, which is the
    // stricter reading V2 asks for.
    let rejection = reject_squares(&body_with(&[("answer_expr", r#""a**2 + os""#)]));
    assert_eq!(
        rejection.message,
        "answer_expr 'a**2 + os' is outside the decidable grammar: a name that is not a function or variable"
    );
    assert_eq!(rejection.code, "grammar");
}

#[test]
fn a_sample_outside_its_own_domain_is_refused() {
    // `tests/test_problem_templates.py:451-458`.
    let rejection = reject_squares(&body_with(&[(
        "samples",
        r#"[{"params": {"a": 10000}, "expected": "100000000"}]"#,
    )]));
    assert_eq!(
        rejection.message,
        "sample 0 binds a=10000, which its own domain cannot produce — a sample outside the domain verifies nothing"
    );
    assert_eq!(rejection.code, "sample-domain");
}

#[test]
fn the_samples_must_agree_with_the_servers_own_evaluation() {
    // THE check (`tests/test_problem_templates.py:146-151`).
    let rejection = reject_squares(&body_with(&[("answer_expr", r#""a*2""#)]));
    assert_eq!(
        rejection.message,
        "answer_expr gives '2' for {'a': 1} but the sample claims '1' — the expression does not compute the stated answer"
    );
    assert_eq!(rejection.code, "sample-agreement");
}

#[test]
fn a_dead_parameter_is_refused() {
    // `tests/test_problem_templates.py:199-210`.
    let rejection = reject_squares(&body_with(&[(
        "params",
        r#"{"a": {"kind": "int", "low": 1, "high": 12},
            "spare": {"kind": "int", "low": 1, "high": 3}}"#,
    )]));
    assert_eq!(
        rejection.message,
        "parameters ['spare'] are declared but never used"
    );
    assert_eq!(rejection.code, "dead-parameter");
}

#[test]
fn a_parameter_may_not_shadow_a_function_name() {
    // `tests/test_problem_templates.py:229-249`, the six names it parametrizes.
    for name in ["pi", "sqrt", "E", "log", "Max", "factorial"] {
        let params = format!(r#"{{"{name}": {{"kind": "int", "low": 1, "high": 12}}}}"#);
        let statement = format!(r#""Compute ${{{name}}}$.""#);
        let rejection = reject_squares(&body_with(&[
            ("params", params.as_str()),
            ("statement", statement.as_str()),
            ("answer_expr", r#""a""#),
            ("solution_sketch", ""),
            ("samples", r#"[]"#),
        ]));
        assert_eq!(
            rejection.message,
            format!(
                "parameter name '{name}' collides with a SymPy function the answer expression may call"
            )
        );
        assert_eq!(rejection.code, "parameter-collision");
    }
}

#[test]
fn an_unknowns_name_is_refused_only_for_an_expression_answer() {
    // `tests/test_problem_templates.py:252-273`: `n` is the unknown a formula is
    // written in, and it is a plain parameter name for a numeric answer.
    let numeric = body_with(&[
        ("statement", r#""Compute ${n} + 1$.""#),
        ("params", r#"{"n": {"kind": "int", "low": 1, "high": 12}}"#),
        ("answer_expr", r#""n + 1""#),
        ("solution_sketch", r#""Add one.""#),
        ("hints", r#"["What comes after this number?"]"#),
        (
            "samples",
            r#"[{"params": {"n": 1}, "expected": "2"},
                {"params": {"n": 12}, "expected": "13"}]"#,
        ),
    ]);
    let verified = accept(&numeric, AnswerKind::Numeric, &["49", "81"]);
    assert_eq!(verified.space, SpaceSize::Exact(12));

    let expression = numeric.replace(
        r#""answer_kind": "numeric""#,
        r#""answer_kind": "expression""#,
    );
    let rejection = reject(&expression, AnswerKind::Expression, &["49", "81"]);
    assert_eq!(
        rejection.message,
        "parameter name 'n' collides with the unknown an expression answer is written in"
    );
    assert_eq!(rejection.code, "unknown-collision");
}

#[test]
fn an_unverifiable_answer_kind_can_have_no_template() {
    // `tests/test_problem_templates.py:320-326`.
    for (kind, written) in [
        (AnswerKind::MultiStep, "multi-step"),
        (AnswerKind::Proof, "proof"),
    ] {
        let rejection = reject(&body_with(&[]), kind, &["49", "81"]);
        assert_eq!(
            rejection.message,
            format!("answer kind {written} is not symbolically decidable")
        );
        assert_eq!(rejection.code, "answer-kind");
    }
}

#[test]
fn the_reserved_and_non_answer_name_sets_are_the_1_0_literals() {
    // `problem_templates.py:533-548` (`_ALLOWED_NAMES`, 45 names), plus the 2.0
    // constants and bounded evaluator functions. Written out in registry order,
    // preserving every historical entry and pinning the complete current set.
    assert_eq!(
        RESERVED_NAMES.to_vec(),
        "ascendingchain Abs And E Eq False Float Ge Gt ITE Integer Le Lt Max Min Ne Not Or Piecewise Rational S True abs atandeg binomial cancel divisibilitylabel equalitylabel linearclass relationform ceiling compounding cos e exp expequation expand excludepoint rayunion convertnotation symbol boundarycircle boundarystyle boundaryincluded raydirection negativeabs factor factorial factorlist false firstmultiples floor gcd lcm lowerbound ln log logequation max min multipart nsimplify pi primeclass primefactors powerform quarterextremum quotientremainder repeatedfactors sign signcase simplify sin sqrt tan together true trianglelaw upperbound"
            .split(' ')
            .collect::<Vec<&str>>()
    );
    for (name, _) in cadus_core::template::EVAL_FUNCTIONS {
        assert!(
            RESERVED_NAMES.contains(&name),
            "evaluator function {name:?} must be reserved from parameter shadowing"
        );
    }
    // `problem_templates.py:203-205`. `-oo` lexes as the token `oo`, so the
    // token set holds nine names and the Python set holds ten strings.
    assert_eq!(
        NON_ANSWERS.to_vec(),
        vec![
            "AccumBounds",
            "False",
            "I",
            "True",
            "false",
            "nan",
            "oo",
            "true",
            "zoo",
        ]
    );
    // `problem_templates.py:553`.
    assert_eq!(
        FREE_SYMBOLS.to_vec(),
        vec!["k", "n", "r", "t", "theta", "u", "v", "w", "x", "y", "z"]
    );
    assert_eq!(
        TEMPLATABLE_KINDS.to_vec(),
        vec![AnswerKind::Numeric, AnswerKind::Expression]
    );
}

#[test]
fn every_choice_must_appear_in_a_worked_sample() {
    // `tests/test_problem_templates.py:376-402`: `answer_expr = "a + b"` with
    // `op` a choice of `["+", "-"]` and every sample using `"+"` was accepted,
    // and then served a wrong answer for 81 of its 162 instances.
    let rejection = reject_squares(&body_with(&[
        ("statement", r#""Compute ${a} {op} {b}$.""#),
        (
            "params",
            r#"{"a": {"kind": "int", "low": 1, "high": 9},
                "b": {"kind": "int", "low": 1, "high": 9},
                "op": {"kind": "choice", "values": ["+", "-"]}}"#,
        ),
        ("answer_expr", r#""a + b""#),
        ("solution_sketch", r#""Combine ${a}$ and ${b}$.""#),
        ("hints", r#"["Which operation does the sign name?"]"#),
        (
            "samples",
            r#"[{"params": {"a": 1, "b": 1, "op": "+"}, "expected": "2"},
                {"params": {"a": 9, "b": 9, "op": "+"}, "expected": "18"}]"#,
        ),
    ]));
    assert_eq!(
        rejection.message,
        "no worked sample uses op=['-'] — every choice must appear in a sample, or the expression is unverified for it"
    );
    assert_eq!(rejection.code, "choice-coverage");
}
