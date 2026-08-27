//! M4 U1 acceptance: the template core (A1, D6, V2, T1).
//!
//! Every expected value in this file is a literal. The statements, the answers,
//! the digests, the counts, and the rejection reasons are written out, and none
//! of them is read back from the code under test. The literals come from three
//! places:
//!
//! - `docs/reference/serving-1.0-spec.md`, section 2 (the document), section 3
//!   (instantiation), section 8 (the traps), and section 9 (the pinned literals);
//! - `/home/deploy/dev/cadus/cadus_web/problem_templates.py`, the 1.0 module the
//!   specification surveys;
//! - plain arithmetic worked by hand, for the counts and the answers.
//!
//! No test in this file calls a model, opens a socket, or reads a clock (T1, R3).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeSet;

use cadus_core::learner::problem_text_hash;
use cadus_core::template::{Bindings, eval::EvalError, render::RenderError};
use cadus_core::template::{
    Cmp, Compiled, Constraint, Domain, DrawPlan, EXHAUSTIVE_SPACE_LIMIT, Instance, MAX_CHOICES,
    MAX_DOMAIN_SIZE, MIN_SPACE_SIZE, RESAMPLE_ATTEMPTS, Scalar, SpaceSize, TEMPLATE_VERSION,
    TemplateDoc, Term, Value, below, from_body, holds, render, rng_from_seed, space_size,
    stray_brace, to_body,
};

// --------------------------------------------------------------------------
// Fixtures
// --------------------------------------------------------------------------

/// The 1.0 perfect-squares template, in the 2.0 document shape.
///
/// 1.0 stores `{"v": 2, "text": "Compute ${a}^{{2}}$.", "answer_expr": "a**2",
/// "params": {"a": {"kind": "int", "low": 1, "high": 12}}, "space_size": 12}`
/// (`docs/reference/serving-1.0-spec.md`, section 2.1, the printed round trip).
fn perfect_squares_body() -> &'static str {
    r#"{
      "v": 1,
      "topic_id": "perfect-squares",
      "answer_kind": "numeric",
      "statement": "Compute ${a}^{{2}}$.",
      "params": {"a": {"kind": "int", "low": 1, "high": 12}},
      "answer_expr": "a**2",
      "solution_sketch": "${a} \\times {a}$ gives the answer.",
      "hints": ["What does squaring a number mean?"],
      "samples": [{"params": {"a": 1}, "expected": "1"},
                  {"params": {"a": 12}, "expected": "144"}]
    }"#
}

fn doc_from(body: &str) -> TemplateDoc {
    from_body(body).expect("the fixture body reads")
}

fn bind(pairs: &[(&str, i64)]) -> Bindings {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), Value::Num(num_rational_from(*value))))
        .collect()
}

fn num_rational_from(value: i64) -> num_rational::BigRational {
    num_rational::BigRational::from(num_bigint::BigInt::from(value))
}

fn text_binding(name: &str, text: &str) -> (String, Value) {
    (name.to_string(), Value::Text(text.to_string()))
}

/// One evaluation case: the source, the tuple it binds, and the answer.
type EvalCase = (&'static str, &'static [(&'static str, i64)], &'static str);

/// The whole numbers of a bound tuple, as `i64`, in name order.
fn whole_values(bindings: &Bindings) -> Vec<i64> {
    bindings
        .values()
        .map(|value| {
            value
                .as_integer()
                .and_then(|number| i64::try_from(number).ok())
                .expect("the tuple binds whole numbers")
        })
        .collect()
}

// --------------------------------------------------------------------------
// The four acceptance checks of U1
// --------------------------------------------------------------------------

/// U1 acceptance 1. `docs/plans/M4.md`: the 1.0 template `a**2` over 1..12
/// renders `Compute $7^{2}$.` for `a = 7` with answer `49`.
#[test]
fn perfect_squares_renders_the_1_0_statement_and_answer() {
    let doc = doc_from(perfect_squares_body());
    let compiled = Compiled::new(&doc).expect("the template compiles");
    let instance = compiled
        .instantiate(bind(&[("a", 7)]))
        .expect("a = 7 instantiates");

    assert_eq!(instance.text, "Compute $7^{2}$.");
    assert_eq!(instance.answer, "49");
    // The digest is `sha1(utf8(text))[:12]`, with no normalization
    // (`docs/reference/serving-1.0-spec.md` section 5.1). The value is written
    // out, not recomputed.
    assert_eq!(instance.instance_hash, "e4047cd6798e");
    assert_eq!(
        instance.instance_hash,
        problem_text_hash("Compute $7^{2}$.")
    );
}

/// U1 acceptance 2. A constrained pair `a > b` never draws `a <= b` over 10,000
/// draws. 1.0 could not express this constraint at all
/// (`problem_templates.py:59-66`), and the missing constraint is the defect A1
/// names.
#[test]
fn a_greater_than_b_never_draws_a_lower_or_equal_pair_over_10_000_draws() {
    let doc = doc_from(
        r#"{
          "v": 1,
          "topic_id": "two-digit-subtraction",
          "answer_kind": "numeric",
          "statement": "Compute ${a} - {b}$.",
          "params": {"a": {"kind": "int", "low": 1, "high": 12},
                     "b": {"kind": "int", "low": 1, "high": 12}},
          "constraints": [{"op": "gt", "left": "a", "right": "b"}],
          "answer_expr": "a - b",
          "hints": ["Which column do you subtract first?"],
          "samples": [{"params": {"a": 12, "b": 1}, "expected": "11"}]
        }"#,
    );
    let compiled = Compiled::new(&doc).expect("the template compiles");
    let mut rng = rng_from_seed(20_260_827);
    let mut lowest_difference = i64::MAX;

    for _ in 0..10_000 {
        let bindings = compiled
            .plan()
            .draw_satisfying(&doc.constraints, &mut rng)
            .expect("a satisfying tuple exists");
        let values = whole_values(&bindings);
        assert_eq!(values.len(), 2, "the tuple binds a and b");
        let (a, b) = (values[0], values[1]);
        assert!(a > b, "the draw returned a = {a} and b = {b}");
        lowest_difference = lowest_difference.min(a - b);
    }

    // The smallest difference a satisfying tuple allows is 1, so the draw
    // reached the tight edge of the constraint and did not merely stay far
    // inside it.
    assert_eq!(lowest_difference, 1);
    // 12 x 12 = 144 declared tuples, and 66 of them have a > b: 11 + 10 + ... + 1.
    assert_eq!(
        space_size(&doc.params, &doc.constraints).expect("the space counts"),
        SpaceSize::Exact(66)
    );
}

/// U1 acceptance 3. `\times` as a choice value renders.
///
/// 1.0 substitutes a bound value into the SOURCE TEXT, and a backslash is markup
/// on the replacement side of a regular expression: `\times` raised
/// `re.PatternError` until 1.0 wrapped the replacement in a callable
/// (`sympy_check.py:299-303`, pinned by `tests/test_problem_templates.py:513`).
/// 2.0 substitutes into a tree and renders with a scanner, so the class is gone.
/// The specification asks for the pin anyway (section 8, trap 6).
#[test]
fn a_backslash_choice_value_renders() {
    let doc = doc_from(
        r#"{
          "v": 1,
          "topic_id": "times-tables",
          "answer_kind": "numeric",
          "statement": "Compute ${a} {op} {b}$.",
          "params": {"a": {"kind": "int", "low": 2, "high": 12},
                     "b": {"kind": "int", "low": 2, "high": 12},
                     "op": {"kind": "choice", "values": ["\\times"]}},
          "answer_expr": "a*b",
          "hints": ["Read the sign first."],
          "samples": [{"params": {"a": 2, "b": 2, "op": "\\times"}, "expected": "4"}]
        }"#,
    );
    let compiled = Compiled::new(&doc).expect("the template compiles");
    let mut bindings = bind(&[("a", 7), ("b", 8)]);
    let (name, value) = text_binding("op", "\\times");
    bindings.insert(name, value);

    let instance = compiled.instantiate(bindings).expect("the tuple renders");
    assert_eq!(instance.text, "Compute $7 \\times 8$.");
    assert_eq!(instance.answer, "56");
}

