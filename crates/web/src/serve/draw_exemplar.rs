//! The exemplar rows of the A6 fallback, and the policy a served exemplar
//! captures. [`Exemplar::verdict_policy`] decides between a graded row and a
//! self-check row.

use super::*;

use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::{AnswerKind, Exemplar};
use cadus_core::pool::{POOL_ROW_VERSION, PoolAnswer, PoolProblem};

/// Instantiate the authored exemplars of the target knowledge point (A6).
///
/// The whole list goes in at once and the D5 ring picks, instead of 1.0's bare
/// `pool[index % len]` (`serving-1.0-spec.md` section 6.1). An exemplar whose
/// answer the checker cannot decide is skipped by
/// [`ExemplarSource::fill`], so one broken exemplar never takes the knowledge
/// point off the air. A knowledge point the arena does not hold, or one whose
/// whole list is undecidable, gives no row.
pub(in crate::serve) fn exemplar_rows(graph: &Curriculum, target: &Target) -> Vec<NewInstance> {
    let Some((kind, kp)) = authored(graph, target) else {
        return Vec::new();
    };
    // The source and the self-check rows read the same verdict rule, so each
    // exemplar becomes a graded row or a self-check row, and not the two.
    let source = ExemplarSource::new(&target.key, &kp.exemplars).with_topic_kind(kind);
    // The seed changes nothing for an exemplar list: the rotation is author
    // order and no draw runs.
    let mut rows: Vec<NewInstance> = match source.fill(&target.key, source.len(), 0) {
        Ok(batch) => batch
            .instances()
            .iter()
            .map(|instance| NewInstance::from_instance(instance, Source::Exemplar, None, 0))
            .collect(),
        Err(reason) => {
            tracing::warn!(
                kp_id = %target.key,
                error = %reason,
                "serve: the A6 exemplar fallback built no decidable instance"
            );
            Vec::new()
        }
    };
    // The self-check rows join the batch ONLY for an ALL-`none` knowledge point
    // (note 101 b): the learner views the worked solution and the completion
    // counts. A knowledge point WITH verdict-capable exemplars keeps its graded
    // drills — a teach-only exemplar of a mixed list serves nothing (the standing
    // graded path covers the point; a self-check row beside a drill would never
    // be a verdict item). A zero-exemplar knowledge point completes with its
    // teach page (no draw), which the readiness gate carries.
    let all_none = !kp.exemplars.is_empty() && rows.is_empty();
    if all_none {
        rows.extend(kp.exemplars.iter().filter_map(|exemplar| {
            self_check_contract(exemplar, kind).map(|contract| self_check_row(exemplar, contract))
        }));
    }
    rows
}

/// The teach-only contract of one exemplar that gives no verdict.
///
/// [`Exemplar::verdict_policy`] is the one rule. An explicit `kind: none`
/// contract, a key that its contract refuses, a missing contract on a
/// `multi-step` or `proof` topic, and a missing contract with an answer
/// outside the decidable grammar (V2) give no verdict. Such an item is a
/// self-check and not a graded drill.
fn self_check_contract(exemplar: &Exemplar, kind: AnswerKind) -> Option<AnswerContract> {
    exemplar
        .verdict_policy(kind)
        .is_err()
        .then_some(AnswerContract::None)
}

/// The answer kind of the target topic and the authored knowledge point.
fn authored<'graph>(
    graph: &'graph Curriculum,
    target: &Target,
) -> Option<(AnswerKind, &'graph KnowledgePoint)> {
    let kind = graph
        .idx_of(&target.serve)
        .and_then(|idx| graph.topic(idx))
        .map(|topic| topic.answer_kind);
    kind.zip(authored_kp(graph, &target.serve, &target.kp))
}

/// The self-check pool row of one teach-only exemplar.
///
/// The row records the `none` contract, so the grade path refuses every
/// verdict for it before it reads the answer pair (`check_contract` →
/// Undecidable, D-F1), and no deterministic wrong can ever fire.
fn self_check_row(exemplar: &Exemplar, contract: AnswerContract) -> NewInstance {
    NewInstance {
        source: Source::Exemplar,
        content_digest: None,
        generation_context: None,
        problem: PoolProblem {
            v: POOL_ROW_VERSION,
            text: exemplar.problem.clone(),
            bindings: BTreeMap::new(),
            seed: 0,
        },
        expected_answer: PoolAnswer {
            v: POOL_ROW_VERSION,
            answer_contract: Some(contract),
            answer: exemplar.answer.clone(),
        },
        instance_hash: problem_text_hash(&exemplar.problem),
    }
}

