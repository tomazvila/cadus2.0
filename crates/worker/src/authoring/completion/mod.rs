//! Zero-model proposal generation. Production gates validate drafts; independent AI review authorizes serving.
mod evidence;
mod expression;
mod instruction;
mod method;
mod templates;

pub use evidence::diagnosis_from_templates;

use crate::authoring::{
    job::verify_kind,
    prompt::{AuthoringSpec, Kind},
};
use cadus_core::instruction::{ServedInstance, template_instances};
use serde_json::{Value, json};
use std::collections::BTreeSet;

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

fn add_teaching_candidates(
    out: &mut Proposals,
    spec: &AuthoringSpec,
    served: &[ServedInstance],
    exemplar_index: usize,
    variants: &[(i64, expression::Calculation)],
    mut teaching: bool,
) -> bool {
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
        if teaching {
            out.assessment.push(json!({"problem":problem,"answer":calculation.answer,"source_exemplar":exemplar_index,"review_status":"candidate only; hold-out designation and family independence unverified"}));
        } else {
            let arguments = json!({"concept":method::rule(spec),"worked_example":{"problem":problem,"steps":[method::rule(spec),format!("Evaluate the given operations in ${}$ exactly.",calculation.expression),calculation.answer]}});
            teaching = keep(out, spec, Kind::Teach, arguments, served);
        }
    }
    teaching
}

fn add_practice_candidate(
    out: &mut Proposals,
    spec: &AuthoringSpec,
    served: &[ServedInstance],
    source: &expression::Calculation,
    parameter_span: std::ops::Range<usize>,
    variants: &[(i64, expression::Calculation)],
    practice: bool,
) -> bool {
    if practice || variants.len() < 12 {
        return practice;
    }
    let Some(parameter) = ['a', 'b', 'c', 'f', 'g', 'h', 'm', 'p', 'q', 's']
        .into_iter()
        .find(|letter| !source.expression.contains(*letter))
    else {
        return practice;
    };
    let candidates: Vec<_> = variants
        .iter()
        .filter(|(_, calculation)| {
            let problem = format!("Compute ${}$.", calculation.expression);
            spec.exemplars
                .iter()
                .all(|exemplar| exemplar.problem.trim() != problem.trim())
        })
        .take(12)
        .collect();
    if candidates.len() < 12 {
        return practice;
    }
    let formula = format!(
        "{}{parameter}{}",
        &source.expression[..parameter_span.start],
        &source.expression[parameter_span.end..]
    );
    let statement = format!(
        "Compute ${}{{{parameter}}}{}$.",
        expression::escape(&source.expression[..parameter_span.start]),
        expression::escape(&source.expression[parameter_span.end..])
    );
    let values: Vec<i64> = candidates.iter().map(|(value, _)| *value).collect();
    let samples: Vec<Value> = candidates
        .iter()
        .map(|(value, calculation)| {
            json!({"params":{parameter.to_string():value},"expected":calculation.answer})
        })
        .collect();
    let arguments = json!({"statement":statement,"params":{parameter.to_string():{"kind":"choice","values":values}},"constraints":[],"answer_expr":formula,"solution_sketch":method::rule(spec),"hints":[method::rule(spec)],"distractors":[],"samples":samples});
    keep(out, spec, Kind::Template, arguments, served)
}

/// Derive closed exact arithmetic candidates and topic-specific hint scaffolds.
/// Explicit policies validate authored source answers; unsupported models remain refused.
#[must_use]
pub fn generate(spec: &AuthoringSpec, served: &[ServedInstance]) -> Proposals {
    let mut out = Proposals::default();
    instruction::add_reviewed_hint(&mut out, spec, served);
    if !out.drafts.iter().any(|row| row["kind"] == "hint_ladder") {
        keep(
            &mut out,
            spec,
            Kind::HintLadder,
            method::hints(spec),
            served,
        );
    }
    let mut teaching = false;
    let mut practice = false;
    for (index, exemplar) in spec.exemplars.iter().enumerate() {
        let Some(source) = expression::source(exemplar) else {
            continue;
        };
        out.solutions.push(json!({"exemplar_index":index,"problem":exemplar.problem,"answer":exemplar.answer,"answer_contract":exemplar.answer_contract,
            "solution_sketch":format!("{} The exact calculation is ${} = {}$.", method::rule(spec), source.expression, exemplar.answer),
            "verification":"closed rational expression equals the authored answer under its typed policy; instructional detail requires independent AI review"}));
        let Some((start, end, variants)) = expression::variants(&source) else {
            continue;
        };
        teaching = add_teaching_candidates(&mut out, spec, served, index, &variants, teaching);
        practice = add_practice_candidate(
            &mut out,
            spec,
            served,
            &source,
            start..end,
            &variants,
            practice,
        );
    }
    if !practice && let Some(arguments) = templates::special(spec) {
        keep(&mut out, spec, Kind::Template, arguments, served);
    }
    separate_candidates(&mut out, spec, served);
    teaching = out.drafts.iter().any(|row| row["kind"] == "teach");
    practice = out.drafts.iter().any(|row| row["kind"] == "template");
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

/// Keep teaching and assessment outside every authored or generated practice item.
fn separate_candidates(out: &mut Proposals, spec: &AuthoringSpec, served: &[ServedInstance]) {
    let mut practice = served.to_vec();
    for draft in &out.drafts {
        if draft["kind"] != "template" {
            continue;
        }
        if let Ok(body) = verify_kind(Kind::Template, spec, &draft["arguments"], served) {
            practice.extend(template_instances(&body));
        }
    }
    let before = out.drafts.len();
    out.drafts.retain(|draft| {
        draft["kind"] != "teach"
            || verify_kind(Kind::Teach, spec, &draft["arguments"], &practice).is_ok()
    });
    if out.drafts.len() != before {
        out.refusals.push(
            "teach: worked-example-collision: candidate overlaps authored or practice evidence"
                .to_owned(),
        );
    }
    let mut used: BTreeSet<String> = spec
        .exemplars
        .iter()
        .map(|item| item.problem.trim().to_owned())
        .chain(practice.iter().map(|item| item.problem.trim().to_owned()))
        .collect();
    for draft in &out.drafts {
        if let Some(problem) = draft["arguments"]["worked_example"]["problem"].as_str() {
            used.insert(problem.trim().to_owned());
        }
    }
    out.assessment.retain(|candidate| {
        candidate["problem"]
            .as_str()
            .is_some_and(|problem| used.insert(problem.trim().to_owned()))
    });
}