/// U1 acceptance 4. A rational parameter draws reduced fractions.
///
/// The domain is new in 2.0: 1.0 has `IntDomain` and `ChoiceDomain` only
/// (`problem_templates.py:271-292`). A numerator in 1..9 over a denominator in
/// 2..12 makes 99 pairs and 66 DISTINCT reduced values, because `2/4` and `1/2`
/// are one value.
#[test]
fn a_rational_parameter_draws_reduced_fractions() {
    let doc = doc_from(
        r#"{
          "v": 1,
          "topic_id": "fraction-doubling",
          "answer_kind": "numeric",
          "statement": "Compute $2 \\times {r}$.",
          "params": {"r": {"kind": "rational",
                           "num": {"low": 1, "high": 9},
                           "den": {"low": 2, "high": 12}}},
          "answer_expr": "2*r",
          "hints": ["Double the numerator."],
          "samples": [{"params": {"r": "1/2"}, "expected": "1"}]
        }"#,
    );
    let plan = DrawPlan::new(&doc.params).expect("the domain materializes");
    assert_eq!(plan.declared_space(), 66);

    let mut rng = rng_from_seed(7);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for _ in 0..5_000 {
        let bindings = plan.draw(&mut rng);
        let value = bindings.get("r").expect("r is bound");
        let number = value.as_rational().expect("r is a number").clone();
        // A reduced fraction has a greatest common divisor of one and a positive
        // denominator. `BigRational` holds no other form once it is built here.
        let gcd = num_integer::Integer::gcd(number.numer(), number.denom());
        assert_eq!(
            gcd,
            num_bigint::BigInt::from(1),
            "value {number} is reduced"
        );
        assert!(*number.denom() > num_bigint::BigInt::from(0));
        seen.insert(value.canonical_string());
    }

    // All 66 distinct values come out, and the spellings are the reduced ones.
    assert_eq!(seen.len(), 66);
    assert!(seen.contains("1/2"));
    assert!(seen.contains("2/3"));
    assert!(seen.contains("9/2"));
    // A whole value writes its digits and never `4/1`.
    assert!(seen.contains("4"));
    assert!(!seen.contains("4/1"));
    // A reducible spelling never appears.
    for spelling in ["2/4", "3/6", "4/8", "6/9", "8/12"] {
        assert!(!seen.contains(spelling), "{spelling} is not reduced");
    }
}

// --------------------------------------------------------------------------
// The float trap, and the answer string
// --------------------------------------------------------------------------

/// `a*1.5` with `a = 2` answers `3`, and never `3.00000000000000`.
///
/// The 1.0 value is measured and quoted in the specification, section 3.4:
/// `a*1.5 {a:2} -> '3.00000000000000'`. It is SymPy's `Float` repr, and it is a
/// C4 hazard: the string reaches the learner as the expected answer. 2.0 reads
/// `1.5` as the exact rational `3/2` (D6), so the product is the integer 3.
#[test]
fn a_decimal_coefficient_answers_an_exact_integer_and_not_a_float_repr() {
    let doc = doc_from(
        r#"{
          "v": 1,
          "topic_id": "one-and-a-half-times",
          "answer_kind": "numeric",
          "statement": "Compute $1.5 \\times {a}$.",
          "params": {"a": {"kind": "int", "low": 2, "high": 40}},
          "answer_expr": "a*1.5",
          "hints": ["Half of the number, added to the number."],
          "samples": [{"params": {"a": 2}, "expected": "3"}]
        }"#,
    );
    let compiled = Compiled::new(&doc).expect("the template compiles");

    let instance = compiled
        .instantiate(bind(&[("a", 2)]))
        .expect("a = 2 works");
    assert_eq!(instance.answer, "3");
    assert_ne!(instance.answer, "3.00000000000000");

    // An odd multiplier lands on a half, and the half writes as an exact
    // fraction, never as `4.50000000000000`.
    let odd = compiled
        .instantiate(bind(&[("a", 3)]))
        .expect("a = 3 works");
    assert_eq!(odd.answer, "9/2");

    // No answer of the whole space carries a trailing zero run (spec trap 3).
    for value in 2..=40 {
        let each = compiled
            .instantiate(bind(&[("a", value)]))
            .expect("every value instantiates");
        assert!(
            !each.answer.contains(".0"),
            "answer {} carries a decimal point",
            each.answer
        );
    }
}

/// A negative bound value never re-associates, in the answer or in the statement.
///
/// 1.0 substitutes textually, so `a**2` with `a = -3` would read `-3**2` = -9
/// unless every value is wrapped in brackets; 1.0 wraps them for exactly this
/// reason (`sympy_check.py:297-305`). 2.0 substitutes a literal node into a tree,
/// and the writer brackets a negative literal under a power.
///
/// The statement side takes the same brackets, and the renderer writes them: the
/// author writes `${a}^{{2}}$` and the learner reads `$(-3)^{2}$`, which is the
/// question the answer 9 answers (M4 review 1, finding 18).
#[test]
fn a_negative_bound_value_squares_to_a_positive_answer() {
    let doc = doc_from(
        r#"{
          "v": 1,
          "topic_id": "squares-of-negatives",
          "answer_kind": "numeric",
          "statement": "Compute ${a}^{{2}}$.",
          "params": {"a": {"kind": "int", "low": -12, "high": -1}},
          "answer_expr": "a**2",
          "hints": ["What sign does a square carry?"],
          "samples": [{"params": {"a": -1}, "expected": "1"}]
        }"#,
    );
    let compiled = Compiled::new(&doc).expect("the template compiles");
    let instance = compiled
        .instantiate(bind(&[("a", -3)]))
        .expect("a = -3 instantiates");
    assert_eq!(instance.text, "Compute $(-3)^{2}$.");
    assert_eq!(instance.answer, "9");
    assert_ne!(instance.answer, "-9");
}

