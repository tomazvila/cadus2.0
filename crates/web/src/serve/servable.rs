//! The health check of the serving path: every knowledge point of the course
//! graph that can serve no problem at all.
//!
//! A point can serve through its authored exemplars (the A6 rows that
//! [`exemplar_rows`] builds, decidable or self-check) and through an approved
//! template. The template side lives in the database, so the caller passes the
//! serving keys that hold one. The selection never needs this list to stay
//! alive, because [`super::fallback`] moves a session to a sibling point; the
//! list tells the content owners which points need authored exemplars.

use super::*;

/// One knowledge point that serves nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyKp {
    /// The course of the topic.
    pub course: String,
    /// The topic id.
    pub topic: String,
    /// The knowledge point id.
    pub kp: String,
    /// Exemplars the author wrote.
    pub authored: usize,
    /// Pool rows the authored exemplars build.
    pub exemplar_rows: usize,
    /// Whether the point has a finite objective policy.
    pub finite: bool,
}

/// The servable count of one point: exemplar rows plus approved templates.
fn servable(graph: &Curriculum, topic: &str, kp: &str, templates: &BTreeSet<String>) -> usize {
    let target = Target::new(topic.to_owned(), topic.to_owned(), kp.to_owned());
    let rows = exemplar_rows(graph, &target).len();
    rows.saturating_add(usize::from(templates.contains(&target.key)))
}

/// Every point reachable from a course whose servable count is zero.
///
/// `templates` holds the serving keys (`<topic>/<kp>`) with an approved
/// template. The order is course order, then load order of the topic.
#[must_use]
pub fn empty_kps(graph: &Curriculum, templates: &BTreeSet<String>) -> Vec<EmptyKp> {
    let mut found = Vec::new();
    for course in graph.courses() {
        for &idx in graph.topics_in_course(course.id.as_str()) {
            let topic = graph.id_of(idx);
            for kp in graph.knowledge_points(idx) {
                if servable(graph, topic, kp.id.as_str(), templates) > 0 {
                    continue;
                }
                let target = Target::new(topic.to_owned(), topic.to_owned(), kp.id.to_string());
                found.push(EmptyKp {
                    course: course.id.to_string(),
                    topic: topic.to_owned(),
                    kp: kp.id.to_string(),
                    authored: kp.exemplars.len(),
                    exemplar_rows: exemplar_rows(graph, &target).len(),
                    finite: kp.finite_objective_domain.is_some(),
                });
            }
        }
    }
    found
}

/// The tab-separated report of [`empty_kps`], header first.
#[must_use]
pub fn report_tsv(empty: &[EmptyKp]) -> String {
    let mut out = String::from("course\ttopic\tkp\tauthored_exemplars\texemplar_rows\tfinite\n");
    for row in empty {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            row.course, row.topic, row.kp, row.authored, row.exemplar_rows, row.finite
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::fixture::{arena, topic_doc};
    use super::*;

    /// A point with no exemplar and no approved template is flagged; a point
    /// with either one is not.
    #[test]
    fn the_health_check_flags_a_point_that_serves_nothing() {
        let graph = arena(&[topic_doc("counting", &[("kp1", &["7"]), ("kp2", &[])])]);
        let none = BTreeSet::new();
        let flagged = empty_kps(&graph, &none);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].kp, "kp2");
        assert_eq!(flagged[0].course, "c1");
        assert_eq!(flagged[0].authored, 0);
        let with_template = BTreeSet::from(["counting/kp2".to_owned()]);
        assert!(empty_kps(&graph, &with_template).is_empty());
        assert!(report_tsv(&flagged).contains("c1\tcounting\tkp2\t0\t0\tfalse\n"));
    }

    /// The whole curriculum tree: write the report where the caller asks.
    /// `CADUS_EMPTY_KPS_OUT` names the file; `CADUS_TEMPLATE_KEYS` names a file
    /// with one serving key per line for the approved templates.
    #[test]
    fn the_report_over_the_whole_tree_lists_every_empty_point() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
        let (graph, _findings) =
            cadus_core::curriculum::load_curriculum(&root).expect("the tree loads");
        let templates: BTreeSet<String> = std::env::var("CADUS_TEMPLATE_KEYS")
            .ok()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default();
        let report = report_tsv(&empty_kps(&graph, &templates));
        if let Ok(out) = std::env::var("CADUS_EMPTY_KPS_OUT") {
            std::fs::write(out, &report).expect("the report is written");
        }
        assert!(report.starts_with("course\ttopic\tkp\t"));
    }
}