/// Capture the current authored policy for a newly served exemplar. Matching
/// both statement and answer keeps changed content from borrowing a policy.
/// Template rows retain their approved document's policy. Existing served
/// problems bypass this function and keep their original captured policy.
pub(in crate::serve) fn answer_of(
    graph: &Curriculum,
    target: &Target,
    row: &PoolRow,
) -> cadus_core::pool::PoolAnswer {
    let mut answer = row.expected_answer.clone();
    if row.source == Source::Exemplar
        && let Some((kind, exemplar)) = authored(graph, target).and_then(|(kind, kp)| {
            kp.exemplars
                .iter()
                .find(|item| item.problem == row.problem.text && item.answer == answer.answer)
                .map(|item| (kind, item))
        })
    {
        // An exemplar that gives no verdict carries the teach-only policy, so
        // the grade path never decides a verdict for the item. If not, the
        // authored contract (or its absence) stays.
        answer.answer_contract =
            self_check_contract(exemplar, kind).or_else(|| exemplar.answer_contract.clone());
    }
    answer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::fixture::{arena, graph, topic_doc};

    /// The target that draws `kp` of `topic`, and records against it.
    fn target(topic: &str, kp: &str) -> Target {
        Target::new(topic.to_string(), topic.to_string(), kp.to_string())
    }

    /// The pool row of one new instance.
    fn pool_row(instance: NewInstance) -> PoolRow {
        PoolRow {
            id: Uuid::nil(),
            source: Source::Exemplar,
            content_digest: None,
            generation_context: None,
            problem: instance.problem,
            expected_answer: instance.expected_answer,
            instance_hash: instance.instance_hash,
        }
    }

    /// The rows of the verdict table: a valid contract, a `none` contract, no
    /// contract, and a contract with a key that does not validate.
    fn table() -> Vec<(Value, &'static str)> {
        vec![
            (json!({"kind": "exact"}), "7"),
            (json!({"kind": "none"}), "7"),
            (Value::Null, "7"),
            (json!({"kind": "exact"}), "many words"),
            (Value::Null, "many words"),
        ]
    }

    /// Each exemplar is a graded row if `verdict_policy` gives a verdict, and
    /// a self-check row if not, for each contract state and each topic kind.
    #[test]
    fn the_pool_rows_agree_with_the_verdict_policy() {
        for kind in ["numeric", "expression", "multi-step", "proof"] {
            for (contract, answer) in table() {
                let mut topic = topic_doc("counting", &[("kp1", &[answer])]);
                topic["answer_kind"] = json!(kind);
                if !contract.is_null() {
                    topic["knowledge_points"][0]["exemplars"][0]["answer_contract"] = contract;
                }
                let graph = arena(&[topic]);
                let target = target("counting", "kp1");
                let (topic_kind, kp) = authored(&graph, &target).unwrap();
                let verdict = kp.exemplars[0].verdict_policy(topic_kind).is_ok();
                let mut rows = exemplar_rows(&graph, &target);
                assert_eq!(rows.len(), 1, "{kind} {answer}");
                let row = pool_row(rows.remove(0));
                let graded = row.expected_answer.answer_contract != Some(AnswerContract::None);
                assert_eq!(graded, verdict, "{kind} {answer}");
                let captured = answer_of(&graph, &target, &row).answer_contract;
                assert_eq!(
                    captured != Some(AnswerContract::None),
                    verdict,
                    "{kind} {answer}"
                );
            }
        }
    }

    #[test]
    fn the_exemplar_rows_serve_an_undecidable_list_as_self_check_rows() {
        let undecidable = arena(&[topic_doc("words", &[("kp1", &["many", "few"])])]);
        let rows = exemplar_rows(&undecidable, &target("words", "kp1"));
        // The tier-2 change: an exemplar list the checker cannot decide is no
        // longer a dark knowledge point. Every exemplar becomes a self-check
        // row that records the `none` contract, so the grade path refuses
        // every verdict for it and no deterministic wrong can fire.
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.source == Source::Exemplar));
        assert!(
            rows.iter()
                .all(|row| row.expected_answer.answer_contract == Some(AnswerContract::None))
        );
        assert!(rows.iter().all(|row| row.content_digest.is_none()));
        assert!(exemplar_rows(&graph(), &target("addition", "kp9")).is_empty());
        assert!(exemplar_rows(&graph(), &target("empty", "kp1")).is_empty());
        // A topic the arena does not hold gives no authored knowledge point.
        assert!(exemplar_rows(&graph(), &target("nowhere", "kp1")).is_empty());
        assert_eq!(exemplar_rows(&graph(), &target("addition", "kp1")).len(), 1);
    }

    /// An exemplar with an explicit contract keeps it; a missing contract on a
    /// decidable answer keeps the legacy grading; only the undecidable answers
    /// and the explicit `none` take the self-check policy.
    #[test]
    fn the_self_check_policy_follows_the_authored_contract() {
        let mut declared = Exemplar {
            answer_contract: Some(AnswerContract::Exact),
            problem: "Give 7.".to_owned(),
            answer: "7".to_owned(),
            solution_sketch: Some("Count to 7.".to_owned()),
        };
        declared.answer_contract = Some(AnswerContract::None);
        assert_eq!(
            self_check_contract(&declared, AnswerKind::Numeric),
            Some(AnswerContract::None)
        );
        declared.answer_contract = Some(AnswerContract::Exact);
        assert_eq!(self_check_contract(&declared, AnswerKind::Numeric), None);
        declared.answer_contract = None;
        assert_eq!(self_check_contract(&declared, AnswerKind::Numeric), None);
        declared.answer = "many".to_owned();
        assert_eq!(
            self_check_contract(&declared, AnswerKind::Numeric),
            Some(AnswerContract::None)
        );
    }

    #[test]
    fn a_new_exemplar_serve_captures_the_matching_current_policy() {
        let mut topic = topic_doc("counting", &[("kp1", &["7"])]);
        topic["knowledge_points"][0]["exemplars"][0]["answer_contract"] = json!({"kind": "exact"});
        let graph = arena(&[topic]);
        let target = target("counting", "kp1");
        let instance = exemplar_rows(&graph, &target).remove(0);
        let mut row = PoolRow {
            id: Uuid::nil(),
            source: Source::Exemplar,
            content_digest: None,
            generation_context: None,
            problem: instance.problem,
            expected_answer: instance.expected_answer,
            instance_hash: instance.instance_hash,
        };
        row.expected_answer.answer_contract = None;
        let captured = answer_of(&graph, &target, &row);
        assert_eq!(captured.answer_contract, Some(AnswerContract::Exact));
        assert_eq!(row.expected_answer.answer_contract, None);
        row.expected_answer.answer = "8".to_owned();
        assert_eq!(answer_of(&graph, &target, &row).answer_contract, None);
        row.expected_answer.answer = "7".to_owned();
        row.problem.text = "Different question".to_owned();
        assert_eq!(answer_of(&graph, &target, &row).answer_contract, None);
        row.problem.text = "Give 7.".to_owned();
        row.source = Source::Template;
        assert_eq!(answer_of(&graph, &target, &row).answer_contract, None);
        assert_eq!(captured.answer_contract, Some(AnswerContract::Exact));
    }

    /// Note 101 (b), branch 2: the self-check rows join the batch ONLY for an
    /// ALL-`none` knowledge point. A MIXED list (a decidable exemplar beside a
    /// teach-only one) serves its graded drills; the teach-only member serves
    /// nothing (never a self-check row beside a drill).
    #[test]
    fn a_mixed_knowledge_point_builds_no_self_check_row_and_an_all_none_one_does() {
        // The mixed topic: kp1 carries one decidable and one `none` exemplar.
        let mixed = serde_json::json!({
            "id": "mixed101",
            "name": "mixed101",
            "difficulty": 0.3,
            "answer_kind": "numeric",
            "expected_time_secs": 30,
            "knowledge_points": [{"id": "kp1", "name": "kp1", "exemplars": [
                {"problem": "Give 7.", "answer": "7"},
                {"problem": "Why does the sign flip?", "answer": "the worked solution",
                 "answer_contract": {"kind": "none"}}
            ]}]
        });
        let graph = arena(&[mixed]);
        let rows = exemplar_rows(&graph, &target("mixed101", "kp1"));
        assert!(!rows.is_empty(), "the decidable exemplar authors a graded row");
        assert!(
            rows.iter().all(|row| row
                .expected_answer
                .answer_contract
                .as_ref()
                .is_none_or(|contract| !matches!(contract, AnswerContract::None))),
            "a mixed list serves no self-check row: {rows:?}"
        );

        // The all-none topic: every exemplar is teach-only, and each becomes a
        // self-check row (the tier-2 completion).
        let all_none = serde_json::json!({
            "id": "prose101",
            "name": "prose101",
            "difficulty": 0.3,
            "answer_kind": "numeric",
            "expected_time_secs": 30,
            "knowledge_points": [{"id": "kp1", "name": "kp1", "exemplars": [
                {"problem": "Why the sign flips?", "answer": "the worked solution",
                 "answer_contract": {"kind": "none"}},
                {"problem": "Why the terms cancel?", "answer": "the worked solution",
                 "answer_contract": {"kind": "none"}}
            ]}]
        });
        let graph = arena(&[all_none]);
        let rows = exemplar_rows(&graph, &target("prose101", "kp1"));
        assert_eq!(rows.len(), 2, "each all-none exemplar is a self-check row");
        assert!(rows.iter().all(|row| row
            .expected_answer
            .answer_contract
            .as_ref()
            .is_some_and(|contract| matches!(contract, AnswerContract::None))));
    }
}