/// Every instance of every fixture template canonicalizes without `Undecidable`.
///
/// This is the property that makes the M5 grade path deterministic (V2, A3, L2).
/// `Compiled::instantiate` refuses an instance whose answer leaves the grammar,
/// so an `Ok` result IS the property.
#[test]
fn every_instance_of_every_fixture_canonicalizes() {
    let bodies = [
        perfect_squares_body(),
        r#"{"v": 1, "topic_id": "gcd", "answer_kind": "numeric",
            "statement": "What is the greatest common divisor of ${a}$ and ${b}$?",
            "params": {"a": {"kind": "int", "low": 2, "high": 30},
                       "b": {"kind": "int", "low": 2, "high": 30}},
            "answer_expr": "gcd(a, b)", "hints": ["List the factors of each."],
            "samples": [{"params": {"a": 2, "b": 2}, "expected": "2"}]}"#,
        r#"{"v": 1, "topic_id": "roots", "answer_kind": "numeric",
            "statement": "Compute $\\sqrt{{{a}}}$.",
            "params": {"a": {"kind": "int", "low": 1, "high": 50}},
            "answer_expr": "sqrt(a)", "hints": ["Look for a square factor."],
            "samples": [{"params": {"a": 1}, "expected": "1"}]}"#,
        r#"{"v": 1, "topic_id": "linear", "answer_kind": "expression",
            "statement": "Expand ${a}(x + {b})$.",
            "params": {"a": {"kind": "int", "low": 2, "high": 9},
                       "b": {"kind": "int", "low": 1, "high": 9}},
            "answer_expr": "a*x + a*b", "hints": ["Multiply both terms."],
            "samples": [{"params": {"a": 2, "b": 1}, "expected": "2*x + 2"}]}"#,
    ];

    let mut counted = 0_usize;
    for body in bodies {
        let doc = doc_from(body);
        let compiled = Compiled::new(&doc).expect("the template compiles");
        let mut rng = rng_from_seed(4);
        for bindings in compiled.candidates(&mut rng).expect("the stream builds") {
            let instance: Instance = compiled
                .instantiate(bindings.clone())
                .unwrap_or_else(|error| panic!("{bindings:?} did not instantiate: {error}"));
            assert!(!instance.answer.is_empty());
            assert_eq!(instance.instance_hash.chars().count(), 12);
            counted += 1;
        }
    }
    // 12 + 841 + 50 + 72 instances, all of them decided.
    assert_eq!(counted, 975);
}

// --------------------------------------------------------------------------
// The constraint language
// --------------------------------------------------------------------------

/// `carries` decides the column addition of two whole numbers.
///
/// Every expected value is worked by hand. `carries` is the one domain predicate
/// 1.0 names and could not express (`problem_templates.py:59-66`,
/// `docs/WEB_SERVICE.md:452`).
#[test]
fn the_carries_predicate_reads_the_decimal_columns() {
    let cases: [(i64, i64, bool); 10] = [
        // 9 + 3 = 12, so the ones column carries.
        (59, 63, true),
        // 2 + 3 = 5 and 1 + 1 = 2. No column reaches ten.
        (12, 13, false),
        // 5 + 5 = 10.
        (5, 5, true),
        (5, 4, false),
        // 9 + 1 = 10 in the ones column.
        (99, 1, true),
        // 0 + 0 = 0 and 1 + 1 = 2.
        (10, 10, false),
        // 1 + 9 = 10 in the ones column.
        (91, 19, true),
        // 0 + 0, then 5 + 5 = 10 in the tens column.
        (250, 250, true),
        // The digit run has no sign, so the magnitudes decide.
        (-59, 63, true),
        (0, 0, false),
    ];

    for (left, right, expected) in cases {
        let constraint = Constraint {
            op: Cmp::Carries,
            left: Term::Param("a".to_string()),
            right: Term::Param("b".to_string()),
        };
        let bindings = bind(&[("a", left), ("b", right)]);
        assert_eq!(
            holds(&constraint, &bindings).expect("the predicate decides"),
            expected,
            "carries({left}, {right})"
        );
    }
}

/// A `carries` constraint on `a + b` holds on 1,000 draws.
///
/// `a` and `b` each run over 1..9, so a carry means exactly `a + b >= 10`. There
/// are 45 such pairs: 1 for `a = 1`, 2 for `a = 2`, and so on to 9 for `a = 9`.
/// The three literals below — 45, 10, and 18 — are that hand count, the smallest
/// carrying sum, and the largest sum. A draw that ever returned a non-carrying
/// pair would push the smallest sum below 10.
#[test]
fn a_carries_constraint_holds_on_1_000_draws() {
    let doc = doc_from(
        r#"{
          "v": 1,
          "topic_id": "addition-with-carrying",
          "answer_kind": "numeric",
          "statement": "Compute ${a} + {b}$.",
          "params": {"a": {"kind": "int", "low": 1, "high": 9},
                     "b": {"kind": "int", "low": 1, "high": 9}},
          "constraints": [{"op": "carries", "left": "a", "right": "b"}],
          "answer_expr": "a + b",
          "hints": ["Add the ones column first."],
          "samples": [{"params": {"a": 9, "b": 9}, "expected": "18"}]
        }"#,
    );
    let compiled = Compiled::new(&doc).expect("the template compiles");
    let mut rng = rng_from_seed(11);
    let mut pairs: BTreeSet<(i64, i64)> = BTreeSet::new();
    let mut lowest_sum = i64::MAX;
    let mut highest_sum = i64::MIN;

    for _ in 0..1_000 {
        let bindings = compiled
            .plan()
            .draw_satisfying(&doc.constraints, &mut rng)
            .expect("a carrying pair exists");
        let values = whole_values(&bindings);
        assert_eq!(values.len(), 2);
        let (a, b) = (values[0], values[1]);
        lowest_sum = lowest_sum.min(a + b);
        highest_sum = highest_sum.max(a + b);
        pairs.insert((a, b));
    }

    assert_eq!(lowest_sum, 10);
    assert_eq!(highest_sum, 18);
    assert_eq!(pairs.len(), 45);
    assert_eq!(
        space_size(&doc.params, &doc.constraints).expect("the space counts"),
        SpaceSize::Exact(45)
    );
}

/// A borrow is a carry of the addition that undoes the subtraction.
///
/// The specification asks for both predicates (section 2.3). The operator list is
/// the nine names of `docs/plans/M4.md`, so the borrow is written with `carries`
/// over a `sub` term: `a - b` borrows exactly when `carries(a - b, b)` holds.
#[test]
fn a_borrow_is_written_as_a_carry_over_a_sub_term() {
    let constraint = Constraint {
        op: Cmp::Carries,
        left: Term::Sub(
            Box::new(Term::Param("a".to_string())),
            Box::new(Term::Param("b".to_string())),
        ),
        right: Term::Param("b".to_string()),
    };
    // 50 - 10 = 40, and 40 + 10 needs no carry: 50 - 10 needs no borrow.
    assert!(!holds(&constraint, &bind(&[("a", 50), ("b", 10)])).expect("it decides"));
    // 53 - 18 = 35, and 35 + 18 carries in the ones column: 53 - 18 borrows.
    assert!(holds(&constraint, &bind(&[("a", 53), ("b", 18)])).expect("it decides"));
    // 92 - 37 = 55, and 55 + 37 carries: 2 is below 7.
    assert!(holds(&constraint, &bind(&[("a", 92), ("b", 37)])).expect("it decides"));
    // 99 - 11 = 88, and 88 + 11 = 99 with no carry.
    assert!(!holds(&constraint, &bind(&[("a", 99), ("b", 11)])).expect("it decides"));
}

/// The comparison operators and the number-theory predicates decide exactly.
#[test]
fn the_constraint_operators_decide_on_exact_values() {
    let bindings = bind(&[("a", 12), ("b", 8)]);
    let cases: [(Cmp, bool); 9] = [
        (Cmp::Eq, false),
        (Cmp::Ne, true),
        (Cmp::Lt, false),
        (Cmp::Le, false),
        (Cmp::Gt, true),
        (Cmp::Ge, true),
        // 12 does not divide 8.
        (Cmp::Divides, false),
        // gcd(12, 8) = 4.
        (Cmp::Coprime, false),
        // 2 + 8 = 10 in the ones column.
        (Cmp::Carries, true),
    ];
    for (op, expected) in cases {
        let constraint = Constraint {
            op,
            left: Term::Param("a".to_string()),
            right: Term::Param("b".to_string()),
        };
        assert_eq!(
            holds(&constraint, &bindings).expect("it decides"),
            expected,
            "{} on 12 and 8",
            op.as_str()
        );
    }

    // `divides` reads left into right: 4 divides 12.
    let divides = Constraint {
        op: Cmp::Divides,
        left: Term::Param("b".to_string()),
        right: Term::Param("a".to_string()),
    };
    assert!(holds(&divides, &bind(&[("a", 12), ("b", 4)])).expect("it decides"));
    assert!(!holds(&divides, &bind(&[("a", 12), ("b", 5)])).expect("it decides"));
}

