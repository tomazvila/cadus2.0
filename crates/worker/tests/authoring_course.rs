//! Course-scoped staged plans stay inside one shared paid-pass allocation.
#![allow(clippy::unwrap_used)]
use cadus_core::curriculum::load_curriculum;
use cadus_worker::authoring::{
    cli::{AuthorArgs, Command, PlanRow, parse, select_for, staged_documents},
    prompt::Kind,
};

#[test]
fn foundations_selection_is_exact_and_cross_course_keys_are_refused() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
    let (curriculum, _) = load_curriculum(&root).unwrap();
    let Command::Author(mut args) = parse(&[
        "author",
        "--course",
        "foundations",
        "--template-passes",
        "3",
    ])
    .unwrap() else {
        unreachable!()
    };
    let specs = select_for(&curriculum, &args).unwrap();
    assert_eq!(specs.len(), 809);
    assert_eq!(args.template_passes, 3);
    let mut units = std::collections::BTreeMap::<String, usize>::new();
    for spec in &specs {
        *units
            .entry(
                curriculum
                    .unit_of(curriculum.idx_of(&spec.topic_id).unwrap())
                    .to_owned(),
            )
            .or_default() += 1;
    }
    assert_eq!(units["arithmetic-core"], 81);
    assert_eq!(units.values().sum::<usize>(), 809);
    assert!(specs.iter().all(
        |spec| curriculum.course_of(curriculum.idx_of(&spec.topic_id).unwrap()) == "foundations"
    ));
    args.course = Some("no-such-course".to_owned());
    assert!(select_for(&curriculum, &args).is_err());
    let other = curriculum
        .topics()
        .iter()
        .find(|topic| {
            curriculum.course_of(curriculum.idx_of(topic.id.as_str()).unwrap()) != "foundations"
        })
        .unwrap();
    let args = AuthorArgs {
        course: Some("foundations".to_owned()),
        kps: vec![format!("{}/{}", other.id, other.knowledge_points[0].id)],
        ..AuthorArgs::default()
    };
    assert!(select_for(&curriculum, &args).is_err());
    assert!(parse(&["author", "--template-passes", "4"]).is_err());
}

#[test]
fn three_template_rounds_plus_instruction_fill_six_slots_per_knowledge_point() {
    let rows: Vec<_> = (0..809)
        .flat_map(|index| {
            [
                Kind::Template,
                Kind::Teach,
                Kind::HintLadder,
                Kind::Diagnosis,
            ]
            .map(move |kind| PlanRow {
                kp_id: format!("topic/kp{index}"),
                kind,
                taken: 0,
                target: if kind == Kind::Template { 3 } else { 1 },
            })
        })
        .collect();
    assert_eq!(staged_documents(&rows, 1), 3236);
    assert_eq!(staged_documents(&rows, 3), 4854);
}

mod common;

#[tokio::test]
async fn three_bank_rounds_use_the_same_budget_and_store_pending_documents() {
    cadus_store::test_support::TestDb::with(|db| async move {
        let replies = (12..=14).map(|high| {
            let mut body = common::good_arguments();
            body["params"]["a"]["high"] = serde_json::json!(high);
            body["samples"][1] = serde_json::json!({"params": {"a": high}, "expected": (high * high).to_string()});
            common::reply("emit_template", &body.to_string(), Some(serde_json::json!({"cost": 0.05})))
        }).collect();
        let fake = common::FakeModel::start(replies).await;
        let result = common::run_binary(&common::superuser_dsn(&db.name), &fake.base_url,
            &["author", "--kp", "perfect-squares/kp1", "--kind", "template", "--template-passes", "3", "--budget-usd", "1", "--request-reserve-usd", "0.5"]).await;
        assert_eq!(result.code, Some(0), "{}", result.stderr);
        assert!(result.stdout.contains("round 3/3"));
        assert!(result.stdout.contains("reserved: 150000 micro-USD"));
        assert_eq!(fake.call_count(), 3);
        let rows = common::content_rows(&db.admin, "perfect-squares/kp1").await;
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| row.status == "pending"));
    }).await;
}
