//! The pool draw with the A6 exemplar fallback, and the authored solution
//! sketch of the drawn row (A4, D-M5-3).

use super::*;

/// Take one instance out of the pool, and fall back to the exemplars (A6).
///
/// The order is: pop; on a miss instantiate the authored exemplars in process
/// and write them into the pool; pop again; and if the whole authored list is
/// already claimed, rotate through it. NOTHING here calls a model (A6, T1).
pub(super) async fn draw(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    graph: &Curriculum,
    target: &Target,
    avoid: &Avoid<'_>,
) -> Result<PoolRow, ApiError> {
    let popped = store(state, pop_with_ring_tx(tx, user_id, &target.key, avoid)).await?;
    if let Some(claimed) = popped.claimed {
        return Ok(claimed.row);
    }
    if let Some(row) = refill(state, tx, user_id, graph, target, avoid).await? {
        return Ok(row);
    }
    let rotated = store(state, reclaim_exemplar_tx(tx, user_id, &target.key, avoid)).await?;
    match rotated {
        Some(row) => {
            tracing::warn!(
                user_id = %user_id,
                kp_id = %target.key,
                "serve: the A6 exemplar rotation served an instance again; this knowledge point \
                 needs an approved template"
            );
            Ok(row)
        }
        None => Err(no_problem(&target.serve)),
    }
}

/// Write the authored exemplars of the target into the pool and pop again
/// (A6). `None` when the knowledge point authors nothing usable, or when the
/// whole authored list is already claimed.
async fn refill(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    graph: &Curriculum,
    target: &Target,
    avoid: &Avoid<'_>,
) -> Result<Option<PoolRow>, ApiError> {
    let rows = exemplar_rows(graph, target);
    if rows.is_empty() {
        return Ok(None);
    }
    store(state, insert_batch(&mut **tx, user_id, &target.key, &rows)).await?;
    let refilled = store(state, pop_with_ring_tx(tx, user_id, &target.key, avoid)).await?;
    let Some(claimed) = refilled.claimed else {
        return Ok(None);
    };
    let inserted = rows.len();
    tracing::warn!(
        user_id = %user_id,
        kp_id = %target.key,
        rows = inserted,
        "serve: the pool was empty, so the A6 exemplar fallback filled it in process"
    );
    Ok(Some(claimed.row))
}

/// Instantiate the authored exemplars of the target knowledge point (A6).
///
/// The whole list goes in at once and the D5 ring picks, instead of 1.0's bare
/// `pool[index % len]` (`serving-1.0-spec.md` section 6.1). An exemplar whose
/// answer the checker cannot decide is skipped by
/// [`ExemplarSource::fill`], so one broken exemplar never takes the knowledge
/// point off the air. A knowledge point the arena does not hold, or one whose
/// whole list is undecidable, gives no row.
pub(super) fn exemplar_rows(graph: &Curriculum, target: &Target) -> Vec<NewInstance> {
    let Some(kp) = authored_kp(graph, &target.serve, &target.kp) else {
        return Vec::new();
    };
    let source = ExemplarSource::new(&target.key, &kp.exemplars);
    // The seed changes nothing for an exemplar list: the rotation is author
    // order and no draw runs.
    match source.fill(&target.key, source.len(), 0) {
        Ok(batch) => batch
            .instances()
            .iter()
            .map(|instance| NewInstance::from_instance(instance, Source::Exemplar, None, 0))
            .collect(),
        Err(reason) => {
            tracing::warn!(
                kp_id = %target.key,
                error = %reason,
                "serve: the A6 exemplar fallback built no instance"
            );
            Vec::new()
        }
    }
}

/// Capture the current authored policy for a newly served exemplar. Matching
/// both statement and answer keeps changed content from borrowing a policy.
/// Template rows retain their approved document's policy. Existing served
/// problems bypass this function and keep their original captured policy.
pub(super) fn answer_of(
    graph: &Curriculum,
    target: &Target,
    row: &PoolRow,
) -> cadus_core::pool::PoolAnswer {
    let mut answer = row.expected_answer.clone();
    if row.source == Source::Exemplar
        && let Some(exemplar) = authored_kp(graph, &target.serve, &target.kp).and_then(|kp| {
            kp.exemplars
                .iter()
                .find(|item| item.problem == row.problem.text && item.answer == answer.answer)
        })
    {
        answer.answer_contract = exemplar.answer_contract;
    }
    answer
}

