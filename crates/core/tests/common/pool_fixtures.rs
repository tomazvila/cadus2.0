//! Builders the pool tests share: the perfect-squares template, its twelve
//! pinned digests, and the three-exemplar fallback fixture.

use cadus_core::curriculum::model::Exemplar;
use cadus_core::template::{Bindings, Compiled, Instance, Scalar, TemplateDoc, from_body};

/// The 1.0 perfect-squares template, in the 2.0 document shape.
///
/// The declared space is 12 tuples, which is the `MIN_SPACE_SIZE` floor of
/// specification section 9. Twelve is also the count 1.0 uses for its
/// blocked-set test (`tests/test_problem_templates.py:583-618`).
#[must_use]
pub fn perfect_squares_body() -> &'static str {
    r#"{
      "topic_id": "perfect-squares",
      "answer_kind": "numeric",
      "v": 1,
      "samples": [{"params": {"a": 1}, "expected": "1"},
                  {"params": {"a": 12}, "expected": "144"}],
      "hints": ["What does squaring a number mean?"],
      "solution_sketch": "${a} \\times {a}$ gives the answer.",
      "answer_expr": "a**2",
      "params": {"a": {"kind": "int", "low": 1, "high": 12}},
      "statement": "Compute ${a}^{{2}}$."
    }"#
}

/// Read one fixture body into a document.
#[must_use]
pub fn doc_from(body: &str) -> TemplateDoc {
    from_body(body).expect("the fixture body reads")
}

/// Bind one whole number to one parameter name.
#[must_use]
pub fn bind_int(name: &str, value: i64) -> Bindings {
    let mut bindings = Bindings::new();
    bindings.insert(name.to_string(), Scalar::Int(value).value());
    bindings
}

/// Bind two whole numbers to two parameter names.
#[must_use]
pub fn bind_two(first: &str, one: i64, second: &str, two: i64) -> Bindings {
    let mut bindings = bind_int(first, one);
    bindings.insert(second.to_string(), Scalar::Int(two).value());
    bindings
}

/// The twelve statements of the perfect-squares template and their digests.
///
/// Every digest is `sha1(utf8(statement))[:12]`, worked out from the statement
/// and written here as a literal (specification section 5.1). The seventh row is
/// the row 1.0 pins in `tests/test_problem_templates.py:594-608`.
pub const SQUARE_STATEMENTS: [(&str, &str); 12] = [
    ("Compute $1^{2}$.", "44b34b7dc138"),
    ("Compute $2^{2}$.", "c88b03aa364f"),
    ("Compute $3^{2}$.", "e61280d48db9"),
    ("Compute $4^{2}$.", "b70f053d75ea"),
    ("Compute $5^{2}$.", "999f3a215c9b"),
    ("Compute $6^{2}$.", "4479aead909f"),
    ("Compute $7^{2}$.", "e4047cd6798e"),
    ("Compute $8^{2}$.", "46f23bdadecb"),
    ("Compute $9^{2}$.", "a60352fc67e1"),
    ("Compute $10^{2}$.", "87eb77864313"),
    ("Compute $11^{2}$.", "88df4e1cc368"),
    ("Compute $12^{2}$.", "c3d8b10562c1"),
];

/// The three exemplars of the fallback fixture, in author order.
pub const EXEMPLARS: [(&str, &str, &str); 3] = [
    ("Compute $3 + 4$.", "7", "2af3b1f3dd58"),
    ("Compute $10 + 6$.", "16", "f8a6a976625f"),
    ("Compute $25 + 25$.", "50", "60ddf2e09d81"),
];

/// One authored exemplar with no solution sketch.
#[must_use]
pub fn exemplar(problem: &str, answer: &str) -> Exemplar {
    Exemplar {
        problem: problem.to_string(),
        answer: answer.to_string(),
        solution_sketch: None,
    }
}

/// The three exemplars of [`EXEMPLARS`], in author order.
#[must_use]
pub fn exemplar_fixture() -> Vec<Exemplar> {
    EXEMPLARS
        .iter()
        .map(|(problem, answer, _)| exemplar(problem, answer))
        .collect()
}

/// The twelve instances of the perfect-squares template, in `a` order.
///
/// The order is fixed, so a test that asserts "the LAST candidate" names a
/// literal statement and never re-derives one.
#[must_use]
pub fn squares_in_order(compiled: &Compiled<'_>) -> Vec<Instance> {
    (1..=12)
        .map(|a| {
            compiled
                .instantiate(bind_int("a", a))
                .expect("the perfect-squares template instantiates")
        })
        .collect()
}
