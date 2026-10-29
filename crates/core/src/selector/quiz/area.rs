//! The exam areas of a quiz round (D-VBE3).
//!
//! A course with `exam_weights` asks for a fixed share of items from each exam
//! area. The share follows the largest remainder rule.

use std::collections::BTreeMap;

use crate::curriculum::{Curriculum, ExamWeights, TOPIC_AREAS};

/// The weights of the four areas, in the order of [`TOPIC_AREAS`].
#[must_use]
pub fn weight_list(weights: &ExamWeights) -> [u8; 4] {
    [
        weights.numbers,
        weights.models,
        weights.geometry,
        weights.data,
    ]
}

/// The item count of each area for a round of `length` items.
///
/// Each area gets the whole part of `length * weight / 100`. The items that are
/// left go to the areas with the largest remainders. A tie goes to the larger
/// weight, and then to the earlier area.
#[must_use]
pub fn area_quotas(length: usize, weights: &ExamWeights) -> [usize; 4] {
    let list = weight_list(weights);
    let mut quotas = [0_usize; 4];
    let mut remainders: Vec<(usize, u8, usize)> = Vec::new();
    for (position, weight) in list.iter().enumerate() {
        let scaled = length.saturating_mul(usize::from(*weight));
        quotas[position] = scaled / 100;
        remainders.push((scaled % 100, *weight, position));
    }
    let given: usize = quotas.iter().sum();
    remainders.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(right.1.cmp(&left.1))
            .then(left.2.cmp(&right.2))
    });
    for &(_, _, position) in remainders.iter().take(length.saturating_sub(given)) {
        quotas[position] += 1;
    }
    quotas
}

/// The position in [`TOPIC_AREAS`] of the area with the largest weight.
/// A tie goes to the earlier area.
#[must_use]
pub fn largest_area(weights: &ExamWeights) -> usize {
    let list = weight_list(weights);
    let mut best = 0;
    for (position, weight) in list.iter().enumerate() {
        if *weight > list[best] {
            best = position;
        }
    }
    best
}

/// The area of each topic of a course and of its prerequisite ancestors.
///
/// A topic with an `area` keeps it. A topic with no `area` takes the area of
/// the first topic of the course, in load order, that has it as a prerequisite
/// ancestor. A topic with no area at all is not in the map.
#[must_use]
pub fn area_map(graph: &Curriculum, course_id: &str) -> BTreeMap<String, usize> {
    let mut map: BTreeMap<String, usize> = BTreeMap::new();
    let position_of = |name: &str| TOPIC_AREAS.iter().position(|area| *area == name);
    for &idx in graph.topics_in_course(course_id) {
        let Some(topic) = graph.topic(idx) else {
            continue;
        };
        if let Some(position) = topic.area.as_deref().and_then(position_of) {
            map.insert(topic.id.as_str().to_owned(), position);
        }
    }
    for &idx in graph.topics_in_course(course_id) {
        let Some(position) = graph
            .topic(idx)
            .and_then(|topic| map.get(topic.id.as_str()).copied())
        else {
            continue;
        };
        for ancestor in graph.ancestors(idx) {
            let Some(topic) = graph.topic(ancestor) else {
                continue;
            };
            if topic.area.is_none() {
                map.entry(topic.id.as_str().to_owned()).or_insert(position);
            }
        }
    }
    map
}
