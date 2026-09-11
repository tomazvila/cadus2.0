//! Evidence for unit00 objectives that the current template schema cannot preserve.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use cadus_core::{
    curriculum::load_curriculum,
    template::{Bindings, answer, parse_answer_expr},
};
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::{Value, json};
use std::path::Path;

fn candidate(statement: &str, expr: &str, values: &[i64], expected: &str) -> Value {
    json!({"statement":statement,"answer_expr":expr,
        "params":{"a":{"kind":"choice","values":values}},"constraints":[],
        "solution_sketch":"Use the requested classification or representation of the given number.",
        "hints":["What representation does this question ask you to give?"],
        "distractors":[],"samples":values.iter().map(|a|json!({"params":{"a":a},
            "expected":expected})).collect::<Vec<_>>()})
}

fn refusal(spec: &cadus_worker::authoring::prompt::AuthoringSpec, args: &Value) -> Value {
    match verify_kind(Kind::Template, spec, args, &[]) {
        Err(e) => json!({"code":e.code,"message":e.message}),
        Ok(_) => panic!(
            "probe unexpectedly passed for {}/{}",
            spec.topic_id, spec.kp_id
        ),
    }
}

fn labels(curriculum: &cadus_core::curriculum::Curriculum) -> Vec<Value> {
    let mut rows = Vec::new();
    for (key, statement, expected, values) in [
        (
            "divisibility-rules/kp1",
            "Is ${a}$ divisible by five? (yes/no)",
            "yes",
            (100..220).step_by(10).collect::<Vec<_>>(),
        ),
        (
            "divisibility-rules/kp2",
            "Is ${a}$ divisible by three? (yes/no)",
            "yes",
            (102..138).step_by(3).collect::<Vec<_>>(),
        ),
        (
            "divisibility-rules/kp3",
            "Is ${a}$ divisible by four? (yes/no)",
            "yes",
            (104..152).step_by(4).collect::<Vec<_>>(),
        ),
        (
            "factors-and-multiples/kp3",
            "Is ${a}$ a multiple of three? (yes/no)",
            "yes",
            (6..42).step_by(3).collect::<Vec<_>>(),
        ),
        (
            "prime-composite-numbers/kp1",
            "Classify ${a}$: prime or composite?",
            "composite",
            (4..28).step_by(2).collect::<Vec<_>>(),
        ),
        (
            "prime-composite-numbers/kp3",
            "Is ${a}$ prime? (yes/no)",
            "no",
            (4..28).step_by(2).collect::<Vec<_>>(),
        ),
    ] {
        let spec = select(curriculum, &[key.to_owned()]).unwrap().remove(0);
        let args = candidate(statement, expected, &values, expected);
        rows.push(json!({"kp_key":key,"blocker":"label_output",
            "constraints":spec.constraints,"shared_contract":spec.template_contract(),
            "required":"Text classification selected from the authored label options.",
            "schema_limit":"answer_expr evaluates mathematical ASTs; it has no label literal or label-valued conditional. Text-choice bindings cannot be evaluated as answers.",
            "probe_arguments":args,"worker_rejection":refusal(&spec,&args)}));
    }
    rows
}

fn remainders(curriculum: &cadus_core::curriculum::Curriculum) -> Vec<Value> {
    let mut rows = Vec::new();
    for (key, d, start) in [
        ("division-with-remainders/kp1", 5, 21),
        ("division-with-remainders/kp2", 7, 22),
        ("long-division-one-digit/kp3", 3, 103),
        ("long-division/kp3", 32, 321),
    ] {
        let spec = select(curriculum, &[key.to_owned()]).unwrap().remove(0);
        let values: Vec<_> = (0..12).map(|i| start + i * d).collect();
        let expr = format!("floor(a/{d}) R(a-{d}*floor(a/{d}))");
        let statement = format!("Compute ${{a}} \\div {d}$. Give quotient and remainder as q Rr.");
        let mut args = candidate(&statement, &expr, &values, "4 R1");
        args["samples"] = json!(
            values
                .iter()
                .map(|a| json!({"params":{"a":a},
            "expected":format!("{} R{}",a/d,a%d)}))
                .collect::<Vec<_>>()
        );
        rows.push(json!({"kp_key":key,"blocker":"quotient_remainder_output",
            "constraints":spec.constraints,"shared_contract":spec.template_contract(),
            "required":"q Rr with an integer quotient and nonzero remainder smaller than the divisor.",
            "schema_limit":"The evaluator can compute quotient and remainder separately, but cannot emit the q Rr representation. These KPs also have divisor-specific exemplar contracts that are not one shared template contract; author-supplied contracts are stripped by worker assembly.",
            "probe_arguments":args,"worker_rejection":refusal(&spec,&args)}));
    }
    rows
}

