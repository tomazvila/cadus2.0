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

use cadus_core::answer::canonical_form;
use cadus_core::curriculum::{AnswerKind, Exemplar};
use cadus_core::template::{
    Compiled, EXHAUSTIVE_SPACE_LIMIT, Envelope, FREE_SYMBOLS, GATE_SAMPLES, GATE_SEED, GateSpec,
    MAX_CHOICES, MAX_DOMAIN_SIZE, MAX_EXPONENT, MIN_SPACE_SIZE, NON_ANSWERS, RESERVED_NAMES,
    Rejection, SpaceSize, TEMPLATABLE_KINDS, TEMPLATE_VERSION, TemplateDoc, exemplar_envelope,
    from_body, gate, gate_body, rng_from_seed, with_space_size,
};

// --------------------------------------------------------------------------
// Fixtures
// --------------------------------------------------------------------------

/// The 1.0 perfect-squares template, in the 2.0 document shape.
///
/// 1.0 prints this document at `docs/reference/serving-1.0-spec.md` section 2.1:
/// `{"v": 2, "text": "Compute ${a}^{{2}}$.", "answer_expr": "a**2",
/// "params": {"a": {"kind": "int", "low": 1, "high": 12}}, "space_size": 12}`.
const BASE: [(&str, &str); 9] = [
    ("v", "1"),
    ("topic_id", r#""perfect-squares""#),
    ("answer_kind", r#""numeric""#),
    ("statement", r#""Compute ${a}^{{2}}$.""#),
    ("params", r#"{"a": {"kind": "int", "low": 1, "high": 12}}"#),
    ("answer_expr", r#""a**2""#),
    (
        "solution_sketch",
        r#""${a} \\times {a}$ gives the answer.""#,
    ),
    ("hints", r#"["What does squaring a number mean?"]"#),
    (
        "samples",
        r#"[{"params": {"a": 1}, "expected": "1"},
            {"params": {"a": 12}, "expected": "144"}]"#,
    ),
];

/// Write a body from the base document with the named fields replaced.
///
/// An override whose value is the empty string drops the field.
fn body_with(overrides: &[(&str, &str)]) -> String {
    let mut fields: Vec<(String, String)> = BASE
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    for (key, value) in overrides {
        match fields.iter_mut().find(|(name, _)| name == key) {
            Some(slot) => slot.1 = (*value).to_string(),
            None => fields.push(((*key).to_string(), (*value).to_string())),
        }
    }
    let written: Vec<String> = fields
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| format!("\"{key}\": {value}"))
        .collect();
    format!("{{{}}}", written.join(", "))
}

fn doc_of(body: &str) -> TemplateDoc {
    from_body(body).unwrap_or_else(|err| panic!("the fixture body reads: {err}\n{body}"))
}

/// The exemplars of a knowledge point, given their answers.
fn exemplars(answers: &[&str]) -> Vec<Exemplar> {
    answers
        .iter()
        .map(|answer| Exemplar {
            problem: "Compute $7^2$.".to_string(),
            answer: (*answer).to_string(),
            solution_sketch: None,
        })
        .collect()
}

/// Verify a body against a knowledge point, and expect a rejection.
fn reject(body: &str, kind: AnswerKind, exemplar_answers: &[&str]) -> Rejection {
    let pool = exemplars(exemplar_answers);
    let spec = GateSpec {
        answer_kind: kind,
        exemplars: &pool,
    };
    gate(&doc_of(body), &spec).expect_err("the gate refuses this document")
}

/// Verify a numeric body whose knowledge point answers 49 and 81.
fn reject_squares(body: &str) -> Rejection {
    reject(body, AnswerKind::Numeric, &["49", "81"])
}

/// Verify a body and expect it to pass.
fn accept(
    body: &str,
    kind: AnswerKind,
    exemplar_answers: &[&str],
) -> cadus_core::template::Verified {
    let pool = exemplars(exemplar_answers);
    let spec = GateSpec {
        answer_kind: kind,
        exemplars: &pool,
    };
    match gate(&doc_of(body), &spec) {
        Ok(verified) => verified,
        Err(rejection) => panic!("the gate refused an accepted fixture: {rejection}"),
    }
}

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
    let rejection = reject(
        &body_with(&[
            ("statement", r#""Compute ${a} - {b}$.""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 59, "high": 70},
                    "b": {"kind": "int", "low": 63, "high": 63}}"#,
            ),
            ("answer_expr", r#""a - b""#),
            ("solution_sketch", r#""Subtract ${b}$ from ${a}$.""#),
            ("hints", r#"["Which column do you subtract first?"]"#),
            (
                "samples",
                r#"[{"params": {"a": 59, "b": 63}, "expected": "-4"},
                    {"params": {"a": 70, "b": 63}, "expected": "7"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["25"],
    );
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
    let rejection = reject_squares(&body_with(&[
        ("params", r#"{"a": {"kind": "int", "low": 1, "high": 5}}"#),
        (
            "samples",
            r#"[{"params": {"a": 1}, "expected": "1"},
                {"params": {"a": 5}, "expected": "25"}]"#,
        ),
    ]));
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
    // `problem_templates.py:533-548` (`_ALLOWED_NAMES`, 45 names), plus the three
    // 2.0 spellings `e`, `min`, and `max`. Written out, in sorted order.
    assert_eq!(
        RESERVED_NAMES.to_vec(),
        vec![
            "Abs",
            "And",
            "E",
            "Eq",
            "False",
            "Float",
            "Ge",
            "Gt",
            "ITE",
            "Integer",
            "Le",
            "Lt",
            "Max",
            "Min",
            "Ne",
            "Not",
            "Or",
            "Piecewise",
            "Rational",
            "S",
            "True",
            "abs",
            "binomial",
            "cancel",
            "ceiling",
            "cos",
            "e",
            "exp",
            "expand",
            "factor",
            "factorial",
            "false",
            "floor",
            "gcd",
            "lcm",
            "ln",
            "log",
            "max",
            "min",
            "nsimplify",
            "pi",
            "sign",
            "simplify",
            "sin",
            "sqrt",
            "tan",
            "together",
            "true",
        ]
    );
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

#[test]
fn a_choice_domain_is_bounded_because_every_choice_must_be_sampled() {
    // `tests/test_problem_templates.py:540-556`.
    let values: Vec<String> = (0..=MAX_CHOICES)
        .map(|index| format!("\"{index}\""))
        .collect();
    let params = format!(
        r#"{{"a": {{"kind": "int", "low": 1, "high": 9}},
             "op": {{"kind": "choice", "values": [{}]}}}}"#,
        values.join(", ")
    );
    let rejection = reject_squares(&body_with(&[
        ("statement", r#""Compute ${a} + 1 {op}$.""#),
        ("params", params.as_str()),
        ("answer_expr", r#""a + 1""#),
        ("solution_sketch", r#""Add one to ${a}$.""#),
        ("hints", r#"["What is one more?"]"#),
        (
            "samples",
            r#"[{"params": {"a": 1, "op": "0"}, "expected": "2"}]"#,
        ),
    ]));
    assert_eq!(
        rejection.message,
        "a choice domain of 25 exceeds MAX_CHOICES (24); every choice must appear in a worked sample, so use an int domain or split the template"
    );
    assert_eq!(rejection.code, "choice-domain");
}

#[test]
fn a_backslash_in_a_choice_value_passes_the_gate() {
    // `tests/test_problem_templates.py:509-537`: `\times` as a choice value broke
    // the replacement side of the 1.0 regular expression, mid-serve, past the
    // error guard and into a 500. 2.0 renders through a scanner, so the class is
    // gone — and the specification (trap 6) asks for the pin anyway.
    let verified = accept(
        &body_with(&[
            ("statement", r#""Compute ${a} {sym} 2$.""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 1, "high": 9},
                    "sym": {"kind": "choice", "values": ["\\times", "\\cdot"]}}"#,
            ),
            ("answer_expr", r#""a * 2""#),
            ("solution_sketch", r#""Double ${a}$.""#),
            ("hints", r#"["What does doubling mean?"]"#),
            (
                "samples",
                r#"[{"params": {"a": 1, "sym": "\\times"}, "expected": "2"},
                    {"params": {"a": 9, "sym": "\\times"}, "expected": "18"},
                    {"params": {"a": 5, "sym": "\\cdot"}, "expected": "10"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["49", "81"],
    );
    assert_eq!(verified.space, SpaceSize::Exact(18));
    assert_eq!(verified.instances_checked, 18);
}

// --------------------------------------------------------------------------
// 4. The pinned constants and the sampled branch
// --------------------------------------------------------------------------

#[test]
fn the_pinned_constants_hold_their_values() {
    // `docs/reference/serving-1.0-spec.md` section 9. 1.0 draws
    // `GATE_SAMPLES = 200`; 2.0 raises the count to the exhaustive limit, so the
    // walked branch and the drawn branch read the same number of instances.
    assert_eq!(GATE_SAMPLES, 4_096);
    assert_eq!(EXHAUSTIVE_SPACE_LIMIT, 4_096);
    assert_eq!(MIN_SPACE_SIZE, 12);
    assert_eq!(MAX_CHOICES, 24);
    assert_eq!(MAX_DOMAIN_SIZE, 10_000);
    assert_eq!(MAX_EXPONENT, 1_000);
    assert_eq!(TEMPLATE_VERSION, 1);
    assert_eq!(GATE_SEED, 0);
}

#[test]
fn a_space_too_large_to_walk_takes_the_sampled_branch() {
    // `tests/test_problem_templates.py:1172-1220`: 200 x 200 = 40,000 instances,
    // well above the walkable limit, and `a - b` goes negative for a < b.
    let rejection = reject(
        &body_with(&[
            ("statement", r#""Compute ${a} - {b}$.""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 100, "high": 299},
                    "b": {"kind": "int", "low": 100, "high": 299}}"#,
            ),
            ("answer_expr", r#""a - b""#),
            ("solution_sketch", r#""Subtract ${b}$ from ${a}$.""#),
            ("hints", r#"["Which column do you subtract first?"]"#),
            (
                "samples",
                r#"[{"params": {"a": 299, "b": 100}, "expected": "199"},
                    {"params": {"a": 100, "b": 299}, "expected": "-199"},
                    {"params": {"a": 299, "b": 299}, "expected": "0"},
                    {"params": {"a": 100, "b": 100}, "expected": "0"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["25"],
    );
    assert_eq!(rejection.code, "envelope-sign");
    assert!(
        rejection.message.ends_with(
            ", but every authored answer for this knowledge point is non-negative — narrow the domains so no instance goes below zero"
        ),
        "{}",
        rejection.message
    );
}

#[test]
fn the_sampled_branch_reports_its_estimate_and_its_sample_count() {
    // The same 200 x 200 space with an expression that never leaves the
    // envelope. 40,000 declared tuples, no constraint, so every drawn tuple is a
    // hit and the estimate is the declared product.
    let verified = accept(
        &body_with(&[
            ("statement", r#""Compute ${a} + {b}$.""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 100, "high": 299},
                    "b": {"kind": "int", "low": 100, "high": 299}}"#,
            ),
            ("answer_expr", r#""a + b""#),
            ("solution_sketch", r#""Add ${b}$ to ${a}$.""#),
            ("hints", r#"["Which column do you add first?"]"#),
            (
                "samples",
                r#"[{"params": {"a": 100, "b": 100}, "expected": "200"},
                    {"params": {"a": 299, "b": 299}, "expected": "598"},
                    {"params": {"a": 100, "b": 299}, "expected": "399"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["25"],
    );
    assert_eq!(
        verified.space,
        SpaceSize::Estimated {
            estimate: 40_000,
            samples: 4_096,
            hits: 4_096,
        }
    );
    assert_eq!(verified.instances_checked, 4_096);
    assert!(!verified.exhaustive);
}

// --------------------------------------------------------------------------
// 5. The 2.0 additions
// --------------------------------------------------------------------------

#[test]
fn the_space_is_the_satisfying_count_and_the_ends_come_from_it() {
    // `a > b` over 1..6 twice admits 15 of the 36 tuples, by hand: 5+4+3+2+1.
    // The lowest `a` of a SATISFYING tuple is 2, not the declared 1 — spec trap
    // 11, which says to read the ends off the satisfying set.
    let satisfying = |samples: &str| -> String {
        body_with(&[
            ("statement", r#""Compute ${a} - {b}$.""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 1, "high": 6},
                    "b": {"kind": "int", "low": 1, "high": 6}}"#,
            ),
            (
                "constraints",
                r#"[{"op": "gt", "left": "a", "right": "b"}]"#,
            ),
            ("answer_expr", r#""a - b""#),
            ("solution_sketch", r#""Take ${b}$ from ${a}$.""#),
            ("hints", r#"["Which number is larger?"]"#),
            ("samples", samples),
        ])
    };
    let verified = accept(
        &satisfying(
            r#"[{"params": {"a": 2, "b": 1}, "expected": "1"},
                {"params": {"a": 6, "b": 5}, "expected": "1"},
                {"params": {"a": 6, "b": 1}, "expected": "5"}]"#,
        ),
        AnswerKind::Numeric,
        &["25"],
    );
    assert_eq!(verified.space, SpaceSize::Exact(15));
    assert_eq!(verified.instances_checked, 15);

    // Drop the sample at the satisfying low end of `a`, and the gate names 2.
    let rejection = reject(
        &satisfying(
            r#"[{"params": {"a": 3, "b": 1}, "expected": "2"},
                {"params": {"a": 6, "b": 5}, "expected": "1"},
                {"params": {"a": 6, "b": 1}, "expected": "5"}]"#,
        ),
        AnswerKind::Numeric,
        &["25"],
    );
    assert_eq!(
        rejection.message,
        "no worked sample uses the low end of a (2) — the edges are where an expression stops being right"
    );
}

#[test]
fn a_sample_outside_the_constraints_verifies_nothing() {
    let rejection = reject(
        &body_with(&[
            ("statement", r#""Compute ${a} - {b}$.""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 1, "high": 6},
                    "b": {"kind": "int", "low": 1, "high": 6}}"#,
            ),
            (
                "constraints",
                r#"[{"op": "gt", "left": "a", "right": "b"}]"#,
            ),
            ("answer_expr", r#""a - b""#),
            ("solution_sketch", r#""Take ${b}$ from ${a}$.""#),
            ("hints", r#"["Which number is larger?"]"#),
            (
                "samples",
                r#"[{"params": {"a": 2, "b": 5}, "expected": "-3"},
                    {"params": {"a": 6, "b": 5}, "expected": "1"},
                    {"params": {"a": 6, "b": 1}, "expected": "5"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["25"],
    );
    assert_eq!(
        rejection.message,
        "sample 0 binds {'a': 2, 'b': 5}, which the gt constraint refuses — a sample outside the constraints verifies nothing"
    );
    assert_eq!(rejection.code, "sample-constraint");
}

#[test]
fn a_constraint_set_with_no_satisfying_tuple_is_refused() {
    let rejection = reject(
        &body_with(&[
            ("statement", r#""Compute ${a} + {b}$.""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 1, "high": 12},
                    "b": {"kind": "int", "low": 20, "high": 31}}"#,
            ),
            (
                "constraints",
                r#"[{"op": "eq", "left": "a", "right": "b"}]"#,
            ),
            ("answer_expr", r#""a + b""#),
            ("solution_sketch", r#""Add ${b}$ to ${a}$.""#),
            ("hints", r#"["Which column do you add first?"]"#),
            (
                "samples",
                r#"[{"params": {"a": 1, "b": 20}, "expected": "21"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["25"],
    );
    assert_eq!(
        rejection.message,
        "the constraints refuse every tuple of the declared domains, so the template has no instance to serve"
    );
    assert_eq!(rejection.code, "no-satisfying-tuple");
}

#[test]
fn a_stated_space_size_that_is_not_the_count_is_refused() {
    // `space_size` is the gate's field, never the author's
    // (`problem_templates.py:309`), and the 2.0 digest covers it (spec trap 8).
    let rejection = reject_squares(&body_with(&[("space_size", "20")]));
    assert_eq!(
        rejection.message,
        "space_size states 20 and the gate counts 12 — the gate fills space_size, not the author"
    );
    assert_eq!(rejection.code, "space-size");
}

#[test]
fn a_constraint_over_a_fractional_parameter_is_refused() {
    // Spec section 2.3: reject a template whose constraints read a non-integer
    // parameter under `divides`, `coprime`, `carries`, `mod`, or `digit_sum`.
    let rejection = reject_squares(&body_with(&[
        ("statement", r#""Compute ${a} \\times {r}$.""#),
        (
            "params",
            r#"{"a": {"kind": "int", "low": 1, "high": 12},
                "r": {"kind": "rational", "num": {"low": 1, "high": 3},
                      "den": {"low": 2, "high": 4}}}"#,
        ),
        (
            "constraints",
            r#"[{"op": "divides", "left": "a", "right": "r"}]"#,
        ),
        ("answer_expr", r#""a""#),
        ("solution_sketch", r#""Multiply ${a}$ by ${r}$.""#),
        ("samples", r#"[]"#),
    ]));
    assert_eq!(
        rejection.message,
        "the divides constraint reads whole numbers, and parameter 'r' draws values that are not whole"
    );
    assert_eq!(rejection.code, "constraint-whole");
}

#[test]
fn a_constraint_over_an_undeclared_parameter_is_refused() {
    let rejection = reject_squares(&body_with(&[(
        "constraints",
        r#"[{"op": "gt", "left": "a", "right": "b"}]"#,
    )]));
    assert_eq!(
        rejection.message,
        "a constraint term names undeclared parameter 'b'"
    );
    assert_eq!(rejection.code, "constraint-parameter");
}

#[test]
fn a_template_needs_a_hint_ladder() {
    let rejection = reject_squares(&body_with(&[("hints", "[]")]));
    assert_eq!(
        rejection.message,
        "a template needs at least one hint rung, and a hint may never give the answer away"
    );
    assert_eq!(rejection.code, "hint-missing");
}

#[test]
fn a_hint_that_names_the_answer_is_refused() {
    // Hard Rule 3: a hint is Socratic and never the final step. `a` of 4 answers
    // 16, and the statement `Compute $4^{2}$.` does not carry 16, so the rung
    // hands it over.
    let rejection = reject_squares(&body_with(&[(
        "hints",
        r#"["Remember that four squared is 16."]"#,
    )]));
    assert_eq!(
        rejection.message,
        "hint 0 reads 'Remember that four squared is 16.' for {'a': 4}, which names the answer '16' — a hint is a question, never the final step (Hard Rule 3)"
    );
    assert_eq!(rejection.code, "hint-answer");
}

#[test]
fn a_hint_that_names_an_undeclared_parameter_is_refused() {
    let rejection = reject_squares(&body_with(&[("hints", r#"["Look at {b} first."]"#)]));
    assert_eq!(rejection.message, "hint 0 uses undeclared parameters ['b']");
    assert_eq!(rejection.code, "hint-placeholder");
}

#[test]
fn the_exemplar_envelope_reads_exact_canonical_forms() {
    // Spec trap 12: 1.0 reads every exemplar answer through `float()`, which is
    // a float in a correctness decision. 2.0 reads the exact rational.
    assert_eq!(
        exemplar_envelope(&exemplars(&["49", "81"])),
        Some(Envelope {
            non_negative: true,
            integral: true
        })
    );
    assert_eq!(
        exemplar_envelope(&exemplars(&["1/2", "3/4"])),
        Some(Envelope {
            non_negative: true,
            integral: false
        })
    );
    assert_eq!(
        exemplar_envelope(&exemplars(&["-3", "8"])),
        Some(Envelope {
            non_negative: false,
            integral: true
        })
    );
    // A surd is not a plain number, so there is no envelope to read.
    assert_eq!(exemplar_envelope(&exemplars(&["2*sqrt(2)"])), None);
    // Neither is a knowledge point with no exemplar.
    assert_eq!(exemplar_envelope(&[]), None);
}

#[test]
fn a_whole_number_envelope_refuses_a_fractional_instance() {
    let rejection = reject(
        &body_with(&[
            ("statement", r#""Compute ${a} \\div 2$.""#),
            ("answer_expr", r#""a/2""#),
            ("solution_sketch", r#""Halve ${a}$.""#),
            ("hints", r#"["What is half of an odd number?"]"#),
            (
                "samples",
                r#"[{"params": {"a": 1}, "expected": "1/2"},
                    {"params": {"a": 12}, "expected": "6"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["25"],
    );
    assert_eq!(
        rejection.message,
        "instance {'a': 1} answers '1/2', but every authored answer for this knowledge point is a whole number"
    );
    assert_eq!(rejection.code, "envelope-integral");

    // With a fractional exemplar there is no integrality rule, and the same
    // document passes.
    let verified = accept(
        &body_with(&[
            ("statement", r#""Compute ${a} \\div 2$.""#),
            ("answer_expr", r#""a/2""#),
            ("solution_sketch", r#""Halve ${a}$.""#),
            ("hints", r#"["What is half of an odd number?"]"#),
            (
                "samples",
                r#"[{"params": {"a": 1}, "expected": "1/2"},
                    {"params": {"a": 12}, "expected": "6"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["1/2"],
    );
    assert_eq!(verified.space, SpaceSize::Exact(12));
}

#[test]
fn a_distractor_that_is_the_right_answer_is_refused() {
    let rejection = reject_squares(&body_with(&[(
        "distractors",
        r#"[{"answer": "a*a", "error_tag": "doubled"}]"#,
    )]));
    assert_eq!(
        rejection.message,
        "distractor 0 answers '1' for {'a': 1}, which is the right answer — a distractor names a mistake"
    );
    assert_eq!(rejection.code, "distractor");
}

// --------------------------------------------------------------------------
// 6. The body read owns the shape rows the typed read enforces
// --------------------------------------------------------------------------

#[test]
fn the_body_read_reports_the_1_0_shape_rejections() {
    let pool = exemplars(&["49", "81"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
    };
    // The last row reads as a document — an empty list is a list — so the typed
    // read passes it to the gate and the gate refuses it. The message is the
    // same sentence either way, which is the point: one mistake, one wording.
    let cases: [(&str, &str, &str); 7] = [
        (
            r#"{"a": {"kind": "int", "low": 1.5, "high": 12}}"#,
            "an int domain needs integer 'low' and 'high'",
            "body",
        ),
        (
            r#"{"a": {"kind": "choice", "values": [true]}}"#,
            "choice values must be strings or integers",
            "body",
        ),
        (
            r#"{"a": 5}"#,
            "a parameter domain was not an object",
            "body",
        ),
        (
            r#"{"a": {"kind": "weird"}}"#,
            "unknown domain kind 'weird'",
            "body",
        ),
        (
            r#"{"a": {"low": 1, "high": 2}}"#,
            "unknown domain kind None",
            "body",
        ),
        (
            r#"{"a": {"kind": "int", "low": "1", "high": 12}}"#,
            "an int domain needs integer 'low' and 'high'",
            "body",
        ),
        (
            r#"{"a": {"kind": "choice", "values": []}}"#,
            "a choice domain needs a non-empty 'values' list",
            "choice-domain",
        ),
    ];
    for (params, expected, code) in cases {
        let body = body_with(&[("params", params)]);
        let rejection = gate_body(&body, &spec).expect_err("the body does not verify");
        assert_eq!(rejection.message, expected, "params {params}");
        assert_eq!(rejection.code, code, "params {params}");
    }

    for (field, value, expected) in [
        (
            "solution_sketch",
            "5",
            "solution_expr must be a string when present",
        ),
        ("samples", "[5]", "a sample was not an object"),
        (
            "samples",
            r#"[{"params": {"a": 1}}]"#,
            "a sample needs 'params' and a scalar 'expected'",
        ),
    ] {
        let body = body_with(&[(field, value)]);
        let rejection = gate_body(&body, &spec).expect_err("the body does not read");
        assert_eq!(rejection.message, expected, "{field} = {value}");
    }
}

#[test]
fn gate_body_reads_and_verifies_a_well_formed_body() {
    let pool = exemplars(&["49", "81"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
    };
    let (doc, verified) = gate_body(&body_with(&[]), &spec).expect("the body verifies");
    assert_eq!(doc.answer_expr, "a**2");
    assert_eq!(doc.topic_id, "perfect-squares");
    assert_eq!(verified.space, SpaceSize::Exact(12));
}

// --------------------------------------------------------------------------
// 7. Every rejection substring the 1.0 tests assert
// --------------------------------------------------------------------------

#[test]
fn every_rejection_substring_of_the_specification_appears() {
    // `docs/reference/serving-1.0-spec.md` section 9, the row
    // "Rejection substrings asserted". Each substring is a literal here, and the
    // message it must appear in is produced by a real gate run.
    let messages: Vec<String> = vec![
        reject_squares(&body_with(&[("answer_expr", r#""a*2""#)])).message,
        reject_squares(&body_with(&[("statement", r#""Compute ${b}^{{2}}$.""#)])).message,
        reject_squares(&body_with(&[("statement", r#""Compute ${a}^{2}$.""#)])).message,
        reject_squares(&body_with(&[("samples", "[]")])).message,
        reject_squares(&body_with(&[("params", "{}")])).message,
        reject_squares(&body_with(&[(
            "params",
            r#"{"a": {"kind": "int", "low": 1, "high": 1000000}}"#,
        )]))
        .message,
        reject_squares(&body_with(&[(
            "params",
            r#"{"a": {"kind": "int", "low": 9, "high": 2}}"#,
        )]))
        .message,
        reject_squares(&body_with(&[("answer_expr", r#""a + b""#)])).message,
        reject_squares(&body_with(&[(
            "samples",
            r#"[{"params": {"q": 7}, "expected": "49"}]"#,
        )]))
        .message,
        reject_squares(&body_with(&[("answer_expr", r#""a**99999""#)])).message,
        reject(
            &body_with(&[
                ("statement", r#""Compute ${a} - {b}$.""#),
                (
                    "params",
                    r#"{"a": {"kind": "int", "low": 59, "high": 70},
                        "b": {"kind": "int", "low": 63, "high": 63}}"#,
                ),
                ("answer_expr", r#""a - b""#),
                ("solution_sketch", r#""Subtract ${b}$ from ${a}$.""#),
                ("hints", r#"["Which column do you subtract first?"]"#),
                (
                    "samples",
                    r#"[{"params": {"a": 59, "b": 63}, "expected": "-4"},
                        {"params": {"a": 70, "b": 63}, "expected": "7"}]"#,
                ),
            ]),
            AnswerKind::Numeric,
            &["25"],
        )
        .message,
        reject(
            &body_with(&[
                ("statement", r#""Compute ${a} \\div 2$.""#),
                ("answer_expr", r#""a/2""#),
                ("solution_sketch", r#""Halve ${a}$.""#),
                ("hints", r#"["What is half of an odd number?"]"#),
                (
                    "samples",
                    r#"[{"params": {"a": 1}, "expected": "1/2"},
                        {"params": {"a": 12}, "expected": "6"}]"#,
                ),
            ]),
            AnswerKind::Numeric,
            &["25"],
        )
        .message,
        reject_squares(&body_with(&[
            ("params", r#"{"a": {"kind": "int", "low": 1, "high": 5}}"#),
            (
                "samples",
                r#"[{"params": {"a": 1}, "expected": "1"},
                    {"params": {"a": 5}, "expected": "25"}]"#,
            ),
        ]))
        .message,
    ];
    for wanted in [
        "does not compute the stated answer",
        "undeclared parameters",
        "unescaped brace",
        "worked samples",
        "at least one parameter",
        "MAX_DOMAIN_SIZE",
        "is empty",
        "unknown names",
        "template declares",
        "exceeds the evaluation bound",
        "non-negative",
        "whole number",
        "distinct problem",
    ] {
        assert!(
            messages.iter().any(|message| message.contains(wanted)),
            "no rejection carries {wanted:?}"
        );
    }
}