/// The authored worked solution of the drawn row, or `None`.
///
/// The grade reply of unit U8 reveals it after the attempt commits, and the
/// stock re-solve text of D-M5-3 tells the learner to study it, so a served
/// problem that carries no sketch makes that text point at nothing (M5 review 1,
/// findings F2 and F11).
///
/// The pool row names its own author. An `exemplar` row (A6) names the authored
/// exemplar of the knowledge point by its statement, which
/// [`ExemplarSource::fill`] copies verbatim. A `template` row names the
/// `content_store` document by `content_digest`, and that document's sketch is a
/// statement over the same parameters, so the row's own bindings render it.
///
/// The read costs one indexed `content_store` statement inside the transaction
/// the caller owns, and only for a template row. A `generator` row (A7) names no
/// author, so it takes the `None` branch.
///
/// `None` is the closed answer of every refusal: a row whose digest lost its
/// approval, a document that does not read, an author who wrote no sketch, and a
/// sketch that does not render. The reply then omits `solution`, and the learner
/// keeps the verdict and the expected answer.
pub(super) async fn solution_of(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    graph: &Curriculum,
    target: &Target,
    row: &PoolRow,
) -> Result<Option<String>, ApiError> {
    if row.source == Source::Exemplar {
        return Ok(exemplar_sketch(graph, target, &row.problem.text));
    }
    let Some(digest) = row.content_digest.as_deref() else {
        return Ok(None);
    };
    let found = store(state, approved_template(&mut **tx, &target.key)).await?;
    // C6: the row was drawn from ONE digest, and the approved document may be a
    // later one. The sketch of a different document is not this problem's
    // solution.
    Ok(found
        .filter(|approved| approved.digest == digest)
        .and_then(|approved| template_sketch(&approved.body, &row.problem.bindings, &target.key)))
}

/// The authored exemplar whose statement is `text`, and its sketch (A6).
fn exemplar_sketch(graph: &Curriculum, target: &Target, text: &str) -> Option<String> {
    let kp = authored_kp(graph, &target.serve, &target.kp)?;
    kp.exemplars
        .iter()
        .find(|exemplar| exemplar.problem == text)
        .and_then(|exemplar| exemplar.solution_sketch.clone())
}

/// Render the sketch of one template document against the bindings of one row.
fn template_sketch(body: &str, bindings: &BTreeMap<String, String>, key: &str) -> Option<String> {
    let doc = match from_body(body) {
        Ok(doc) => doc,
        Err(reason) => {
            tracing::warn!(
                kp_id = %key,
                error = %reason,
                "serve: the approved template did not read, so the serve carries no solution"
            );
            return None;
        }
    };
    let sketch = doc.solution_sketch?;
    let values: Bindings = bindings
        .iter()
        .map(|(name, text)| (name.clone(), binding_value(text)))
        .collect();
    match render(&sketch, &values) {
        Ok(text) => Some(text),
        Err(reason) => {
            tracing::warn!(
                kp_id = %key,
                error = %reason,
                "serve: the authored solution sketch did not render"
            );
            None
        }
    }
}

/// Read one canonical binding string back into the value the draw bound.
///
/// The pool row keeps every bound value as its canonical string (D6), and the
/// renderer takes values. The reader is the inverse of
/// [`cadus_core::template::Value::canonical_string`]: a spelling that names an
/// exact rational and carries a decimal point is the authored spelling of a
/// decimal domain; every other spelling that names a rational is a number; and a
/// spelling that names no rational is a text choice, such as a multiplication
/// sign. The three cases write the same string back and take the same brackets
/// as the draw took.
fn binding_value(canonical: &str) -> Binding {
    let Some(number) = literal_to_rational(canonical) else {
        return Binding::Text(canonical.to_string());
    };
    if canonical.contains('.') {
        return Binding::Spelled {
            text: canonical.to_string(),
            number,
        };
    }
    Binding::Num(number)
}

/// The authored knowledge point of one topic, or `None`.
fn authored_kp<'graph>(
    graph: &'graph Curriculum,
    topic_id: &str,
    kp_id: &str,
) -> Option<&'graph KnowledgePoint> {
    let idx = graph.idx_of(topic_id)?;
    let kp_idx = graph.kp_idx_of(idx, kp_id)?;
    graph.knowledge_point(idx, kp_idx)
}

#[cfg(test)]
mod tests {
    use super::fixture::{arena, graph, topic_doc};
    use super::*;

    /// The target that draws `kp` of `topic`, and records against it.
    fn target(topic: &str, kp: &str) -> Target {
        Target::new(topic.to_string(), topic.to_string(), kp.to_string())
    }

