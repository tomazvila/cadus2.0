//! The refusal sites of the domains, the draws, the renderer, the document,
//! and the diagnosis gate, reached one by one (D6, C4).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::domain_edges::*;

#[test]
fn a_scalar_has_a_text_and_a_rational() {
    assert_eq!(Scalar::Int(3).text(), "3");
    assert_eq!(
        Scalar::Int(3).rational(),
        Some(BigRational::from_integer(BigInt::from(3)))
    );
    assert_eq!(
        Scalar::Text("1.5".to_string()).rational(),
        Some(BigRational::new(BigInt::from(3), BigInt::from(2)))
    );
    assert_eq!(Scalar::Text("x".to_string()).rational(), None);
}

#[test]
fn a_literal_with_a_zero_or_a_symbolic_side_is_refused() {
    assert_eq!(literal_to_rational("1/0"), None);
    assert_eq!(literal_to_rational("x/2"), None);
    assert_eq!(literal_to_rational("1/y"), None);
    assert_eq!(
        literal_to_rational("3/4"),
        Some(BigRational::new(BigInt::from(3), BigInt::from(4)))
    );
}

#[test]
fn a_number_and_a_text_never_compare_equal_and_a_number_sorts_first() {
    let plus = Value::Text("+".to_string());
    assert_ne!(num(1), Value::Text("1".to_string()));
    assert!(num(1) < plus);
    assert!(plus > num(1));
    assert!(plus < Value::Text("-".to_string()));
    assert_eq!(plus, Value::Text("+".to_string()));
}

#[test]
fn the_empty_choice_and_the_zero_scale() {
    let empty = Domain::Choice { values: vec![] };
    assert_eq!(empty.size("a"), Err(DomainError::EmptyChoice));
    assert_eq!(empty.values("a"), Err(DomainError::EmptyChoice));
    let whole = Domain::Decimal {
        low: -5,
        high: 5,
        scale: 0,
    };
    let values = whole.values("d").expect("the domain lists");
    assert_eq!(values.len(), 11);
    assert_eq!(values[0].canonical_string(), "-5");
    assert_eq!(values[10].canonical_string(), "5");
}

#[test]
fn every_domain_error_through_size_and_values() {
    let too_many_choices = Domain::Choice {
        values: (0..10_001).map(Scalar::Int).collect(),
    };
    let cases: [(Domain, DomainError); 9] = [
        (int(9, 2), DomainError::EmptyRange { low: 9, high: 2 }),
        (
            too_many_choices,
            DomainError::TooLarge {
                name: "a".to_string(),
                count: 10_001,
            },
        ),
        (
            Domain::Rational {
                num: IntRange { low: 1, high: 3 },
                den: IntRange { low: -1, high: 1 },
            },
            DomainError::ZeroDenominator { low: -1, high: 1 },
        ),
        (
            Domain::Rational {
                num: IntRange { low: 5, high: 1 },
                den: IntRange { low: 1, high: 2 },
            },
            DomainError::EmptyRange { low: 5, high: 1 },
        ),
        (
            Domain::Rational {
                num: IntRange { low: 1, high: 2 },
                den: IntRange { low: 5, high: 1 },
            },
            DomainError::EmptyRange { low: 5, high: 1 },
        ),
        (
            Domain::Rational {
                num: IntRange { low: 1, high: 200 },
                den: IntRange { low: 1, high: 100 },
            },
            DomainError::TooLarge {
                name: "a".to_string(),
                count: 20_000,
            },
        ),
        (
            Domain::Decimal {
                low: 1,
                high: 9,
                scale: 10,
            },
            DomainError::DecimalScale { scale: 10 },
        ),
        (
            Domain::Decimal {
                low: 9,
                high: 2,
                scale: 1,
            },
            DomainError::EmptyRange { low: 9, high: 2 },
        ),
        (
            Domain::Decimal {
                low: 1,
                high: 20_000,
                scale: 1,
            },
            DomainError::TooLarge {
                name: "a".to_string(),
                count: 20_000,
            },
        ),
    ];
    for (domain, error) in cases {
        assert_eq!(domain.size("a"), Err(error.clone()), "{domain:?}");
        assert_eq!(domain.values("a"), Err(error), "{domain:?}");
    }
}

