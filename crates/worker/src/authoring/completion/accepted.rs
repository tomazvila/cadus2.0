//! Independently evaluable recipes for reviewed curriculum additions.
use cadus_core::template::{Bindings, Scalar, answer, parse_answer_expr};
use serde_json::{Map, Value, json};

#[allow(clippy::expect_used)]
fn catalog() -> Vec<Value> {
    serde_json::from_str(include_str!("accepted_templates.json"))
        .expect("the checked-in accepted-template catalog is valid JSON")
}

pub(super) fn arguments(key: &str) -> Option<Value> {
    let recipe = catalog().into_iter().find(|item| item["key"] == key)?;
    let statement = recipe["statement"].as_str()?;
    let answer_expr = recipe["answer_expr"].as_str()?;
    let solution = recipe["solution"].as_str()?;
    let hint = recipe["hint"].as_str()?;
    let values = recipe["values"].as_array()?;
    let ast = parse_answer_expr(answer_expr).ok()?;
    let mut samples = Vec::with_capacity(values.len());
    let mut choices = Vec::with_capacity(values.len());
    for item in values {
        let value = item.as_i64()?;
        let scalar = Scalar::Int(value);
        let mut bindings = Bindings::new();
        bindings.insert("a".to_owned(), scalar.value());
        let expected = answer(&ast, &bindings).ok()?.text;
        samples.push(json!({"params":{"a":scalar},"expected":expected}));
        choices.push(Scalar::Int(value));
    }
    let mut params = Map::new();
    params.insert("a".to_owned(), json!({"kind":"choice","values":choices}));
    Some(json!({
        "statement": statement,
        "params": params,
        "constraints": [],
        "answer_expr": answer_expr,
        "solution_sketch": solution,
        "hints": [hint],
        "distractors": [],
        "samples": samples,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn rejected_weak_or_mismatched_families_stay_out() {
        let keys: BTreeSet<_> = catalog()
            .into_iter()
            .filter_map(|item| item["key"].as_str().map(str::to_owned))
            .collect();
        for key in [
            "prime-composite-numbers/kp3",
            "whole-number-exponents/kp1",
            "discriminant/kp2",
            "multiplying-binomials/kp3",
            "synthetic-division/kp1",
            "synthetic-division/kp2",
            "evaluating-functions/kp1",
            "function-composition/kp3",
            "function-notation/kp2",
            "graphs-of-logarithmic-functions/kp1",
            "graphs-of-logarithmic-functions/kp2",
            "natural-exponential-function/kp3",
            "angle-of-elevation-depression/kp1",
            "angle-of-elevation-depression/kp2",
            "angle-of-elevation-depression/kp3",
            "complementary-angle-trig/kp3",
            "multiplying-dividing-rational-expressions/kp1",
            "rational-expression-restrictions/kp1",
            "rational-expressions/kp2",
            "right-triangle-trig/kp1",
            "right-triangle-trig/kp3",
            "special-right-triangles/kp1",
            "trig-ratios-definition/kp1",
            "trig-ratios-definition/kp2",
            "complex-fractions/kp2",
        ] {
            assert!(!keys.contains(key), "rejected recipe returned: {key}");
        }
    }

    #[test]
    fn catalog_keys_and_values_are_unique() {
        let recipes = catalog();
        let keys: BTreeSet<_> = recipes
            .iter()
            .filter_map(|item| item["key"].as_str())
            .collect();
        assert_eq!(keys.len(), recipes.len());
        for recipe in recipes {
            let key = recipe["key"].as_str().expect("key");
            let values = recipe["values"].as_array().expect("values");
            let unique: BTreeSet<_> = values.iter().filter_map(Value::as_i64).collect();
            assert_eq!(unique.len(), values.len(), "{key}");
            assert!(values.len() >= 12, "{key}");
        }
    }
}
