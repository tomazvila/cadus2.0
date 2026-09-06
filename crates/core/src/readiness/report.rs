//! The readiness report: the counts per topic and per course, and the blocker
//! histogram the worker prints.

use std::collections::BTreeMap;

use super::facts::ReadinessIndex;
use super::resolve::ReadinessSet;
use super::{Blocker, Readiness};

/// The readiness of one topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicReport {
    /// The topic id.
    pub topic_id: String,
    /// The course of the topic's unit file.
    pub course_id: String,
    /// The knowledge points a lesson serves.
    pub ready: usize,
    /// The knowledge points a lesson does not serve.
    pub blocked: usize,
    /// How many knowledge points each blocker stops.
    pub blockers: BTreeMap<Blocker, usize>,
    /// The readiness of every knowledge point, in authored order.
    pub knowledge_points: Vec<Readiness>,
}

impl TopicReport {
    /// The blockers of the topic, in [`Blocker`] order.
    #[must_use]
    pub fn blocker_list(&self) -> Vec<Blocker> {
        self.blockers.keys().copied().collect()
    }
}

/// The readiness of one course.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseReport {
    /// The course id.
    pub course_id: String,
    /// The topics of the course.
    pub topics: usize,
    /// The knowledge points of the course.
    pub knowledge_points: usize,
    /// The knowledge points a lesson serves.
    pub ready: usize,
    /// The knowledge points a lesson does not serve.
    pub blocked: usize,
    /// How many knowledge points each blocker stops.
    pub blockers: BTreeMap<Blocker, usize>,
    /// The topics, in load order.
    pub topic_reports: Vec<TopicReport>,
}

/// The readiness of a whole tree, or of one course of it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadinessReport {
    /// The courses, in the load order of their first topic.
    pub courses: Vec<CourseReport>,
}

impl ReadinessReport {
    /// Build the report from one resolved set.
    ///
    /// `course` names one course, or `None` reports every course. The topics
    /// keep the load order of the arena, so two runs over one tree write the
    /// same report.
    #[must_use]
    pub fn build(index: &ReadinessIndex, set: &ReadinessSet, course: Option<&str>) -> Self {
        let mut courses: Vec<CourseReport> = Vec::new();
        let mut at: BTreeMap<String, usize> = BTreeMap::new();
        for topic_id in index.topics() {
            let course_id = index.course_of(topic_id).to_owned();
            if course.is_some_and(|wanted| wanted != course_id) {
                continue;
            }
            let topic = topic_report(topic_id, &course_id, set);
            let position = *at.entry(course_id.clone()).or_insert_with(|| {
                courses.push(CourseReport {
                    course_id: course_id.clone(),
                    topics: 0,
                    knowledge_points: 0,
                    ready: 0,
                    blocked: 0,
                    blockers: BTreeMap::new(),
                    topic_reports: Vec::new(),
                });
                courses.len() - 1
            });
            // The entry above put the course in the list, so the index stands.
            if let Some(report) = courses.get_mut(position) {
                add_topic(report, topic);
            }
        }
        Self { courses }
    }

    /// The course of one id.
    #[must_use]
    pub fn course(&self, course_id: &str) -> Option<&CourseReport> {
        self.courses
            .iter()
            .find(|course| course.course_id == course_id)
    }

    /// The blocker histogram of every course together.
    #[must_use]
    pub fn histogram(&self) -> BTreeMap<Blocker, usize> {
        let mut out: BTreeMap<Blocker, usize> = BTreeMap::new();
        for course in &self.courses {
            for (blocker, count) in &course.blockers {
                *out.entry(*blocker).or_insert(0) += count;
            }
        }
        out
    }

    /// The knowledge points a lesson serves, and the ones it does not.
    #[must_use]
    pub fn totals(&self) -> (usize, usize) {
        self.courses
            .iter()
            .fold((0, 0), |(ready, blocked), course| {
                (ready + course.ready, blocked + course.blocked)
            })
    }
}

/// Add one topic's counts to its course.
fn add_topic(course: &mut CourseReport, topic: TopicReport) {
    course.topics += 1;
    course.knowledge_points += topic.knowledge_points.len();
    course.ready += topic.ready;
    course.blocked += topic.blocked;
    for (blocker, count) in &topic.blockers {
        *course.blockers.entry(*blocker).or_insert(0) += count;
    }
    course.topic_reports.push(topic);
}

/// The report of one topic.
fn topic_report(topic_id: &str, course_id: &str, set: &ReadinessSet) -> TopicReport {
    let knowledge_points: Vec<Readiness> = set.topic(topic_id).into_iter().cloned().collect();
    let mut blockers: BTreeMap<Blocker, usize> = BTreeMap::new();
    let mut ready = 0;
    for readiness in &knowledge_points {
        if readiness.serves_lesson() {
            ready += 1;
        }
        for blocker in readiness.blockers() {
            *blockers.entry(blocker).or_insert(0) += 1;
        }
    }
    TopicReport {
        topic_id: topic_id.to_owned(),
        course_id: course_id.to_owned(),
        ready,
        blocked: knowledge_points.len() - ready,
        blockers,
        knowledge_points,
    }
}