#[test]
fn the_space_helpers_report_a_bad_domain_and_a_constraint_they_cannot_decide() {
    let bad = params(&[("a", int(9, 2))]);
    let empty = DomainError::EmptyRange { low: 9, high: 2 };
    assert_eq!(declared_space(&bad), Err(empty.clone()));
    assert_eq!(enumerate(&bad, 4_096), Err(empty.clone()));
    assert_eq!(walk_satisfying(&bad, &[]), Err(empty.clone()));
    assert_eq!(space_size(&bad, &[]), Err(empty.clone()));
    let nine = params(&[("a", int(1, 3)), ("b", int(1, 3))]);
    assert_eq!(
        enumerate(&nine, 4),
        Err(DomainError::TooLarge {
            name: "b".to_string(),
            count: 9,
        })
    );
    // A domain past the exhaustive limit, then a bad one.
    let late = params(&[("a", int(1, 10_000)), ("b", int(9, 2))]);
    assert_eq!(walk_satisfying(&late, &[]), Err(empty));
    // The exhaustive walk and the sampled walk both report an undecidable constraint.
    let small = params(&[("a", int(1, 2)), ("op", operators())]);
    assert_eq!(
        walk_satisfying(&small, &[text_as_number()]),
        Err(not_numeric())
    );
    let large = params(&[("a", int(1, 5_000)), ("op", operators())]);
    assert_eq!(
        walk_satisfying(&large, &[text_as_number()]),
        Err(not_numeric())
    );
}

#[test]
fn the_draw_helpers_and_the_bounded_draw() {
    let mut rng = rng_from_seed(7);
    let plan = params(&[("a", int(1, 3))]);
    let drawn = draw_bindings(&plan, &mut rng).expect("draws");
    assert!(drawn.contains_key("a"));
    assert_eq!(
        draw_bindings(&params(&[("a", int(9, 2))]), &mut rng),
        Err(DomainError::EmptyRange { low: 9, high: 2 })
    );
    let never = Constraint {
        op: Cmp::Gt,
        left: Term::Param("a".to_string()),
        right: Term::Lit(BigRational::from_integer(BigInt::from(5))),
    };
    assert_eq!(
        draw_satisfying(&plan, &[never], &mut rng),
        Err(DrawError::NoSatisfyingTuple { attempts: 1_000 })
    );
    let texts = params(&[("op", operators())]);
    assert_eq!(
        draw_satisfying(&texts, &[text_as_number()], &mut rng),
        Err(DrawError::Constraint(
            cadus_core::template::ConstraintError::NotNumeric {
                name: "op".to_string(),
                text: "+".to_string(),
            }
        ))
    );
    assert_eq!(below(&mut rng, 0), 0);
    assert_eq!(below(&mut rng, 1), 0);
    // A bound just above half the range rejects about one draw in two, so the
    // rejection loop of the multiply-and-shift method runs.
    let bound = (1_u64 << 63) + 1;
    for _ in 0..64 {
        assert!(below(&mut rng, bound) < bound);
    }
}

#[test]
fn the_candidate_stream_reports_a_bad_domain_and_an_undecidable_constraint() {
    let mut rng = rng_from_seed(1);
    let bad = params(&[("a", int(9, 2))]);
    let empty = DomainError::EmptyRange { low: 9, high: 2 };
    assert_eq!(
        candidates(&bad, &[], &mut rng),
        Err(DrawError::Domain(empty.clone()))
    );
    let late = params(&[("a", int(1, 10_000)), ("b", int(9, 2))]);
    assert_eq!(
        candidates(&late, &[], &mut rng),
        Err(DrawError::Domain(empty))
    );
    let refused = DrawError::Constraint(cadus_core::template::ConstraintError::NotNumeric {
        name: "op".to_string(),
        text: "+".to_string(),
    });
    let small = params(&[("a", int(1, 2)), ("op", operators())]);
    assert_eq!(
        candidates(&small, &[text_as_number()], &mut rng),
        Err(refused.clone())
    );
    let large = params(&[("a", int(1, 5_000)), ("op", operators())]);
    assert_eq!(
        candidates(&large, &[text_as_number()], &mut rng),
        Err(refused)
    );
    let plan = DrawPlan::new(&small).expect("the plan builds");
    assert_eq!(plan.declared_space(), 4);
}