/// The term language computes on exact rationals and refuses a text binding.
#[test]
fn the_term_language_computes_exactly() {
    let body = r#"[
      {"op": "gt", "left": {"add": ["a", "b"]}, "right": {"lit": 100}},
      {"op": "eq", "left": {"digit_sum": "a"}, "right": {"lit": 9}},
      {"op": "eq", "left": {"mod": ["a", {"lit": 7}]}, "right": {"lit": 0}},
      {"op": "eq", "left": {"abs": {"sub": ["b", "a"]}}, "right": {"lit": 18}},
      {"op": "eq", "left": {"mul": ["a", {"lit": 2}]}, "right": {"lit": 126}}
    ]"#;
    let constraints: Vec<Constraint> = serde_json::from_str(body).expect("the terms read");
    assert_eq!(constraints.len(), 5);

    // a = 63: 63 + 45 = 108 > 100; 6 + 3 = 9; 63 mod 7 = 0; |45 - 63| = 18;
    // 63 * 2 = 126.
    let bindings = bind(&[("a", 63), ("b", 45)]);
    for constraint in &constraints {
        assert!(
            holds(constraint, &bindings).expect("it decides"),
            "{:?} holds on a = 63, b = 45",
            constraint.op
        );
    }

    // `mod` takes the sign of the divisor, so -7 mod 3 is 2 and not -1.
    let modulo: Constraint = serde_json::from_str(
        r#"{"op": "eq", "left": {"mod": ["a", {"lit": 3}]}, "right": {"lit": 2}}"#,
    )
    .expect("the term reads");
    assert!(holds(&modulo, &bind(&[("a", -7)])).expect("it decides"));

    // A text choice has no value, so a constraint over it is a document defect.
    let mut text = Bindings::new();
    let (name, value) = text_binding("a", "\\times");
    text.insert(name, value);
    let over_text: Constraint =
        serde_json::from_str(r#"{"op": "gt", "left": "a", "right": {"lit": 1}}"#)
            .expect("the term reads");
    let error = holds(&over_text, &text).expect_err("a text has no value");
    assert_eq!(
        error.to_string(),
        "a constraint term names \"a\", which is bound to the text \"\\\\times\" and not to a number"
    );

    // A decimal literal is written as a string, so no float ever enters (D6).
    let decimal: Constraint =
        serde_json::from_str(r#"{"op": "eq", "left": "a", "right": {"lit": "1.5"}}"#)
            .expect("the term reads");
    let mut half = Bindings::new();
    half.insert(
        "a".to_string(),
        Value::Num(num_rational::BigRational::new(
            num_bigint::BigInt::from(3),
            num_bigint::BigInt::from(2),
        )),
    );
    assert!(holds(&decimal, &half).expect("it decides"));
}

// --------------------------------------------------------------------------
// The renderer
// --------------------------------------------------------------------------

/// The scanner reads `{{`, `}}`, and `{name}`, and refuses every other brace.
///
/// The rules and the 12-character snippet come from 1.0 `_stray_brace`
/// (`problem_templates.py:231-259`) and the rejection message at `:521`.
#[test]
fn the_scanner_reads_the_placeholder_grammar() {
    let mut bindings = bind(&[("a", 7)]);
    let (name, value) = text_binding("op", "+");
    bindings.insert(name, value);

    // A doubled brace writes one brace, and a placeholder writes the value.
    assert_eq!(
        cadus_core::template::render("Compute ${a}^{{2}}$.", &bindings).expect("it renders"),
        "Compute $7^{2}$."
    );
    // A thousands mark in LaTeX is a doubled brace pair.
    assert_eq!(
        cadus_core::template::render("Is $7{{,}}329$ larger?", &bindings).expect("it renders"),
        "Is $7{,}329$ larger?"
    );
    // A closing doubled brace writes one brace.
    assert_eq!(
        cadus_core::template::render("{{{a}}}", &bindings).expect("it renders"),
        "{7}"
    );

    // A single literal brace is a rejection, and the report names the index and
    // the 12 characters that start there.
    let stray = stray_brace("Compute $7^{2}$ and ${a}$.").expect("the brace is stray");
    assert_eq!(stray.index, 11);
    assert_eq!(stray.snippet, "{2}$ and ${a");

    // The report of a short tail is the tail, and never a panic.
    let tail = stray_brace("value }").expect("the brace is stray");
    assert_eq!(tail.index, 6);
    assert_eq!(tail.snippet, "}");

    // A well-formed statement has no stray brace.
    assert_eq!(stray_brace("Compute ${a}^{{2}}$."), None);

    // A hole no tuple binds refuses the whole statement, so a hole never reaches
    // a learner. 1.0 buys this with `format_map`'s `KeyError` (`:328-336`).
    let missing =
        cadus_core::template::render("Compute ${z}$.", &bindings).expect_err("z is not bound");
    assert_eq!(
        missing,
        RenderError::Undeclared {
            name: "z".to_string()
        }
    );

    // The placeholder grammar is identifiers only: no format spec, no
    // conversion, no attribute access, no index (1.0 `_PLACEHOLDER_RE`).
    for statement in ["{a:>5}", "{a!r}", "{a.real}", "{a[0]}", "{0}", "{}"] {
        assert!(
            stray_brace(statement).is_some(),
            "{statement} must be a stray brace"
        );
    }

    // Every placeholder name of a statement comes out, in name order.
    let names =
        cadus_core::template::placeholders("${a} {op} {b} and {{a}}").expect("the statement scans");
    assert_eq!(
        names.into_iter().collect::<Vec<_>>(),
        vec!["a".to_string(), "b".to_string(), "op".to_string()]
    );
}

// --------------------------------------------------------------------------
// The exact evaluator
// --------------------------------------------------------------------------

/// The evaluation-only functions compute exactly and are erased.
///
/// The set is the one `docs/plans/M4.md` fixes. Every expected answer below is
/// worked by hand.
#[test]
fn the_evaluation_only_functions_compute_exactly_and_disappear() {
    let cases: [EvalCase; 14] = [
        ("gcd(a, b)", &[("a", 12), ("b", 18)], "6"),
        ("lcm(a, b)", &[("a", 4), ("b", 6)], "12"),
        ("floor(a/b)", &[("a", 7), ("b", 2)], "3"),
        ("ceiling(a/b)", &[("a", 7), ("b", 2)], "4"),
        ("min(a, b)", &[("a", 7), ("b", 2)], "2"),
        ("max(a, b)", &[("a", 7), ("b", 2)], "7"),
        ("factorial(a)", &[("a", 5)], "120"),
        ("binomial(a, b)", &[("a", 5), ("b", 2)], "10"),
        ("abs(a - b)", &[("a", 3), ("b", 10)], "7"),
        ("sqrt(a)", &[("a", 49)], "7"),
        ("sqrt(a)", &[("a", 8)], "sqrt(8)"),
        // The radicand of a root is not always whole: the denominator needs the
        // same perfect-square test as the numerator (M4 review 1, finding 14).
        ("sqrt(4/a)", &[("a", 3)], "sqrt(4/3)"),
        ("sqrt(4/a)", &[("a", 9)], "2/3"),
        ("a/b", &[("a", 10), ("b", 4)], "5/2"),
    ];

    for (source, values, expected) in cases {
        let ast = cadus_core::template::parse_answer_expr(source)
            .unwrap_or_else(|error| panic!("{source} parses: {error}"));
        let computed = cadus_core::template::answer(&ast, &bind(values))
            .unwrap_or_else(|error| panic!("{source} evaluates: {error}"));
        assert_eq!(computed.text, expected, "{source} over {values:?}");
        // No answer string carries a name from the evaluation-only set.
        for name in cadus_core::template::EXTRA_FUNCTIONS {
            assert!(
                !computed.text.contains(name),
                "{} still names {name}",
                computed.text
            );
        }
    }
}

/// `gcd` is one function name and never the product `g*c*d`.
///
/// The M2 parser splits a run of up to three plain letters into a product, so
/// `gcd` reads as `g*c*d` without the extra name set. The template parser passes
/// the set, so the run stays one call.
#[test]
fn an_extra_function_name_does_not_split_into_a_letter_run() {
    let with_set = cadus_core::template::parse_answer_expr("gcd(a, b)").expect("it parses");
    assert_eq!(
        with_set,
        cadus_core::answer::Ast::Func(
            "gcd".to_string(),
            vec![
                cadus_core::answer::Ast::Var("a".to_string()),
                cadus_core::answer::Ast::Var("b".to_string())
            ]
        )
    );
    // The M2 grammar alone splits the run into the product `g*c*d`, so the call
    // never reaches the checker as one function. That is why V2 needs the extra
    // set declared and bounded rather than guessed.
    assert!(
        !matches!(
            cadus_core::answer::parse("gcd(a, b)"),
            Ok(cadus_core::answer::Ast::Func(..))
        ),
        "the M2 grammar must not read `gcd` as a function"
    );
}

/// An `answer_expr` outside the grammar refuses the template (V2).
#[test]
fn an_answer_expression_outside_the_grammar_refuses_the_template() {
    // 1.0 admits `Piecewise`, `Eq`, `And`, `simplify`, `sign`, and more
    // (`_ALLOWED_NAMES`, `problem_templates.py:533-548`). None of them is in the
    // 2.0 grammar: `docs/plans/M4.md` picks option (a) of the specification.
    for source in [
        "Piecewise((a, a > b), (b, True))",
        "simplify(a + b)",
        "sign(a - b)",
        "Eq(a, b)",
        "a @ b",
    ] {
        assert!(
            cadus_core::template::parse_answer_expr(source).is_err(),
            "{source} must leave the grammar"
        );
    }

    let body = perfect_squares_body().replace("a**2", "Piecewise((a, a > 1), (1, True))");
    let doc = doc_from(&body);
    let error = Compiled::new(&doc).expect_err("the answer expression leaves the grammar");
    assert!(
        error
            .to_string()
            .starts_with("answer_expr is outside the decidable grammar"),
        "{error}"
    );
}

/// The evaluator refuses the values 1.0 answers with `zoo`, `I`, and `nan`.
///
/// 1.0 does not check finiteness: `1/0` returns `"zoo"` and `sqrt(-4)` returns
/// `"2*I"` (spec section 3.4, step 5). Neither is a number, and 1.0 catches them
/// one rung later with `_NON_ANSWERS` (`:203`). 2.0 refuses them at the source.
#[test]
fn the_evaluator_refuses_a_division_by_zero_and_a_negative_root() {
    let divide = cadus_core::template::parse_answer_expr("a/b").expect("it parses");
    let error = cadus_core::template::answer(&divide, &bind(&[("a", 1), ("b", 0)]))
        .expect_err("1/0 is not a number");
    assert_eq!(error, EvalError::DivideByZero);
    assert_eq!(error.to_string(), "answer_expr divides by zero");

    let root = cadus_core::template::parse_answer_expr("sqrt(a)").expect("it parses");
    let negative = cadus_core::template::answer(&root, &bind(&[("a", -4)]))
        .expect_err("sqrt(-4) is not a real number");
    assert_eq!(
        negative.to_string(),
        "answer_expr takes the square root of the negative number -4"
    );

    // A factorial outside the bound refuses, and never allocates the number.
    let factorial = cadus_core::template::parse_answer_expr("factorial(a)").expect("it parses");
    let too_large = cadus_core::template::answer(&factorial, &bind(&[("a", 201)]))
        .expect_err("201 is past the bound");
    assert_eq!(
        too_large.to_string(),
        "factorial takes a whole number from 0 to 200, and it got 201"
    );
    assert!(cadus_core::template::answer(&factorial, &bind(&[("a", 200)])).is_ok());
}

// --------------------------------------------------------------------------
// The domains, the space, and the draws
// --------------------------------------------------------------------------

/// The pinned constants of the specification, section 9.
///
/// Each one is written out here. A test that read the constant back would pass
/// on a mutated constant, which is exactly the failure `tests/test_problem_templates.py:1136-1140`
/// records: an evaluator changed `GATE_SAMPLES` to 1 and the 1.0 file stayed green.
#[test]
fn the_pinned_constants_hold_their_1_0_values() {
    assert_eq!(EXHAUSTIVE_SPACE_LIMIT, 4_096);
    assert_eq!(MIN_SPACE_SIZE, 12);
    assert_eq!(MAX_DOMAIN_SIZE, 10_000);
    assert_eq!(MAX_CHOICES, 24);
    assert_eq!(RESAMPLE_ATTEMPTS, 24);
    assert_eq!(TEMPLATE_VERSION, 1);
}

/// `space_size` of `a` in 1..12 is 12, with no constraint.
///
/// The literal is `tests/test_problem_templates.py:148` through the
/// specification, section 9.
#[test]
fn the_space_of_the_1_0_fixtures_is_twelve() {
    let doc = doc_from(perfect_squares_body());
    assert_eq!(
        space_size(&doc.params, &doc.constraints).expect("the space counts"),
        SpaceSize::Exact(12)
    );

    // `6*b` with `b` in 1..12 is 12 as well (`:1315`): the space counts tuples,
    // and never the values the answer takes.
    let six_b = doc_from(
        r#"{"v": 1, "topic_id": "six-times", "answer_kind": "numeric",
            "statement": "Compute $6 \\times {b}$.",
            "params": {"b": {"kind": "int", "low": 1, "high": 12}},
            "answer_expr": "6*b", "hints": ["Count in sixes."],
            "samples": [{"params": {"b": 1}, "expected": "6"}]}"#,
    );
    assert_eq!(
        space_size(&six_b.params, &six_b.constraints).expect("the space counts"),
        SpaceSize::Exact(12)
    );
}

