//! The helpers of the `template_gate` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

pub use cadus_core::answer::canonical_form;
pub use cadus_core::curriculum::{AnswerKind, Exemplar};
pub use cadus_core::template::{
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
pub const BASE: [(&str, &str); 9] = [
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
pub fn body_with(overrides: &[(&str, &str)]) -> String {
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

pub fn doc_of(body: &str) -> TemplateDoc {
    from_body(body).unwrap_or_else(|err| panic!("the fixture body reads: {err}\n{body}"))
}

/// The exemplars of a knowledge point, given their answers.
pub fn exemplars(answers: &[&str]) -> Vec<Exemplar> {
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
pub fn reject(body: &str, kind: AnswerKind, exemplar_answers: &[&str]) -> Rejection {
    let pool = exemplars(exemplar_answers);
    let spec = GateSpec {
        answer_kind: kind,
        exemplars: &pool,
    };
    gate(&doc_of(body), &spec).expect_err("the gate refuses this document")
}

/// Verify a numeric body whose knowledge point answers 49 and 81.
pub fn reject_squares(body: &str) -> Rejection {
    reject(body, AnswerKind::Numeric, &["49", "81"])
}

/// Verify a body and expect it to pass.
pub fn accept(
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
// 6. The M4 review 1 repairs
// --------------------------------------------------------------------------
/// Bind a tuple of whole numbers, in the order the names sort.
pub fn bind(pairs: &[(&str, i64)]) -> Bindings {
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

// --------------------------------------------------------------------------
// 7. M4 review round 2
// --------------------------------------------------------------------------
/// The reviewer's adjacent-parameter document, with the answer it takes.
///
/// `${a}{b}$` writes the two numbers next to each other, so `a = 1, b = 12` and
/// `a = 11, b = 2` render ONE statement, `$112$`.
pub fn adjacent(verb: &str, noun: &str, expression: &str, samples: &str) -> String {
    body_with(&[
        (
            "statement",
            &format!(
                r#""A code is made by writing one number next to another: ${{a}}{{b}}$. {verb} the two numbers that were written. What is the {noun}?""#
            ),
        ),
        (
            "params",
            r#"{"a": {"kind": "int", "low": 1, "high": 12},
                "b": {"kind": "int", "low": 1, "high": 12}}"#,
        ),
        ("answer_expr", expression),
        ("solution_sketch", r#""Read the two numbers apart.""#),
        ("hints", r#"["Which two numbers were written down?"]"#),
        ("samples", samples),
    ])
}

/// M4 review 2, findings 2 and 5: the A1 flagship shape is approvable.
///
/// Three axes over 1..50 with `a*a + b*b = c*c` admit 40 tuples: the 20
/// unordered triples with every side at 50 or under — (3,4,5), (6,8,10),
/// (9,12,15), (12,16,20), (15,20,25), (18,24,30), (21,28,35), (24,32,40),
/// (27,36,45), (30,40,50), (5,12,13), (10,24,26), (15,36,39), (8,15,17),
/// (16,30,34), (7,24,25), (14,48,50), (20,21,29), (9,40,41), (12,35,37) —
/// each of them with the two legs in both orders.
///
/// 125,000 declared tuples put the document above the exhaustive limit, so the
/// deleted 4,096-draw estimator scaled 0 hits to a space of 0 and the gate
/// answered `only 0 distinct problem(s)`. The walk of 262,144 draws finds
/// [`PYTHAGOREAN_FOUND`] of the 40 tuples, and that count is what the gate
/// stores and what the floor reads.
pub const PYTHAGOREAN_FOUND: u64 = 35;

/// The count of tuples the constraints admit, worked by hand from the list above.
pub const PYTHAGOREAN_TUPLES: u64 = 40;

/// The `params` field of two int parameters, `a` and `b`, over the given ranges.
pub fn int_pair(a: (i64, i64), b: (i64, i64)) -> String {
    format!(
        r#"{{"a": {{"kind": "int", "low": {}, "high": {}}}, "b": {{"kind": "int", "low": {}, "high": {}}}}}"#,
        a.0, a.1, b.0, b.1
    )
}

/// A subtraction template `a - b` over two int ranges, with the given samples.
pub fn subtraction_body(a: (i64, i64), b: (i64, i64), samples: &str) -> String {
    body_with(&[
        ("statement", r#""Compute ${a} - {b}$.""#),
        ("params", &int_pair(a, b)),
        ("answer_expr", r#""a - b""#),
        ("solution_sketch", r#""Subtract ${b}$ from ${a}$.""#),
        ("hints", r#"["Which column do you subtract first?"]"#),
        ("samples", samples),
    ])
}

/// The three samples of the difference template under `a > b` over 1..6: the low end of `a`, the low end of `a - b`, and its high end.
pub const DIFFERENCE_SAMPLES: &str = r#"[{"params": {"a": 2, "b": 1}, "expected": "1"},
    {"params": {"a": 6, "b": 5}, "expected": "1"},
    {"params": {"a": 6, "b": 1}, "expected": "5"}]"#;

/// A difference template `a - b` with constraints, in the "take" wording.
///
/// An empty `constraints` drops the field.
pub fn difference_body(a: (i64, i64), b: (i64, i64), constraints: &str, samples: &str) -> String {
    body_with(&[
        ("statement", r#""Compute ${a} - {b}$.""#),
        ("params", &int_pair(a, b)),
        ("constraints", constraints),
        ("answer_expr", r#""a - b""#),
        ("solution_sketch", r#""Take ${b}$ from ${a}$.""#),
        ("hints", r#"["Which number is larger?"]"#),
        ("samples", samples),
    ])
}

/// An addition template `a + b` with constraints. An empty `constraints` drops the field.
pub fn addition_body(a: (i64, i64), b: (i64, i64), constraints: &str, samples: &str) -> String {
    body_with(&[
        ("statement", r#""Compute ${a} + {b}$.""#),
        ("params", &int_pair(a, b)),
        ("constraints", constraints),
        ("answer_expr", r#""a + b""#),
        ("solution_sketch", r#""Add ${b}$ to ${a}$.""#),
        ("hints", r#"["Which column do you add first?"]"#),
        ("samples", samples),
    ])
}

/// The halving template `a/2` over the base domain 1..12.
pub fn halving_body(samples: &str) -> String {
    body_with(&[
        ("statement", r#""Compute ${a} \\div 2$.""#),
        ("answer_expr", r#""a/2""#),
        ("solution_sketch", r#""Halve ${a}$.""#),
        ("hints", r#"["What is half of an odd number?"]"#),
        ("samples", samples),
    ])
}

/// The decimal product template `d*n`, with `d` over the given domain and `n` over 2..9.
pub fn decimal_product_body(d_domain: &str, samples: &str) -> String {
    let params = format!(r#"{{"d": {d_domain}, "n": {{"kind": "int", "low": 2, "high": 9}}}}"#);
    body_with(&[
        ("statement", r#""Compute ${d} \\times {n}$.""#),
        ("params", &params),
        ("answer_expr", r#""d*n""#),
        ("solution_sketch", r#""Multiply ${n}$ by ${d}$.""#),
        ("hints", r#"["What does the decimal point move?"]"#),
        ("samples", samples),
    ])
}

/// The base template over 1..5: five distinct problems, under the floor of 12.
pub fn five_problems_body() -> String {
    body_with(&[
        ("params", r#"{"a": {"kind": "int", "low": 1, "high": 5}}"#),
        (
            "samples",
            r#"[{"params": {"a": 1}, "expected": "1"},
                {"params": {"a": 5}, "expected": "25"}]"#,
        ),
    ])
}

/// Verify a numeric body whose knowledge point answers 25, and expect a pass.
pub fn accept_numeric(body: &str) -> cadus_core::template::Verified {
    accept(body, AnswerKind::Numeric, &["25"])
}

/// Verify a numeric body whose knowledge point answers 25, and expect a rejection.
pub fn reject_numeric(body: &str) -> Rejection {
    reject(body, AnswerKind::Numeric, &["25"])
}

/// The document of live rejection one: `a - b` over 59..70 and 63..63, which
/// answers -4 on the first tuple of the walk.
pub fn live_rejection_one_body() -> String {
    subtraction_body(
        (59, 70),
        (63, 63),
        r#"[{"params": {"a": 59, "b": 63}, "expected": "-4"},
            {"params": {"a": 70, "b": 63}, "expected": "7"}]"#,
    )
}

/// The decimal product template over the choice `0.2`, `0.5`, with its four samples.
pub fn decimal_choice_body() -> String {
    decimal_product_body(
        r#"{"kind": "choice", "values": ["0.2", "0.5"]}"#,
        r#"[{"params": {"d": "0.2", "n": 2}, "expected": "2/5"},
            {"params": {"d": "0.5", "n": 9}, "expected": "9/2"},
            {"params": {"d": "0.2", "n": 9}, "expected": "9/5"},
            {"params": {"d": "0.5", "n": 2}, "expected": "1"}]"#,
    )
}

/// Compile a one-parameter document and instantiate it at `a`.
pub fn squares_instance(doc: &TemplateDoc, a: i64) -> cadus_core::template::Instance {
    Compiled::new(doc)
        .expect("the template compiles")
        .instantiate(bind(&[("a", a)]))
        .expect("the tuple instantiates")
}
