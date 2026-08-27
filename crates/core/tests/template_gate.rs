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
    Bindings, Compiled, EXHAUSTIVE_SPACE_LIMIT, Envelope, FREE_SYMBOLS, GATE_DRAW_BUDGET,
    GATE_SAMPLES, GATE_SEED, GateSpec, MAX_CHOICES, MAX_DOMAIN_SIZE, MAX_EXPONENT, MIN_SPACE_SIZE,
    NON_ANSWERS, RESERVED_NAMES, Rejection, SpaceSize, TEMPLATABLE_KINDS, TEMPLATE_VERSION,
    TemplateDoc, Value, check_instance, exemplar_envelope, from_body, gate, gate_body,
    rng_from_seed, with_space_size,
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

// --------------------------------------------------------------------------
// 6. The M4 review 1 repairs
// --------------------------------------------------------------------------

/// Bind a tuple of whole numbers, in the order the names sort.
fn bind(pairs: &[(&str, i64)]) -> Bindings {
    pairs
        .iter()
        .map(|(name, value)| {
            (
                (*name).to_string(),
                Value::Num(num_rational::BigRational::from(num_bigint::BigInt::from(
                    *value,
                ))),
            )
        })
        .collect()
}

/// M4 review 1, finding 4: the answer never reads a parameter the learner never sees.
///
/// The reviewer's document renders `Compute $3$ squared.` for twelve values of
/// `b` and answers 10, 11, ... 21 for them. The pool keys an instance by the
/// digest of the statement, so it keeps one of the twelve and serves that one
/// answer for every learner who reads the same problem. Nothing downstream sees
/// the defect, because the two halves of one instance agree.
#[test]
fn a_parameter_the_statement_never_shows_is_refused() {
    let rejection = reject_squares(&body_with(&[
        ("statement", r#""Compute ${a}$ squared.""#),
        (
            "params",
            r#"{"a": {"kind": "int", "low": 1, "high": 12},
                "b": {"kind": "int", "low": 1, "high": 12}}"#,
        ),
        ("answer_expr", r#""a**2 + b""#),
        ("solution_sketch", r#""Multiply ${a}$ by itself.""#),
        ("hints", r#"["What does squaring a number mean?"]"#),
        (
            "samples",
            r#"[{"params": {"a": 1, "b": 1}, "expected": "2"},
                {"params": {"a": 1, "b": 12}, "expected": "13"},
                {"params": {"a": 12, "b": 1}, "expected": "145"},
                {"params": {"a": 12, "b": 12}, "expected": "156"}]"#,
        ),
    ]));
    assert_eq!(
        rejection.message,
        "parameter 'b' changes the answer but never appears in the statement"
    );
    assert_eq!(rejection.code, "hidden-parameter");
}

/// A parameter the statement shows and the answer reads is not hidden.
#[test]
fn a_parameter_the_statement_shows_passes_the_hidden_rule() {
    let verified = accept(&body_with(&[]), AnswerKind::Numeric, &["49", "81"]);
    assert_eq!(verified.space, SpaceSize::Exact(12));
}

/// M4 review 1, findings 7 and 12: a spent draw never ends the sampled walk.
///
/// Both documents are the reviewer's. The first is the one whose constraints
/// hold for about one draw in 2,048; the second holds for about one in 450.
/// Before the repair the walk stopped at the first draw that spent its budget,
/// and the gate answered `[no-satisfying-tuple] the constraints refuse every
/// tuple`, which the same gate call contradicts: it counts 20 and 17 tuples.
///
/// The counts are the ones a run of the fixed seed produces, and the seed is a
/// constant, so the numbers hold on every machine.
#[test]
fn a_sparse_constraint_is_walked_past_the_draws_that_spend_their_budget() {
    let sparse = accept(
        &body_with(&[
            ("statement", r#""Compute ${a} + {b}$.""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 4096, "high": 8192},
                    "b": {"kind": "int", "low": 1, "high": 10}}"#,
            ),
            (
                "constraints",
                r#"[{"op": "eq", "left": {"mod": ["a", {"lit": 4096}]}, "right": {"lit": 0}}]"#,
            ),
            ("answer_expr", r#""a + b""#),
            ("solution_sketch", r#""Add ${b}$ to ${a}$.""#),
            ("hints", r#"["Which column do you add first?"]"#),
            (
                "samples",
                r#"[{"params": {"a": 4096, "b": 1}, "expected": "4097"},
                    {"params": {"a": 8192, "b": 10}, "expected": "8202"},
                    {"params": {"a": 4096, "b": 10}, "expected": "4106"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["25"],
    );
    assert_eq!(
        sparse.space,
        SpaceSize::Estimated {
            estimate: 20,
            samples: 4_096,
            hits: 2,
        }
    );
    assert_eq!(sparse.instances_checked, 128);
    assert_eq!(
        sparse.notes,
        vec![
            "the sampled walk found 128 satisfying tuple(s) in 262144 draw(s), and the instance check read those 128"
                .to_string()
        ]
    );

    // 1..90 twice with `a = b` and `5 divides a`: the 18 tuples a = b = 5, 10,
    // ... 90, worked by hand, of 8,100 declared tuples.
    let paired = accept(
        &body_with(&[
            ("statement", r#""Compute ${a} + {b}$.""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 1, "high": 90},
                    "b": {"kind": "int", "low": 1, "high": 90}}"#,
            ),
            (
                "constraints",
                r#"[{"op": "eq", "left": "a", "right": "b"},
                    {"op": "divides", "left": {"lit": 5}, "right": "a"}]"#,
            ),
            ("answer_expr", r#""a + b""#),
            ("solution_sketch", r#""Add ${b}$ to ${a}$.""#),
            ("hints", r#"["Which column do you add first?"]"#),
            (
                "samples",
                r#"[{"params": {"a": 5, "b": 5}, "expected": "10"},
                    {"params": {"a": 90, "b": 90}, "expected": "180"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["25"],
    );
    assert_eq!(
        paired.space,
        SpaceSize::Estimated {
            estimate: 17,
            samples: 4_096,
            hits: 9,
        }
    );
    assert_eq!(paired.instances_checked, 617);
}

/// A sampled walk that finds nothing says what it drew.
///
/// The exhaustive branch keeps the 1.0 sentence, because it read every tuple.
/// The sampled branch read a sample, and the message says so (M4 review 1,
/// finding 12).
#[test]
fn a_sampled_walk_that_finds_no_tuple_names_the_count_it_found() {
    let rejection = reject(
        &body_with(&[
            ("statement", r#""Compute ${a} + {b}$.""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 1, "high": 100},
                    "b": {"kind": "int", "low": 200, "high": 299}}"#,
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
                r#"[{"params": {"a": 1, "b": 200}, "expected": "201"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["25"],
    );
    assert_eq!(
        rejection.message,
        "the sampled walk drew 262144 tuple(s) of the declared domains and 0 satisfied the constraints, so the template has no instance to serve"
    );
    assert_eq!(rejection.code, "no-satisfying-tuple");
    assert_eq!(GATE_DRAW_BUDGET, 262_144);
}

/// M4 review 1, finding 16: above the limit the ends come from the satisfying sample.
///
/// The A1 flagship shape — `a > b` over 1..100 twice — was unapprovable: the
/// gate read the DECLARED low end of `a`, which is 1, and asked for a worked
/// sample there; every such sample breaks `a > b`, and the sample-constraint
/// rule then refused it. The satisfying sample of the fixed seed holds `a` from
/// 2 to 100 and `b` from 1 to 98, and two samples at those ends pass.
#[test]
fn above_the_limit_the_axis_ends_come_from_the_satisfying_sample() {
    let flagship = |samples: &str| -> String {
        body_with(&[
            ("statement", r#""Compute ${a} - {b}$.""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 1, "high": 100},
                    "b": {"kind": "int", "low": 1, "high": 100}}"#,
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
        &flagship(
            r#"[{"params": {"a": 2, "b": 1}, "expected": "1"},
                {"params": {"a": 100, "b": 98}, "expected": "2"}]"#,
        ),
        AnswerKind::Numeric,
        &["25"],
    );
    assert_eq!(
        verified.space,
        SpaceSize::Estimated {
            estimate: 4_938,
            samples: 4_096,
            hits: 2_023,
        }
    );
    assert_eq!(verified.instances_checked, 4_096);
    assert!(!verified.exhaustive);
    // One corner of the two is outside `a > b`, so the crossed-corner rule is
    // skipped and the reason is recorded (finding 22).
    assert_eq!(
        verified.notes,
        vec![
            "the crossed-corner rule is skipped for a and b: the constraints admit no tuple at a=2 with b=98"
                .to_string()
        ]
    );

    // The gate names the satisfying end, and never the declared end 1.
    let rejection = reject(
        &flagship(
            r#"[{"params": {"a": 3, "b": 1}, "expected": "2"},
                {"params": {"a": 100, "b": 98}, "expected": "2"}]"#,
        ),
        AnswerKind::Numeric,
        &["25"],
    );
    assert_eq!(
        rejection.message,
        "no worked sample uses the low end of a (2) — the edges are where an expression stops being right"
    );
    assert_eq!(rejection.code, "edge-coverage");
}

/// M4 review 1, finding 22: a band constraint is approvable with edge samples.
///
/// `b < a < b + 3` over 1..10 twice admits 17 tuples, worked by hand: 9 tuples
/// with a difference of 1 and 8 with a difference of 2. Neither crossed corner —
/// `a = 2` with `b = 9`, nor `a = 10` with `b = 1` — satisfies the band, so the
/// crossed-corner rule says nothing, and the gate records that instead of asking
/// for a sample its own sample-constraint rule refuses.
#[test]
fn a_band_constrained_template_is_approvable_and_the_skip_is_recorded() {
    let verified = accept(
        &body_with(&[
            ("statement", r#""Compute ${a} - {b}$.""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 1, "high": 10},
                    "b": {"kind": "int", "low": 1, "high": 10}}"#,
            ),
            (
                "constraints",
                r#"[{"op": "gt", "left": "a", "right": "b"},
                    {"op": "lt", "left": "a", "right": {"add": ["b", {"lit": 3}]}}]"#,
            ),
            ("answer_expr", r#""a - b""#),
            ("solution_sketch", r#""Take ${b}$ from ${a}$.""#),
            ("hints", r#"["Which number is larger?"]"#),
            (
                "samples",
                r#"[{"params": {"a": 2, "b": 1}, "expected": "1"},
                    {"params": {"a": 10, "b": 9}, "expected": "1"},
                    {"params": {"a": 3, "b": 1}, "expected": "2"},
                    {"params": {"a": 10, "b": 8}, "expected": "2"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["25"],
    );
    assert_eq!(verified.space, SpaceSize::Exact(17));
    assert_eq!(verified.instances_checked, 17);
    assert_eq!(
        verified.notes,
        vec![
            "the crossed-corner rule is skipped for a and b: the constraints admit no tuple at a=2 with b=9, nor at a=10 with b=1"
                .to_string()
        ]
    );
}

/// M4 review 1, finding 9: a decimal parameter reaches the learner as a decimal.
///
/// The choice values are decimals, and the statement shows them as the author
/// wrote them. The answer is exact, because the value behind the spelling is the
/// rational 1/5.
#[test]
fn a_decimal_choice_serves_the_spelling_its_author_wrote() {
    let verified = accept(
        &body_with(&[
            ("statement", r#""Compute ${d} \\times {n}$.""#),
            (
                "params",
                r#"{"d": {"kind": "choice", "values": ["0.2", "0.5"]},
                    "n": {"kind": "int", "low": 2, "high": 9}}"#,
            ),
            ("answer_expr", r#""d*n""#),
            ("solution_sketch", r#""Multiply ${n}$ by ${d}$.""#),
            ("hints", r#"["What does the decimal point move?"]"#),
            (
                "samples",
                r#"[{"params": {"d": "0.2", "n": 2}, "expected": "2/5"},
                    {"params": {"d": "0.5", "n": 9}, "expected": "9/2"},
                    {"params": {"d": "0.2", "n": 9}, "expected": "9/5"},
                    {"params": {"d": "0.5", "n": 2}, "expected": "1"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["2/5"],
    );
    assert_eq!(verified.space, SpaceSize::Exact(16));

    let doc = doc_of(&body_with(&[
        ("statement", r#""Compute ${d} \\times {n}$.""#),
        (
            "params",
            r#"{"d": {"kind": "choice", "values": ["0.2", "0.5"]},
                "n": {"kind": "int", "low": 2, "high": 9}}"#,
        ),
        ("answer_expr", r#""d*n""#),
        ("solution_sketch", r#""Multiply ${n}$ by ${d}$.""#),
        ("hints", r#"["What does the decimal point move?"]"#),
        (
            "samples",
            r#"[{"params": {"d": "0.2", "n": 2}, "expected": "2/5"},
                {"params": {"d": "0.5", "n": 9}, "expected": "9/2"},
                {"params": {"d": "0.2", "n": 9}, "expected": "9/5"},
                {"params": {"d": "0.5", "n": 2}, "expected": "1"}]"#,
        ),
    ]));
    let compiled = Compiled::new(&doc).expect("the template compiles");
    let mut bindings = bind(&[("n", 3)]);
    bindings.insert(
        "d".to_string(),
        cadus_core::template::Scalar::Text("0.2".to_string()).value(),
    );
    let instance = compiled
        .instantiate(bindings)
        .expect("the tuple instantiates");
    assert_eq!(instance.text, "Compute $0.2 \\times 3$.");
    assert_eq!(instance.answer, "3/5");
}

/// A decimal domain draws decimals, and the gate reads its ends.
#[test]
fn a_decimal_domain_serves_decimals() {
    let verified = accept(
        &body_with(&[
            ("statement", r#""Compute ${d} \\times {n}$.""#),
            (
                "params",
                r#"{"d": {"kind": "decimal", "low": 1, "high": 9, "scale": 1},
                    "n": {"kind": "int", "low": 2, "high": 9}}"#,
            ),
            ("answer_expr", r#""d*n""#),
            ("solution_sketch", r#""Multiply ${n}$ by ${d}$.""#),
            ("hints", r#"["What does the decimal point move?"]"#),
            (
                "samples",
                r#"[{"params": {"d": "0.1", "n": 2}, "expected": "1/5"},
                    {"params": {"d": "0.9", "n": 9}, "expected": "81/10"},
                    {"params": {"d": "0.1", "n": 9}, "expected": "9/10"},
                    {"params": {"d": "0.9", "n": 2}, "expected": "9/5"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["2/5"],
    );
    assert_eq!(verified.space, SpaceSize::Exact(72));
    assert_eq!(verified.instances_checked, 72);
}

/// M4 review 1, findings 13 and 17: an expression answer keeps its brackets.
///
/// No test instantiated an expression template before, so the two bracket rules
/// of the answer writer were unverified. `a*(x + 1)` needs the brackets around
/// the sum, and `a*(x**2)**3` needs them around the inner power: without them
/// the answer leaves the decidable grammar and every instance is refused.
#[test]
fn an_expression_answer_brackets_a_sum_and_a_nested_power() {
    let sum_body = body_with(&[
        ("topic_id", r#""distribute-over-a-sum""#),
        ("answer_kind", r#""expression""#),
        ("statement", r#""Expand ${a}(x + 1)$.""#),
        ("params", r#"{"a": {"kind": "int", "low": 2, "high": 13}}"#),
        ("answer_expr", r#""a*(x + 1)""#),
        ("solution_sketch", r#""Multiply each term by ${a}$.""#),
        ("hints", r#"["What does each term multiply?"]"#),
        (
            "samples",
            r#"[{"params": {"a": 2}, "expected": "2*x + 2"},
                {"params": {"a": 13}, "expected": "13*x + 13"}]"#,
        ),
    ]);
    let verified = accept(&sum_body, AnswerKind::Expression, &[]);
    assert_eq!(verified.space, SpaceSize::Exact(12));
    let sum_doc = doc_of(&sum_body);
    let compiled = Compiled::new(&sum_doc).expect("the template compiles");
    let instance = compiled
        .instantiate(bind(&[("a", 7)]))
        .expect("a = 7 instantiates");
    assert_eq!(instance.text, "Expand $7(x + 1)$.");
    assert_eq!(instance.answer, "7*(1 + x)");

    let power_body = body_with(&[
        ("topic_id", r#""powers-of-powers""#),
        ("answer_kind", r#""expression""#),
        (
            "statement",
            r#""Simplify ${a}\\left(x^{{2}}\\right)^{{3}}$.""#,
        ),
        ("params", r#"{"a": {"kind": "int", "low": 2, "high": 14}}"#),
        ("answer_expr", r#""a*(x**2)**3""#),
        ("solution_sketch", r#""Multiply the two exponents.""#),
        ("hints", r#"["What happens to the exponents?"]"#),
        (
            "samples",
            r#"[{"params": {"a": 2}, "expected": "2*x**6"},
                {"params": {"a": 14}, "expected": "14*x**6"}]"#,
        ),
    ]);
    let verified = accept(&power_body, AnswerKind::Expression, &[]);
    assert_eq!(verified.space, SpaceSize::Exact(13));
    let power_doc = doc_of(&power_body);
    let compiled = Compiled::new(&power_doc).expect("the template compiles");
    let instance = compiled
        .instantiate(bind(&[("a", 2)]))
        .expect("a = 2 instantiates");
    assert_eq!(instance.text, "Simplify $2\\left(x^{2}\\right)^{3}$.");
    assert_eq!(instance.answer, "2*(x**2)**3");
}

/// M4 review 1, finding 14: the exact root reads the denominator too.
///
/// `sqrt(4/3)` is not 2, and `sqrt(4/9)` is 2/3. Every earlier sqrt case bound a
/// whole number, so the denominator half of the perfect-square test never ran.
#[test]
fn the_exact_square_root_reads_a_radicand_that_is_not_whole() {
    let body = body_with(&[
        ("topic_id", r#""square-roots-of-fractions""#),
        ("statement", r#""Compute $\\sqrt{{4/{a}}}$.""#),
        ("params", r#"{"a": {"kind": "int", "low": 1, "high": 12}}"#),
        ("answer_expr", r#""sqrt(4/a)""#),
        (
            "solution_sketch",
            r#""Take the root of the top and the bottom.""#,
        ),
        ("hints", r#"["What is the square root of the top?"]"#),
        (
            "samples",
            r#"[{"params": {"a": 1}, "expected": "2"},
                {"params": {"a": 12}, "expected": "sqrt(1/3)"}]"#,
        ),
    ]);
    let verified = accept(&body, AnswerKind::Numeric, &[]);
    assert_eq!(verified.space, SpaceSize::Exact(12));
    let doc = doc_of(&body);
    let compiled = Compiled::new(&doc).expect("the template compiles");
    for (value, wanted) in [(1, "2"), (3, "sqrt(4/3)"), (9, "2/3"), (12, "sqrt(1/3)")] {
        let instance = compiled
            .instantiate(bind(&[("a", value)]))
            .expect("the tuple instantiates");
        assert_eq!(instance.answer, wanted, "sqrt(4/{value})");
    }
}

/// The per-instance check the refill runs is the per-instance check of the gate.
///
/// The refill of D-O4 draws tuples the gate never saw, so it runs this on every
/// instance before the instance enters the pool (M4 review 1, findings 1, 2, and
/// 15). The document is the reviewer's live rejection 1: `a - b` over 59..70 and
/// 63..63 answers -4 for the first tuple, and every authored answer of the
/// knowledge point is non-negative.
#[test]
fn the_per_instance_check_refuses_one_instance_the_gate_refuses() {
    let body = body_with(&[
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
    ]);
    let doc = doc_of(&body);
    let pool = exemplars(&["25"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
    };
    let compiled = Compiled::new(&doc).expect("the template compiles");

    let refused = compiled
        .instantiate(bind(&[("a", 59), ("b", 63)]))
        .expect("the tuple instantiates");
    assert_eq!(refused.answer, "-4");
    let rejection = check_instance(&doc, &spec, &refused).expect_err("the envelope refuses it");
    assert_eq!(
        rejection.message,
        "instance {'a': 59, 'b': 63} answers '-4', but every authored answer for this knowledge point is non-negative — narrow the domains so no instance goes below zero"
    );
    assert_eq!(rejection.code, "envelope-sign");

    let served = compiled
        .instantiate(bind(&[("a", 70), ("b", 63)]))
        .expect("the tuple instantiates");
    assert_eq!(served.answer, "7");
    assert!(check_instance(&doc, &spec, &served).is_ok());
}

/// The per-instance check reads the hint rule and the canonical round trip.
#[test]
fn the_per_instance_check_reads_the_hint_rule() {
    let body = body_with(&[(
        "hints",
        r#"["Remember that four squared is 16.", "What does squaring mean?"]"#,
    )]);
    let doc = doc_of(&body);
    let pool = exemplars(&["49", "81"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
    };
    let compiled = Compiled::new(&doc).expect("the template compiles");
    let instance = compiled
        .instantiate(bind(&[("a", 4)]))
        .expect("a = 4 instantiates");
    assert_eq!(instance.answer, "16");
    let rejection = check_instance(&doc, &spec, &instance).expect_err("the hint gives the answer");
    assert_eq!(rejection.code, "hint-answer");
    assert_eq!(
        rejection.message,
        "hint 0 reads 'Remember that four squared is 16.' for {'a': 4}, which names the answer '16' — a hint is a question, never the final step (Hard Rule 3)"
    );
}

/// The per-instance check reads the canonical round trip (V2).
///
/// The refill stores the answer string and the pool row carries it. A string the
/// M2 checker cannot decide grades nothing, and a string that decides to another
/// value grades every correct learner wrong, so the check reads the string back
/// and compares it with the canonical form the instance carries.
#[test]
fn the_per_instance_check_reads_the_answer_string_back() {
    let doc = doc_of(&body_with(&[]));
    let pool = exemplars(&["49", "81"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
    };
    let compiled = Compiled::new(&doc).expect("the template compiles");
    let instance = compiled
        .instantiate(bind(&[("a", 5)]))
        .expect("a = 5 instantiates");
    assert_eq!(instance.answer, "25");
    assert!(check_instance(&doc, &spec, &instance).is_ok());

    let mut wrong = instance.clone();
    wrong.answer = "26".to_string();
    let rejection = check_instance(&doc, &spec, &wrong).expect_err("26 is not the canonical form");
    assert_eq!(rejection.code, "canonical-mismatch");
    assert_eq!(
        rejection.message,
        "instance {'a': 5} answers '26', which does not read back as the canonical form the instance carries — the answer and its canonical form must agree (V2)"
    );

    let mut undecidable = instance;
    undecidable.answer = "1 +".to_string();
    let rejection =
        check_instance(&doc, &spec, &undecidable).expect_err("the checker cannot decide it");
    assert_eq!(rejection.code, "undecidable-answer");
    assert!(
        rejection.message.starts_with(
            "instance {'a': 5} answers '1 +', which the answer checker cannot decide: "
        ),
        "{}",
        rejection.message
    );
}