/// Above the exhaustive limit the count is a labeled estimate, with its evidence.
///
/// Two domains of 200 values make 40,000 declared tuples, which is the sampled
/// branch of `tests/test_problem_templates.py:1180-1200`. The constraint `a > b`
/// holds on 19,900 of them, which is 49.75 percent, so a 4,096-sample estimate
/// lands near 19,900 and never on it.
#[test]
fn a_space_above_the_limit_is_an_estimate_that_names_its_sample_count() {
    let doc = doc_from(
        r#"{"v": 1, "topic_id": "big-space", "answer_kind": "numeric",
            "statement": "Compute ${a} - {b}$.",
            "params": {"a": {"kind": "int", "low": 1, "high": 200},
                       "b": {"kind": "int", "low": 1, "high": 200}},
            "constraints": [{"op": "gt", "left": "a", "right": "b"}],
            "answer_expr": "a - b", "hints": ["Subtract the ones column."],
            "samples": [{"params": {"a": 200, "b": 1}, "expected": "199"}]}"#,
    );
    let counted = space_size(&doc.params, &doc.constraints).expect("the space counts");
    let SpaceSize::Estimated {
        estimate,
        samples,
        hits,
    } = counted
    else {
        panic!("40,000 declared tuples are past the 4,096 limit: {counted:?}");
    };
    assert_eq!(samples, 4_096);
    assert!(hits > 0 && hits < 4_096, "hits = {hits}");
    // 19,900 of 40,000 tuples satisfy `a > b`. A 4,096-sample estimate stays
    // inside a tenth of that count.
    assert!(
        (17_910..=21_890).contains(&estimate),
        "estimate = {estimate}"
    );
    assert!(!counted.is_exact());
    // The estimate is a function of the document alone: the seed is a constant,
    // so a reviewer reproduces the stored number.
    assert_eq!(
        space_size(&doc.params, &doc.constraints).expect("the space counts"),
        counted
    );
}

