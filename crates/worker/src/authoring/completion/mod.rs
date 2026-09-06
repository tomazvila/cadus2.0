//! Zero-model proposal generation. Production gates validate drafts; humans approve them.
mod expression;
mod method;
mod templates;

use crate::authoring::{
    job::verify_kind,
    prompt::{AuthoringSpec, Kind},
};
use cadus_core::instruction::ServedInstance;
use serde_json::{Value, json};

/// Drafts and evidence for one knowledge point; no database mutation occurs here.
#[derive(Debug, Default)]
pub struct Proposals {
    /// Normal local-import draft rows, already passed through their production gate.
    pub drafts: Vec<Value>,
    /// Per-kind or derivation refusals; they remain explicit review work.
    pub refusals: Vec<String>,
    /// Exact-answer source solution proposals; these never change curriculum automatically.
    pub solutions: Vec<Value>,
    /// Fresh candidate exemplars; family independence still needs review.
    pub assessment: Vec<Value>,
}

fn keep(
    out: &mut Proposals,
    spec: &AuthoringSpec,
    kind: Kind,
    arguments: Value,
    served: &[ServedInstance],
) -> bool {
    match verify_kind(kind, spec, &arguments, served) {
        Ok(_) => {
            out.drafts.push(json!({"kp_id":format!("{}/{}",spec.topic_id,spec.kp_id),"kind":kind.as_str(),"arguments":arguments}));
            true
        }
        Err(reason) => {
            out.refusals.push(format!(
                "{}: {}: {}",
                kind.as_str(),
                reason.code,
                reason.message
            ));
            false
        }
    }
}

/// Derive closed exact arithmetic candidates and topic-specific hint scaffolds.
/// Explicit policies validate authored source answers; unsupported models remain refused.
#[must_use]
pub fn generate(spec: &AuthoringSpec, served: &[ServedInstance]) -> Proposals {
    let mut out = Proposals::default();
    keep(
        &mut out,
        spec,
        Kind::HintLadder,
        method::hints(spec),
        served,
    );
    let mut teaching = false;
    let mut practice = false;
    for (index, exemplar) in spec.exemplars.iter().enumerate() {
        let Some(source) = expression::source(exemplar) else {
            continue;
        };
        out.solutions.push(json!({"exemplar_index":index,"problem":exemplar.problem,"answer":exemplar.answer,"answer_contract":exemplar.answer_contract,
            "solution_sketch":format!("{} The exact calculation is ${} = {}$.", method::rule(spec), source.expression, exemplar.answer),
            "verification":"closed rational expression equals the authored answer under its typed policy; instructional detail requires human review"}));
        let Some((start, end, variants)) = expression::variants(&source) else {
            continue;
        };
        // Reserve candidate positions beyond the practice bank for teaching and assessment.
        for (_, calculation) in variants.iter().skip(12).take(4) {
            let problem = format!("Compute ${}$.", calculation.expression);
            if spec
                .exemplars
                .iter()
                .any(|item| item.problem.trim() == problem.trim())
                || served
                    .iter()
                    .any(|item| item.problem.trim() == problem.trim())
            {
                continue;
            }
            if !teaching {
                let arguments = json!({"concept":method::rule(spec),"worked_example":{"problem":problem,"steps":[method::rule(spec),format!("Evaluate the given operations in ${}$ exactly.",calculation.expression),calculation.answer]}});
                teaching = keep(&mut out, spec, Kind::Teach, arguments, served);
            } else {
                out.assessment.push(json!({"problem":problem,"answer":calculation.answer,"source_exemplar":index,"review_status":"candidate only; hold-out designation and family independence unverified"}));
            }
        }
        if !practice && variants.len() >= 12 {
            let Some(parameter) = ['a', 'b', 'c', 'f', 'g', 'h', 'm', 'p', 'q', 's']
                .into_iter()
                .find(|letter| !source.expression.contains(*letter))
            else {
                continue;
            };
            let formula = format!(
                "{}{parameter}{}",
                &source.expression[..start],
                &source.expression[end..]
            );
            let statement = format!(
                "Compute ${}{{{parameter}}}{}$.",
                expression::escape(&source.expression[..start]),
                expression::escape(&source.expression[end..])
            );
            let values: Vec<i64> = variants.iter().take(12).map(|(value, _)| *value).collect();
            let samples: Vec<Value> = variants.iter().take(12).map(|(value,calc)|json!({"params":{parameter.to_string():value},"expected":calc.answer})).collect();
            let arguments = json!({"statement":statement,"params":{parameter.to_string():{"kind":"choice","values":values}},"constraints":[],"answer_expr":formula,"solution_sketch":method::rule(spec),"hints":[method::rule(spec)],"distractors":[],"samples":samples});
            practice = keep(&mut out, spec, Kind::Template, arguments, served);
        }
    }
    if !practice && let Some(arguments) = templates::special(spec) {
        practice = keep(&mut out, spec, Kind::Template, arguments, served);
    }
    if !teaching {
        out.refusals.push(
            "teach: no distinct, closed rational worked example passed the production gate"
                .to_owned(),
        );
    }
    if !practice {
        out.refusals.push(
            "template: no parameterized exact-expression scaffold passed the production gate"
                .to_owned(),
        );
    }
    if out.solutions.is_empty() {
        out.refusals.push("solutions: no authored closed-rational calculation could be verified; preserve existing sketches and review this objective".to_owned());
    }
    out
}
