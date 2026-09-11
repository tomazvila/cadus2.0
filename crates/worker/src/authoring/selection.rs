//! Explicit curriculum-course selection for offline author passes.
use crate::authoring::{
    cli::{AuthorArgs, CliError},
    prompt::{AuthoringSpec, FiniteAuthoringPolicy},
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
    let mut keys = args.kps.clone();
    if let Some(path) = &args.kp_file {
        let source = std::fs::read_to_string(path)
            .map_err(|error| CliError(format!("cannot read kp file {path}: {error}")))?;
        keys.extend(
            source
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(str::to_owned),
        );
        if keys.is_empty() {
            return Err(CliError("kp file selects no knowledge points".to_owned()));
        }
    }
    let mut seen = std::collections::HashSet::new();
    keys.retain(|key| seen.insert(key.clone()));
    let specs = select(curriculum, &keys)?;
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
    if !keys.is_empty() && selected.len() != keys.len() {
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
        return every_spec(curriculum);
    }
    let mut specs = Vec::with_capacity(keys.len());
    for key in keys {
        specs.push(one_spec(curriculum, key)?);
    }
    Ok(specs)
}

/// Every knowledge point of the tree, in curriculum order.
fn every_spec(curriculum: &Curriculum) -> Result<Vec<AuthoringSpec>, CliError> {
    let mut specs = Vec::new();
    for topic in curriculum.topics() {
        for kp in &topic.knowledge_points {
            specs.push(spec_of(topic, kp)?);
        }
    }
    Ok(specs)
}

/// The spec of one serving key, or the refusal an unknown key earns.
fn one_spec(curriculum: &Curriculum, key: &str) -> Result<AuthoringSpec, CliError> {
    let Some((topic_id, kp_id)) = split_kp_key(key) else {
        return Err(CliError(format!(
            "the knowledge point `{key}` is not a serving key — write it as `<topic_id>/<kp_id>`"
        )));
    };
    let topic = curriculum
        .topics()
        .iter()
        .find(|topic| topic.id.as_str() == topic_id)
        .ok_or_else(|| CliError(format!("the curriculum holds no knowledge point `{key}`")))?;
    let kp = topic
        .knowledge_points
        .iter()
        .find(|kp| kp.id.as_str() == kp_id)
        .ok_or_else(|| CliError(format!("the curriculum holds no knowledge point `{key}`")))?;
    spec_of(topic, kp)
}

/// One curriculum knowledge point, as the spec the prompt reads.
fn spec_of(topic: &Topic, kp: &KnowledgePoint) -> Result<AuthoringSpec, CliError> {
    let kp_key = format!("{}/{}", topic.id.as_str(), kp.id.as_str());
    let finite = kp
        .finite_objective_domain
        .as_ref()
        .map(|domain| FiniteAuthoringPolicy::new(&kp_key, domain))
        .transpose()
        .map_err(CliError)?;
    Ok(AuthoringSpec {
        kp_id: kp.id.as_str().to_owned(),
        kp_name: kp.name.clone(),
        topic_id: topic.id.as_str().to_owned(),
        topic_name: topic.name.clone(),
        answer_kind: topic.answer_kind,
        difficulty_target: None,
        constraints: kp.constraints.clone(),
        exemplars: kp.exemplars.clone(),
        finite,
    })
}
