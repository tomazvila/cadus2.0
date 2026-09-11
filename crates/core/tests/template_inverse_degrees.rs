//! Certified degree-mode inverse tangent, using the existing Approx contract.
#![allow(clippy::unwrap_used)]
use cadus_core::answer::{AnswerContract, canonical_form};
use cadus_core::template::{answer_for_contract, parse_answer_expr};
use std::collections::BTreeMap;

#[test]
fn rounds_across_direct_reduced_reciprocal_and_negative_branches() {
    for (ratio, decimals, expected) in [
        ("0", 1, "0"),
        ("1/13", 1, "4.4"),
        ("3/13", 1, "13"),
        ("1/2", 1, "26.6"),
        ("8/13", 1, "31.6"),
        ("1", 1, "45"),
        ("8/5", 1, "58"),
        ("2", 1, "63.4"),
        ("10", 1, "84.3"),
        ("-1/2", 1, "-26.6"),
        ("-8/5", 1, "-58"),
        ("1/2", 6, "26.565051"),
        ("1", 18, "45"),
        // Independently calculated with mpmath at 90 decimal digits.
        ("1/2", 18, "26.565051177077989352"),
        ("2", 18, "63.434948822922010648"),
        ("65535/65534", 18, "45.000437142112261256"),
        ("1/65535", 18, "0.000874277554110558"),
        ("65535", 18, "89.999125722445889442"),
        ("1/2", 0, "27"),
        ("65535", 1, "90"),
        ("1/65535", 1, "0"),
    ] {
        let ast = parse_answer_expr(&format!("atandeg({ratio})")).unwrap();
        let result = answer_for_contract(
            &ast,
            &BTreeMap::new(),
            Some(&AnswerContract::Approx { decimals }),
        )
        .unwrap();
        assert_eq!(result.canon, canonical_form(expected).unwrap(), "{ratio}");
    }
}

#[test]
fn refuses_wrong_contract_shape_symbolic_ratios_and_excessive_bounds() {
    let ast = parse_answer_expr("atandeg(1/2)").unwrap();
    for policy in [
        None,
        Some(AnswerContract::Exact),
        Some(AnswerContract::Tolerance {
            tolerance: "1/10".into(),
        }),
        Some(AnswerContract::Approx { decimals: 19 }),
    ] {
        assert!(answer_for_contract(&ast, &BTreeMap::new(), policy.as_ref()).is_err());
    }
    for source in [
        "atandeg(x)",
        "atandeg(sqrt(2))",
        "atandeg(pi)",
        "atandeg(65536)",
        "atandeg(1/65536)",
        "atandeg(1/(2-2))",
        "atandeg(1,2)",
    ] {
        assert!(
            answer_for_contract(
                &parse_answer_expr(source).unwrap(),
                &BTreeMap::new(),
                Some(&AnswerContract::Approx { decimals: 1 })
            )
            .is_err()
        );
    }
}
