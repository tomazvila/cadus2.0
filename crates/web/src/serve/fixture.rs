//! The arena and the tasks of the unit tests of this module.

use cadus_core::curriculum::{Catalog, RawCurriculum, RawUnit, Unit};

use super::*;

/// One topic as its unit file spells it. `points` pairs a knowledge point id
/// with the answers of its exemplars.
pub(super) fn topic_doc(id: &str, points: &[(&str, &[&str])]) -> Value {
    let kps: Vec<Value> = points
        .iter()
        .map(|(kp, answers)| {
            let exemplars: Vec<Value> = answers
                .iter()
                .map(|answer| json!({"problem": format!("Give {answer}."), "answer": answer}))
                .collect();
            json!({"id": kp, "name": kp, "exemplars": exemplars})
        })
        .collect();
    json!({
        "id": id,
        "name": format!("The {id} topic"),
        "difficulty": 0.3,
        "answer_kind": "numeric",
        "expected_time_secs": 30,
        "knowledge_points": kps,
    })
}

/// The arena of `topics`, in one unit of course `c1`.
pub(super) fn arena(topics: &[Value]) -> Curriculum {
    let catalog: Catalog =
        serde_json::from_value(json!({"courses": [{"id": "c1", "name": "c1", "order": 0}]}))
            .unwrap();
    let unit: Unit = serde_json::from_value(json!({
        "unit": "M1",
        "course": "c1",
        "module": "M1",
        "topics": topics,
    }))
    .unwrap();
    Curriculum::build(RawCurriculum {
        catalog,
        units: vec![RawUnit {
            course_id: "c1".to_string(),
            file_name: "00-M1.yaml".to_string(),
            unit,
            first_load_index: 0,
        }],
    })
    .unwrap()
}

/// The arena of the unit tests: `addition` with `kp1` and `kp2`, `counting`
/// with `kp1`, and `empty` with no knowledge point.
pub(super) fn graph() -> Curriculum {
    arena(&[
        topic_doc("addition", &[("kp1", &["13.5"]), ("kp2", &["42.5"])]),
        topic_doc("counting", &[("kp1", &["7"])]),
        topic_doc("empty", &[]),
    ])
}

/// A task of `task_type` on `topic`, with the id `assign_ids` gives it.
pub(super) fn task(task_type: TaskType, topic: Option<&str>) -> Task {
    let mut task_id = format!("s1-{}", task_type.as_str());
    if let Some(id) = topic {
        task_id = format!("{task_id}-{id}");
    }
    Task {
        task_id,
        task_type,
        topic: topic.map(str::to_string),
        ..Task::default()
    }
}
