//! Explicit curriculum-course selection for offline author passes.
use crate::authoring::{
    cli::{AuthorArgs, CliError},
    prompt::AuthoringSpec,
};
use cadus_core::{
    curriculum::{Curriculum, KnowledgePoint, Topic},
    pool::split_kp_key,
};

/// Resolve explicit keys and an optional course without a silent scope mismatch.
///
/// # Errors
/// Reject unknown courses, unknown keys, and keys outside the selected course.
pub fn select_for(
    curriculum: &Curriculum,
    args: &AuthorArgs,
) -> Result<Vec<AuthoringSpec>, CliError> {
    let specs = select(curriculum, &args.kps)?;
    let Some(course) = &args.course else {
        return Ok(specs);
    };
    if curriculum.course(course).is_none() {
        return Err(CliError(format!("unknown course {course}")));
    }
    let selected: Vec<_> = specs
        .into_iter()
        .filter(|spec| {
            curriculum
                .idx_of(&spec.topic_id)
                .is_some_and(|topic| curriculum.course_of(topic) == course)
        })
        .collect();
    if !args.kps.is_empty() && selected.len() != args.kps.len() {
        return Err(CliError(
            "explicit knowledge point is outside the selected course".to_owned(),
        ));
    }
    Ok(selected)
}

/// The authoring specs of the knowledge points the operator named.
///
/// An empty `keys` list selects EVERY knowledge point of the tree, in curriculum
/// order. A named key selects one, and the answer keeps the order the operator
/// wrote.
///
/// Every spec states no difficulty target. The curriculum carries a topic
/// difficulty number, not the sentence the prompt asks for, so the prompt takes
/// `prompt::DEFAULT_DIFFICULTY`.
///
/// # Errors
///
/// Returns [`CliError`] naming a key the curriculum does not hold.
pub fn select(curriculum: &Curriculum, keys: &[String]) -> Result<Vec<AuthoringSpec>, CliError> {
    if keys.is_empty() {
        return Ok(every_spec(curriculum));
    }
    let mut specs = Vec::with_capacity(keys.len());
    for key in keys {
        specs.push(one_spec(curriculum, key)?);
    }
    Ok(specs)
}

/// Every knowledge point of the tree, in curriculum order.
fn every_spec(curriculum: &Curriculum) -> Vec<AuthoringSpec> {
    let mut specs = Vec::new();
    for topic in curriculum.topics() {
        for kp in &topic.knowledge_points {
            specs.push(spec_of(topic, kp));
        }
    }
    specs
}

/// The spec of one serving key, or the refusal an unknown key earns.
fn one_spec(curriculum: &Curriculum, key: &str) -> Result<AuthoringSpec, CliError> {
    let Some((topic_id, kp_id)) = split_kp_key(key) else {
        return Err(CliError(format!(
            "the knowledge point `{key}` is not a serving key — write it as `<topic_id>/<kp_id>`"
        )));
    };
    curriculum
        .topics()
        .iter()
        .find(|topic| topic.id.as_str() == topic_id)
        .and_then(|topic| {
            topic
                .knowledge_points
                .iter()
                .find(|kp| kp.id.as_str() == kp_id)
                .map(|kp| spec_of(topic, kp))
        })
        .ok_or_else(|| CliError(format!("the curriculum holds no knowledge point `{key}`")))
}

/// One curriculum knowledge point, as the spec the prompt reads.
fn spec_of(topic: &Topic, kp: &KnowledgePoint) -> AuthoringSpec {
    AuthoringSpec {
        kp_id: kp.id.as_str().to_owned(),
        kp_name: kp.name.clone(),
        topic_id: topic.id.as_str().to_owned(),
        topic_name: topic.name.clone(),
        answer_kind: topic.answer_kind,
        difficulty_target: None,
        constraints: kp.constraints.clone(),
        exemplars: kp.exemplars.clone(),
    }
}