    /// The bindings `a = 8`, `b = -3`, `c = 2.5`, and `op = x`.
    fn bindings() -> BTreeMap<String, String> {
        [("a", "8"), ("b", "-3"), ("c", "2.5"), ("op", "x")]
            .into_iter()
            .map(|(name, text)| (name.to_string(), text.to_string()))
            .collect()
    }

    /// A `counting/kp1` template whose sketch is `sketch`.
    fn body(sketch: &str) -> String {
        let mut doc = json!({"v": 1, "topic_id": "counting", "answer_kind": "numeric"});
        doc["statement"] = json!("Step {b} from {a}.");
        doc["params"] = json!({"b": {"kind": "int", "low": -9, "high": -1}});
        doc["params"]["a"] = json!({"kind": "int", "low": 1, "high": 12});
        doc["answer_expr"] = json!("a + b");
        doc["solution_sketch"] = json!(sketch);
        doc.to_string()
    }

    /// The variant tag of one binding. Every arm is a case of the test below.
    fn tag(binding: &Binding) -> &'static str {
        match binding {
            Binding::Num(_) => "num",
            Binding::Spelled { .. } => "spelled",
            Binding::Text(_) => "text",
        }
    }

    #[test]
    fn a_canonical_binding_reads_back_as_the_value_the_draw_bound() {
        assert_eq!(tag(&binding_value("8")), "num");
        assert_eq!(tag(&binding_value("2.5")), "spelled");
        assert_eq!(tag(&binding_value("x")), "text");
        assert_eq!(binding_value("x"), Binding::Text("x".to_string()));
    }

    #[test]
    fn the_sketch_renders_every_binding_kind_with_the_brackets_of_the_draw() {
        let rendered = template_sketch(
            &body("Start at {a}, step {b}, scale by {c} {op}."),
            &bindings(),
            "counting/kp1",
        );
        assert_eq!(
            rendered.as_deref(),
            Some("Start at 8, step (-3), scale by 2.5 x.")
        );
    }

    #[test]
    fn a_sketch_that_does_not_read_or_render_gives_no_solution() {
        assert_eq!(template_sketch("{", &bindings(), "counting/kp1"), None);
        assert_eq!(
            template_sketch(&body("Step {nowhere}."), &bindings(), "counting/kp1"),
            None
        );
        let mut without = serde_json::from_str::<Value>(&body("x")).unwrap();
        without.as_object_mut().unwrap().remove("solution_sketch");
        assert_eq!(
            template_sketch(&without.to_string(), &bindings(), "counting/kp1"),
            None
        );
    }

    #[test]
    fn the_exemplar_rows_skip_an_undecidable_list_and_an_unknown_point() {
        let undecidable = arena(&[topic_doc("words", &[("kp1", &["many", "few"])])]);
        assert!(exemplar_rows(&undecidable, &target("words", "kp1")).is_empty());
        assert!(exemplar_rows(&graph(), &target("addition", "kp9")).is_empty());
        assert!(exemplar_rows(&graph(), &target("empty", "kp1")).is_empty());
        // A topic the arena does not hold gives no authored knowledge point.
        assert!(exemplar_rows(&graph(), &target("nowhere", "kp1")).is_empty());
        assert_eq!(exemplar_rows(&graph(), &target("addition", "kp1")).len(), 1);
    }

    #[test]
    fn the_exemplar_sketch_is_the_one_the_author_of_the_statement_wrote() {
        let exemplar = arena(&[topic_doc("counting", &[("kp1", &["7"])])]);
        // The fixture authors no sketch, so a matching statement gives none.
        assert_eq!(
            exemplar_sketch(&exemplar, &target("counting", "kp1"), "Give 7."),
            None
        );
        assert_eq!(
            exemplar_sketch(&exemplar, &target("counting", "kp9"), "Give 7."),
            None
        );
    }
    #[test]
    fn a_new_exemplar_serve_captures_the_matching_current_policy() {
        use cadus_core::answer::AnswerContract;
        let mut topic = topic_doc("counting", &[("kp1", &["7"])]);
        topic["knowledge_points"][0]["exemplars"][0]["answer_contract"] = json!({"kind": "exact"});
        let graph = arena(&[topic]);
        let target = target("counting", "kp1");
        let instance = exemplar_rows(&graph, &target).remove(0);
        let mut row = PoolRow {
            id: Uuid::nil(),
            source: Source::Exemplar,
            content_digest: None,
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
}