fn notation(curriculum: &cadus_core::curriculum::Curriculum) -> Vec<Value> {
    let mut rows = Vec::new();
    for (key, expr, wanted) in [
        ("whole-number-exponents/kp1", "3**4", "3^4"),
        ("prime-factorization/kp1", "2*2*3", "2 * 2 * 3"),
        ("prime-factorization/kp2", "2*2*3*5", "2 * 2 * 3 * 5"),
        ("prime-factorization/kp3", "2*2*2*3*3", "2 * 2 * 2 * 3 * 3"),
    ] {
        let spec = select(curriculum, &[key.to_owned()]).unwrap().remove(0);
        let computed = answer(&parse_answer_expr(expr).unwrap(), &Bindings::new()).unwrap();
        assert_ne!(computed.text, wanted);
        let mut binding = Bindings::new();
        binding.insert(
            "a".to_owned(),
            cadus_core::template::Scalar::Text(wanted.to_owned()).value(),
        );
        let text_error = answer(&parse_answer_expr("a").unwrap(), &binding).unwrap_err();
        let mut args = candidate(
            "Write the requested representation of ${a}$.",
            "a",
            &(4..16).collect::<Vec<_>>(),
            wanted,
        );
        args["params"]["a"] = json!({"kind":"choice","values":[wanted]});
        args["samples"] = json!([{"params":{"a":wanted},"expected":wanted}]);
        rows.push(json!({"kp_key":key,"blocker":"evaluated_representation",
            "constraints":spec.constraints,"shared_contract":spec.template_contract(),
            "required":"Preserve the requested exponent notation, repeated multiplication, or product of primes in the emitted answer.",
            "schema_limit":"Evaluation folds closed powers and products to their numerical value. There is no held-expression output node; passing the wanted expression as a text choice is also rejected.",
            "evaluated_probe":{"answer_expr":expr,"required_text":wanted,"actual_text":computed.text},
            "text_binding_error":text_error.to_string(),
            "probe_arguments":args,"worker_rejection":refusal(&spec,&args)}));
    }
    rows
}

fn finite_domains(curriculum: &cadus_core::curriculum::Curriculum) -> Vec<Value> {
    let mut rows = Vec::new();
    for (key, statement, expr, values, reason) in [
        (
            "perfect-squares/kp1",
            "Compute ${a}^{{2}}$.",
            "a**2",
            vec![2, 3, 4, 5, 6, 8, 10, 11],
            "Bases are restricted to 1..12. Authored square calculations already use bases 1, 7, 9, and 12. Eight fresh numerical instances remain. Wording variants cannot enlarge that mathematical family to the required twelve.",
        ),
        (
            "factors-and-multiples/kp2",
            "List the first five positive multiples of ${a}$.",
            "(a,2*a,3*a,4*a,5*a)",
            (2..13).collect(),
            "The base is restricted to 2..12, giving eleven values per fixed list length. Allowed lengths are three through five. Template collection nodes have a fixed number of children, with no parameter-dependent length; even the unused five-multiple family has only eleven instances.",
        ),
    ] {
        let spec = select(curriculum, &[key.to_owned()]).unwrap().remove(0);
        let mut args = candidate(statement, expr, &values, "1");
        args["samples"] = json!(
            values
                .iter()
                .map(|a| {
                    let expected = if key == "perfect-squares/kp1" {
                        (a * a).to_string()
                    } else {
                        (1..=5)
                            .map(|m| (a * m).to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    json!({"params":{"a":a},"expected":expected})
                })
                .collect::<Vec<_>>()
        );
        rows.push(
            json!({"kp_key":key,"blocker":"finite_domain_below_gate_floor",
            "constraints":spec.constraints,"required_distinct_instances":12,
            "fresh_instances_in_probe":values.len(),"schema_limit":reason,
            "probe_arguments":args,"worker_rejection":refusal(&spec,&args)}),
        );
    }
    rows
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let mut rows = labels(&curriculum);
    rows.extend(remainders(&curriculum));
    rows.extend(notation(&curriculum));
    rows.extend(finite_domains(&curriculum));
    rows.sort_by_key(|r| r["kp_key"].as_str().unwrap().to_owned());
    assert_eq!(rows.len(), 16);
    println!("{}", serde_json::to_string_pretty(&rows).unwrap());
}
