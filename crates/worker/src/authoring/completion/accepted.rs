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
    use cadus_core::template::render::render;
    use std::collections::BTreeSet;

    fn make_bindings(a: i64) -> Bindings {
        let mut b = Bindings::new();
        b.insert("a".to_owned(), Scalar::Int(a).value());
        b
    }

    fn recipe(key: &str) -> Value {
        catalog()
            .into_iter()
            .find(|item| item["key"] == key)
            .unwrap_or_else(|| panic!("recipe {key} not found"))
    }

    // Regression guards: digit concatenation and missing-time assumptions.

    #[test]
    fn function_notation_kp3_renders_explicit_multiplication() {
        // a=12 -> f(2*12) not f(212), answer 6*12-2 = 70.
        let r = recipe("function-notation/kp3");
        let statement = r["statement"].as_str().unwrap();
        let rendered = render(statement, &make_bindings(12)).unwrap();
        assert!(
            rendered.contains("2*12"),
            "expected explicit 2*12 in rendered statement, got: {rendered}"
        );
        assert!(
            !rendered.contains("212"),
            "digit concatenation: statement should not contain 212: {rendered}"
        );
        let ast = parse_answer_expr(r["answer_expr"].as_str().unwrap()).unwrap();
        let ans = answer(&ast, &make_bindings(12)).unwrap();
        assert_eq!(ans.text, "70", "f(2*12) = 3*(24)-2 = 70");
    }

    #[test]
    fn solving_right_triangles_sides_kp1_renders_explicit_multiplication() {
        // a=12 -> 5*12 not 512.
        let r = recipe("solving-right-triangles-sides/kp1");
        let statement = r["statement"].as_str().unwrap();
        let rendered = render(statement, &make_bindings(12)).unwrap();
        assert!(
            rendered.contains("5*12"),
            "expected explicit 5*12 in rendered statement, got: {rendered}"
        );
        assert!(
            !rendered.contains("512"),
            "digit concatenation: statement should not contain 512: {rendered}"
        );
        let ast = parse_answer_expr(r["answer_expr"].as_str().unwrap()).unwrap();
        let ans = answer(&ast, &make_bindings(12)).unwrap();
        assert_eq!(ans.text, "36", "3*12 = 36");
    }

    #[test]
    fn gcf_lcm_kp3_statement_makes_time_assumption_explicit() {
        // Statement must say both events start together and use 2*{a}.
        let r = recipe("gcf-lcm/kp3");
        let statement = r["statement"].as_str().unwrap();
        let rendered = render(statement, &make_bindings(13)).unwrap();
        assert!(
            rendered.contains("both occur together now"),
            "rendered statement must mention the shared starting time: {rendered}"
        );
        assert!(
            rendered.contains("2*13"),
            "rendered should show 2*13, got: {rendered}"
        );
        assert!(
            !rendered.contains("twice"),
            "avoid ambiguous twice- wording: {rendered}"
        );
        let ast = parse_answer_expr(r["answer_expr"].as_str().unwrap()).unwrap();
        let ans = answer(&ast, &make_bindings(13)).unwrap();
        assert_eq!(ans.text, "26", "2*13 = 26");
    }

    #[test]
    fn evaluating_polynomials_kp3_renders_negative_parameter() {
        // Renderer wraps negative values: x^2-(-6) at x=5 gives answer 31.
        let r = recipe("evaluating-polynomials/kp3");
        let statement = r["statement"].as_str().unwrap();
        let rendered = render(statement, &make_bindings(-6)).unwrap();
        assert!(
            rendered.contains("(-6)"),
            "rendered should bracket negative value: {rendered}"
        );
        assert!(
            !rendered.contains("--"),
            "no unparenthesized double minus: {rendered}"
        );
        let ast = parse_answer_expr(r["answer_expr"].as_str().unwrap()).unwrap();
        let ans = answer(&ast, &make_bindings(-6)).unwrap();
        assert_eq!(ans.text, "31", "25-(-6) = 31");
    }

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