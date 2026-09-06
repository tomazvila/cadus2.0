//! Portable schema transport keeps dynamic maps and the production gate intact.
#![allow(clippy::unwrap_used)]
mod common;
use cadus_store::test_support::TestDb;
use cadus_worker::authoring::{
    budget::Budget,
    cli::{Command, parse},
    job::{AuthoringJob, Outcome, author_one},
    portable::unpack,
    prompt::Kind,
};
use common::{FakeModel, good_arguments, handle, missing_low_edge, squares_spec};
use serde_json::json;

#[test]
fn portable_envelopes_are_objects_and_cli_options_are_explicit() {
    assert!(unpack(json!({})).is_err());
    assert!(unpack(json!({"document_json": "{"})).is_err());
    assert!(unpack(json!({"document_json": "[]"})).is_err());
    let Command::Author(args) =
        parse(&["author", "--portable-schema", "--decline-dir", "declines"]).unwrap()
    else {
        unreachable!()
    };
    assert!(args.portable_schema);
    assert_eq!(args.decline_dir.as_deref(), Some("declines"));
}

#[tokio::test]
async fn portable_dynamic_params_survive_transport_and_a_decline_snapshot_precedes_repair() {
    TestDb::with(|db| async move {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../target/declines-{}", db.name));
        let bad = missing_low_edge();
        let replies = [bad.clone(), good_arguments()]
            .iter()
            .map(|arguments| common::tool_reply(&json!({"document_json": arguments.to_string()})))
            .collect();
        let fake = FakeModel::start(replies).await;
        let job = AuthoringJob::new(fake.client(4000, 2000))
            .with_budget(Budget::new(5_000_000, 500_000, 16000).unwrap())
            .with_transport(true, Some(directory.to_string_lossy().into_owned()));
        let result = author_one(&handle(&db), &job, Kind::Template, &squares_spec())
            .await
            .unwrap();
        assert_eq!(result.outcome, Outcome::Stored);
        assert_eq!(result.attempts, 2);
        let calls = fake.calls();
        let schema = &calls[0]["tools"][0]["function"]["parameters"];
        assert_eq!(schema["required"], json!(["document_json"]));
        assert_eq!(schema["properties"].as_object().unwrap().len(), 1);
        assert!(fake.user_message(0).contains("never {}"));
        let files: Vec<_> = std::fs::read_dir(&directory).unwrap().collect();
        assert_eq!(files.len(), 1);
        let text = std::fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
        let snapshot: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(snapshot["arguments"], bad);
        assert_eq!(snapshot["refusal"]["code"], "edge-coverage");
        assert!(!text.contains("test-key"));
        assert!(!text.contains("api_key"));
        std::fs::remove_dir_all(directory).unwrap();
    })
    .await;
}
