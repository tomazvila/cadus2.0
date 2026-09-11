//! Geometry recipes vary the mathematical task beyond a common length scale.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use num_integer::Integer;
use serde_json::Value;

fn arguments(key: &str) -> Value {
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "../../../docs/content-foundations/template19-production-gate/drafts.json"
    ))
    .unwrap();
    rows.into_iter().find(|row| row["kp_id"] == key).unwrap()["arguments"].clone()
}

#[test]
fn triangle_classification_covers_twelve_nonsimilar_triangles_and_every_class() {
    let arguments = arguments("pythagorean-converse/kp3");
    let mut shapes = BTreeSet::new();
    let mut classes = BTreeSet::new();
    for sample in arguments["samples"].as_array().unwrap() {
        let a = sample["params"]["a"].as_i64().unwrap();
        let b = sample["params"]["b"].as_i64().unwrap();
        // The statement gives a, b, b+1. Normalize all three sides together
        // so multiplying an existing triangle by a common scale adds no shape.
        let mut sides = [a, b, b + 1];
        sides.sort();
        assert!(sides[0] > 0 && sides[0] + sides[1] > sides[2]);
        let divisor = sides[0].gcd(&sides[1]).gcd(&sides[2]);
        shapes.insert(sides.map(|side| side / divisor));
        let left = sides[0].pow(2) + sides[1].pow(2);
        let right = sides[2].pow(2);
        let class = match left.cmp(&right) {
            std::cmp::Ordering::Less => "obtuse",
            std::cmp::Ordering::Equal => "right",
            std::cmp::Ordering::Greater => "acute",
        };
        assert_eq!(sample["expected"], class);
        classes.insert(class);
    }
    assert!(
        shapes.len() >= 12,
        "scale-only triangle variations: {shapes:?}"
    );
    assert_eq!(classes, BTreeSet::from(["acute", "right", "obtuse"]));
}

#[test]
fn angle_context_covers_twelve_ratios_and_independently_rounded_angles() {
    let arguments = arguments("trig-applications/kp3");
    let contract: AnswerContract =
        serde_json::from_value(arguments["answer_contract"].clone()).unwrap();
    assert_eq!(contract, AnswerContract::Approx { decimals: 1 });
    let mut ratios = BTreeSet::new();
    let mut angles = BTreeSet::new();
    for sample in arguments["samples"].as_array().unwrap() {
        let rise = i32::try_from(sample["params"]["a"].as_i64().unwrap()).unwrap();
        let run = 17;
        let divisor = rise.gcd(&run);
        ratios.insert((rise / divisor, run / divisor));
        // Independent platform atan recomputation checks the certified writer's
        // one-decimal answers; every tested value is away from a rounding tie.
        let angle = (f64::from(rise) / f64::from(run)).atan().to_degrees();
        let rounded = format!("{angle:.1}");
        let expected = sample["expected"].as_str().unwrap();
        assert!(matches!(
            check_contract(&rounded, expected, contract.clone()),
            Outcome::Decided(verdict) if verdict.correct
        ));
        angles.insert(rounded);
    }
    assert!(
        ratios.len() >= 12,
        "scale-only angle variations: {ratios:?}"
    );
    assert!(angles.len() >= 12, "repeated angle outcomes: {angles:?}");
}
