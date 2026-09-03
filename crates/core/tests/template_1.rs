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

mod common;

use common::template::*;

// --------------------------------------------------------------------------
// The four acceptance checks of U1
// --------------------------------------------------------------------------
/// U1 acceptance 1. `docs/plans/M4.md`: the 1.0 template `a**2` over 1..12
/// renders `Compute $7^{2}$.` for `a = 7` with answer `49`.
#[test]
fn perfect_squares_renders_the_1_0_statement_and_answer() {
    let doc = doc_from(&perfect_squares_body());
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
    let doc = doc_from(&squares_of_negatives_body());
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
    let squares = perfect_squares_body();
    let bodies = [
        squares.as_str(),
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