/// The bounded draw never returns the bound, and it repeats from its seed.
///
/// The draw uses Lemire's multiply-and-shift, so no `%` runs on a drawn value
/// and the low end of a domain is not over-represented
/// (`docs/plans/M4.md`, the fixed RNG decision).
#[test]
fn the_bounded_draw_stays_below_the_bound_and_repeats_from_the_seed() {
    let mut rng = rng_from_seed(1);
    let first: Vec<u64> = (0..8).map(|_| below(&mut rng, 12)).collect();
    let mut again = rng_from_seed(1);
    let second: Vec<u64> = (0..8).map(|_| below(&mut again, 12)).collect();
    assert_eq!(first, second, "the seed alone decides the stream");

    let mut different = rng_from_seed(2);
    let other: Vec<u64> = (0..8).map(|_| below(&mut different, 12)).collect();
    assert_ne!(first, other, "a different seed gives a different stream");

    // A bound of one has one answer, and a bound of zero has the same answer.
    let mut edge = rng_from_seed(3);
    assert_eq!(below(&mut edge, 1), 0);
    assert_eq!(below(&mut edge, 0), 0);

    // Over 120,000 draws of a 12-value domain every value comes out, and none of
    // them comes out more than 12 percent of the time. A `% range` fold on a
    // range that does not divide 2^64 leans on the low values; 10,000 is the
    // even share and 12,000 is the bound this asserts.
    let mut counts = [0_usize; 12];
    let mut spread = rng_from_seed(9);
    for _ in 0..120_000 {
        let drawn = below(&mut spread, 12);
        assert!(drawn < 12, "the draw returned {drawn}");
        let index = usize::try_from(drawn).expect("the value fits");
        if let Some(slot) = counts.get_mut(index) {
            *slot += 1;
        }
    }
    for (value, count) in counts.iter().enumerate() {
        assert!(
            (8_000..=12_000).contains(count),
            "value {value} came out {count} times"
        );
    }
}

/// The candidate stream walks the whole space once at or under the limit.
///
/// 1.0 shuffles the full product and yields every tuple once
/// (`problem_templates.py:353-375`). Avoidance is then exact: with 11 of 12
/// instances blocked, the walk still reaches the free one.
#[test]
fn the_candidate_stream_walks_the_whole_small_space_once() {
    let doc = doc_from(perfect_squares_body());
    let compiled = Compiled::new(&doc).expect("the template compiles");
    let mut rng = rng_from_seed(5);
    let stream = compiled.candidates(&mut rng).expect("the stream builds");

    assert_eq!(stream.len(), 12);
    let mut rendered: BTreeSet<String> = BTreeSet::new();
    for bindings in &stream {
        rendered.insert(
            compiled
                .instantiate(bindings.clone())
                .expect("it instantiates")
                .text,
        );
    }
    assert_eq!(rendered.len(), 12);
    assert!(rendered.contains("Compute $1^{2}$."));
    assert!(rendered.contains("Compute $12^{2}$."));

    // The walk is shuffled, so two seeds give two orders of the same 12 tuples.
    let mut other = rng_from_seed(6);
    let second = compiled.candidates(&mut other).expect("the stream builds");
    assert_eq!(second.len(), 12);
    assert_ne!(stream, second, "the walk is shuffled");
}

/// A constraint set no tuple satisfies is an error, and never a violating tuple.
#[test]
fn an_unsatisfiable_constraint_set_reports_that_it_found_no_tuple() {
    let doc = doc_from(
        r#"{"v": 1, "topic_id": "impossible", "answer_kind": "numeric",
            "statement": "Compute ${a} - {b}$.",
            "params": {"a": {"kind": "int", "low": 1, "high": 5},
                       "b": {"kind": "int", "low": 8, "high": 12}},
            "constraints": [{"op": "gt", "left": "a", "right": "b"}],
            "answer_expr": "a - b", "hints": ["Compare the two numbers."],
            "samples": [{"params": {"a": 1, "b": 8}, "expected": "-7"}]}"#,
    );
    let compiled = Compiled::new(&doc).expect("the template compiles");
    let mut rng = rng_from_seed(13);

    let error = compiled
        .plan()
        .draw_satisfying(&doc.constraints, &mut rng)
        .expect_err("no tuple has a > b");
    assert_eq!(
        error.to_string(),
        "no tuple of the declared domains satisfies the constraints after 1000 draw(s)"
    );

    // The exhaustive stream is empty rather than wrong.
    assert!(compiled.candidates(&mut rng).expect("it builds").is_empty());
    assert_eq!(
        space_size(&doc.params, &doc.constraints).expect("the space counts"),
        SpaceSize::Exact(0)
    );
}

// --------------------------------------------------------------------------
// The document
// --------------------------------------------------------------------------

/// The body round-trips byte for byte, and an unknown key refuses it.
///
/// `content_store.digest` covers the whole body (spec section 8, trap 8), so a
/// re-serialized document that changed one byte would ask for a new human
/// approval it does not need.
#[test]
fn the_document_round_trips_and_refuses_an_unknown_key() {
    let compact = r#"{"v":1,"topic_id":"perfect-squares","answer_kind":"numeric","statement":"Compute ${a}^{{2}}$.","params":{"a":{"kind":"int","low":1,"high":12}},"constraints":[{"op":"gt","left":{"add":["a",{"lit":1}]},"right":{"lit":100}}],"answer_expr":"a**2","solution_sketch":"Square it.","hints":["What does squaring mean?"],"samples":[{"params":{"a":1},"expected":"1"}],"space_size":12}"#;
    let doc = doc_from(compact);
    assert_eq!(to_body(&doc).expect("it writes"), compact);
    assert_eq!(doc.v, 1);
    assert_eq!(doc.space_size, Some(SpaceSize::Exact(12)));
    assert_eq!(doc.topic_id, "perfect-squares");

    // An unknown key is a rejection, and never a silently dropped instruction.
    let extra = compact.replace(r#""v":1"#, r#""v":1,"difficulty":3"#);
    let error = from_body(&extra).expect_err("`difficulty` is not a field");
    assert!(error.to_string().contains("unknown field"), "{error}");

    // An unknown domain kind is a rejection too.
    let kind = compact.replace(r#""kind":"int""#, r#""kind":"gaussian""#);
    assert!(from_body(&kind).is_err());

    // An unknown constraint operator is a rejection.
    let op = compact.replace(r#""op":"gt""#, r#""op":"approximately""#);
    assert!(from_body(&op).is_err());

    // A malformed term is a rejection: `sub` takes exactly two operands.
    let sub = compact.replace(r#"{"add":["a",{"lit":1}]}"#, r#"{"sub":["a"]}"#);
    let sub_error = from_body(&sub).expect_err("`sub` takes two operands");
    assert!(
        sub_error
            .to_string()
            .contains("a sub term needs exactly 2 operands, and it has 1"),
        "{sub_error}"
    );

    // A JSON float is a rejection: a decimal literal is written as a string (D6).
    let float = compact.replace(r#"{"lit":100}"#, r#"{"lit":1.5}"#);
    assert!(from_body(&float).is_err());

    // The name sets the document reports are the sets a gate reads.
    assert_eq!(
        doc.param_names().into_iter().collect::<Vec<_>>(),
        vec!["a".to_string()]
    );
    assert_eq!(
        doc.constraint_names().into_iter().collect::<Vec<_>>(),
        vec!["a".to_string()]
    );
    assert_eq!(
        doc.statement_names()
            .expect("the statement scans")
            .into_iter()
            .collect::<Vec<_>>(),
        vec!["a".to_string()]
    );
}

/// A sample carries the tuple it was worked on, and the answer a human wrote.
#[test]
fn a_sample_binds_a_tuple_and_an_answer() {
    let doc = doc_from(perfect_squares_body());
    assert_eq!(doc.samples.len(), 2);
    let first = doc.samples.first().expect("the first sample");
    assert_eq!(first.expected.text(), "1");
    let bindings = first.bindings();
    assert_eq!(whole_values(&bindings), vec![1]);

    let compiled = Compiled::new(&doc).expect("the template compiles");
    for sample in &doc.samples {
        let instance = compiled
            .instantiate(sample.bindings())
            .expect("the sample instantiates");
        assert_eq!(
            instance.answer,
            sample.expected.text(),
            "the sample and the expression agree"
        );
    }
}

/// A domain past `MAX_DOMAIN_SIZE` is refused, and never materialized.
///
/// The payload is 1.0's, `low 1, high 10**6`
/// (`tests/test_problem_templates.py:169` through the specification, section 9).
#[test]
fn a_domain_past_the_bound_is_refused() {
    let doc = doc_from(
        r#"{"v": 1, "topic_id": "too-wide", "answer_kind": "numeric",
            "statement": "Compute ${a}$.",
            "params": {"a": {"kind": "int", "low": 1, "high": 1000000}},
            "answer_expr": "a", "hints": ["Read the number."],
            "samples": [{"params": {"a": 1}, "expected": "1"}]}"#,
    );
    let error = DrawPlan::new(&doc.params).expect_err("a million values is past the bound");
    assert_eq!(
        error.to_string(),
        "domain \"a\" holds 1000000 values, which exceeds MAX_DOMAIN_SIZE (10000)"
    );

    // An empty range is refused too.
    let empty = doc_from(
        r#"{"v": 1, "topic_id": "empty", "answer_kind": "numeric",
            "statement": "Compute ${a}$.",
            "params": {"a": {"kind": "int", "low": 5, "high": 1}},
            "answer_expr": "a", "hints": ["Read the number."],
            "samples": [{"params": {"a": 5}, "expected": "5"}]}"#,
    );
    assert_eq!(
        DrawPlan::new(&empty.params)
            .expect_err("the range runs backwards")
            .to_string(),
        "int domain 5..1 is empty"
    );

    // A rational domain whose denominator range holds zero is refused.
    let zero = doc_from(
        r#"{"v": 1, "topic_id": "zero-denominator", "answer_kind": "numeric",
            "statement": "Compute ${r}$.",
            "params": {"r": {"kind": "rational", "num": {"low": 1, "high": 9},
                             "den": {"low": -3, "high": 3}}},
            "answer_expr": "r", "hints": ["Read the fraction."],
            "samples": [{"params": {"r": "1/2"}, "expected": "1/2"}]}"#,
    );
    assert_eq!(
        DrawPlan::new(&zero.params)
            .expect_err("the denominator range holds zero")
            .to_string(),
        "the denominator range -3..3 of a rational domain holds zero"
    );
}

// --------------------------------------------------------------------------
// The M4 review 1 repairs
// --------------------------------------------------------------------------

/// M4 review 1, finding 8: a decimal constraint literal survives the body.
///
/// `write_rational` writes `1/2` for the literal `0.5`, because 2 is not a power
/// of ten. The reader took decimals only, so a gate-accepted document wrote a
/// body it did not read again, and the refill refused the digest a reviewer
/// had approved (C6).
#[test]
fn a_constraint_literal_round_trips_through_every_form_it_writes() {
    for (written, again) in [
        (r#"{"lit": "0.5"}"#, "1/2"),
        (r#"{"lit": "3/2"}"#, "3/2"),
        (r#"{"lit": "-5/2"}"#, "-5/2"),
        (r#"{"lit": "0.1"}"#, "0.1"),
        (r#"{"lit": 100}"#, "100"),
    ] {
        let source = format!(r#"{{"op": "ge", "left": "p", "right": {written}}}"#);
        let constraint: Constraint = serde_json::from_str(&source).expect("the term reads");
        let body = serde_json::to_string(&constraint).expect("the term writes");
        let reread: Constraint = serde_json::from_str(&body).expect("the written term reads again");
        assert_eq!(constraint, reread, "{written} does not round-trip");
        assert!(
            body.contains(again),
            "{written} writes {body}, which does not carry {again}"
        );
    }

    // The whole document round-trips, which is the property the digest rests on.
    let doc = doc_from(
        r#"{"v": 1, "topic_id": "fraction-of-a-number", "answer_kind": "numeric",
            "statement": "Compute ${p} \\times {a}$.",
            "params": {"p": {"kind": "rational", "num": {"low": 1, "high": 4},
                             "den": {"low": 2, "high": 5}},
                       "a": {"kind": "int", "low": 4, "high": 12}},
            "constraints": [{"op": "ge", "left": "p", "right": {"lit": "0.5"}}],
            "answer_expr": "p*a", "hints": ["What does the denominator ask for?"],
            "samples": [{"params": {"p": "0.5", "a": 4}, "expected": "2"}]}"#,
    );
    let body = to_body(&doc).expect("the document writes");
    assert!(
        body.contains(r#"{"lit":"1/2"}"#),
        "the body writes the literal as a fraction: {body}"
    );
    let again = from_body(&body).expect("the written body reads again");
    assert_eq!(doc, again);
}

/// A literal outside the three forms names the three forms the reader takes.
#[test]
fn a_literal_that_is_not_a_number_names_the_forms_the_reader_takes() {
    let error = serde_json::from_str::<Constraint>(
        r#"{"op": "eq", "left": "a", "right": {"lit": "one half"}}"#,
    )
    .expect_err("a word is not a literal");
    assert!(
        error.to_string().starts_with(
            "a lit term needs a whole number, a decimal string, or 'n/d', not \"one half\""
        ),
        "{error}"
    );
}

/// M4 review 1, finding 9: a value keeps the spelling its author wrote.
///
/// A choice value of `0.2` is the rational 1/5 for the evaluator and the text
/// `0.2` for the renderer. Before the repair the renderer wrote `1/5`, so a
/// decimals knowledge point served fraction problems.
#[test]
fn a_decimal_value_renders_as_the_decimal_its_author_wrote() {
    let value = Scalar::Text("0.2".to_string()).value();
    assert_eq!(value.canonical_string(), "0.2");
    assert_eq!(
        value.as_rational().map(std::string::ToString::to_string),
        Some("1/5".to_string())
    );
    // The number decides equality and order, and never the spelling.
    assert_eq!(
        value,
        Value::Num(num_rational::BigRational::new(
            num_bigint::BigInt::from(1),
            num_bigint::BigInt::from(5),
        ))
    );

    // A decimal domain writes the same spelling, at the scale it declares.
    let domain: Domain =
        serde_json::from_str(r#"{"kind": "decimal", "low": -2, "high": 21, "scale": 1}"#)
            .expect("the domain reads");
    let values = domain.values("d").expect("the domain walks");
    assert_eq!(values.len(), 24);
    let written: Vec<String> = values.iter().map(Value::canonical_string).collect();
    assert_eq!(written.first().map(String::as_str), Some("-0.2"));
    assert_eq!(written.get(2).map(String::as_str), Some("0.0"));
    assert_eq!(written.get(4).map(String::as_str), Some("0.2"));
    assert_eq!(written.last().map(String::as_str), Some("2.1"));
    assert_eq!(domain.size("d").expect("the domain counts"), 24);

    // A scale past the bound is a domain error, and never a wide number.
    let wide: Domain =
        serde_json::from_str(r#"{"kind": "decimal", "low": 1, "high": 9, "scale": 12}"#)
            .expect("the domain reads");
    assert_eq!(
        wide.values("d")
            .expect_err("the scale is too large")
            .to_string(),
        "decimal domain scale 12 exceeds MAX_DECIMAL_SCALE (9)"
    );
}

/// M4 review 1, finding 18: a value that is not atomic takes brackets.
///
/// The evaluator brackets a negative literal and a fraction under a power. The
/// renderer now writes the same brackets, so the printed problem asks the
/// question the stored answer answers. A decimal and a text choice are atomic
/// and take none.
#[test]
fn the_renderer_brackets_a_negative_value_and_a_fraction() {
    let statement = "Compute ${a}^{{2}}$.";
    assert_eq!(
        render(statement, &bind(&[("a", -3)])).expect("it renders"),
        "Compute $(-3)^{2}$."
    );
    assert_eq!(
        render(statement, &bind(&[("a", 3)])).expect("it renders"),
        "Compute $3^{2}$."
    );

    let mut fraction = Bindings::new();
    fraction.insert(
        "a".to_string(),
        Value::Num(num_rational::BigRational::new(
            num_bigint::BigInt::from(3),
            num_bigint::BigInt::from(2),
        )),
    );
    assert_eq!(
        render(statement, &fraction).expect("it renders"),
        "Compute $(3/2)^{2}$."
    );

    let mut decimal = Bindings::new();
    decimal.insert("a".to_string(), Scalar::Text("0.2".to_string()).value());
    assert_eq!(
        render(statement, &decimal).expect("it renders"),
        "Compute $0.2^{2}$."
    );

    let mut negative_decimal = Bindings::new();
    negative_decimal.insert("a".to_string(), Scalar::Text("-0.2".to_string()).value());
    assert_eq!(
        render(statement, &negative_decimal).expect("it renders"),
        "Compute $(-0.2)^{2}$."
    );

    let mut text = Bindings::new();
    let (name, value) = text_binding("a", "\\times");
    text.insert(name, value);
    assert_eq!(
        render("Compute $2 {a} 3$.", &text).expect("it renders"),
        "Compute $2 \\times 3$."
    );
}

/// The statement and the answer of one instance ask and answer one question.
///
/// `a**2` with `a = -3` answers 9. The statement must therefore read `(-3)^{2}`
/// and never `-3^{2}`, which is -9 (M4 review 1, finding 18).
#[test]
fn a_negative_instance_states_the_question_its_answer_answers() {
    let doc = doc_from(
        r#"{"v": 1, "topic_id": "squares-of-negatives", "answer_kind": "numeric",
            "statement": "Compute ${a}^{{2}}$.",
            "params": {"a": {"kind": "int", "low": -12, "high": -1}},
            "answer_expr": "a**2", "hints": ["What sign does a square carry?"],
            "samples": [{"params": {"a": -1}, "expected": "1"}]}"#,
    );
    let compiled = Compiled::new(&doc).expect("the template compiles");
    for (value, text, answer) in [
        (-3, "Compute $(-3)^{2}$.", "9"),
        (-12, "Compute $(-12)^{2}$.", "144"),
    ] {
        let instance = compiled
            .instantiate(bind(&[("a", value)]))
            .expect("the tuple instantiates");
        assert_eq!(instance.text, text);
        assert_eq!(instance.answer, answer);
    }
}

/// M4 review 1, finding 17: the answer writer brackets a power under a power.
///
/// `**` groups to the right, so `x**2**3` reads as `x**(2**3)`. The M2 parser
/// refuses that string as a tower of powers, so the unbracketed form left the
/// decidable grammar and every instance of such a template was refused (V2).
#[test]
fn the_answer_writer_brackets_a_power_that_is_the_base_of_a_power() {
    for (source, wanted) in [
        ("(x**2)**3", "(x**2)**3"),
        ("2*(x**2)**3", "2*(x**2)**3"),
        ("((x + 1)**2)**2", "((1 + x)**2)**2"),
        ("(-x)**2", "(-x)**2"),
        ("(x/2)**3", "(x/2)**3"),
        ("(x*y)**2", "(x*y)**2"),
        ("sqrt(x)**2", "sqrt(x)**2"),
        ("x**2*y**3", "x**2*y**3"),
    ] {
        let ast = cadus_core::template::parse_answer_expr(source)
            .unwrap_or_else(|error| panic!("{source} parses: {error}"));
        let value = cadus_core::template::evaluate(&ast, &Bindings::new())
            .unwrap_or_else(|error| panic!("{source} evaluates: {error}"));
        let written = cadus_core::template::write(&value)
            .unwrap_or_else(|error| panic!("{source} writes: {error}"));
        assert_eq!(written, wanted, "{source}");
        cadus_core::answer::canonical_form(&written)
            .unwrap_or_else(|reason| panic!("{written} does not canonicalize: {reason}"));
    }
}

/// The candidate stream walks past a draw that spends its budget.
///
/// The constraint holds for about one tuple in 2,048, so about six draws in ten
/// spend the 1,000-draw budget. The stream that stopped at the first such draw
/// was empty three runs in five, and the refill then had no instance to insert
/// for a template the gate accepts (M4 review 1, findings 7 and 12).
#[test]
fn a_sparse_candidate_stream_skips_the_draws_that_spend_their_budget() {
    let doc = doc_from(
        r#"{"v": 1, "topic_id": "sparse", "answer_kind": "numeric",
            "statement": "Compute ${a} + {b}$.",
            "params": {"a": {"kind": "int", "low": 4096, "high": 8192},
                       "b": {"kind": "int", "low": 1, "high": 10}},
            "constraints": [{"op": "eq", "left": {"mod": ["a", {"lit": 4096}]},
                             "right": {"lit": 0}}],
            "answer_expr": "a + b", "hints": ["Which column do you add first?"],
            "samples": [{"params": {"a": 4096, "b": 1}, "expected": "4097"}]}"#,
    );
    let compiled = Compiled::new(&doc).expect("the template compiles");
    // The counts are the ones the three seeds produce, and a seeded draw is the
    // same on every machine.
    for (seed, count) in [(1_u64, 11_usize), (5, 10), (9, 6)] {
        let mut rng = rng_from_seed(seed);
        let stream = compiled.candidates(&mut rng).expect("the stream builds");
        assert_eq!(stream.len(), count, "seed {seed}");
        for bindings in &stream {
            assert!(
                cadus_core::template::all_hold(&doc.constraints, bindings).expect("it decides"),
                "the stream never yields a tuple the constraints refuse"
            );
        }
    }
}
