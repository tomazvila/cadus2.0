#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Writes `docs/content-visuals/foundations-reference-manifest.json`: every
//! foundations knowledge point that holds figures, with the figures in their
//! serialized form, so the manifest test compares the tree with itself.
use cadus_core::curriculum::load_curriculum;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).expect("curriculum");
    assert!(findings.is_empty(), "{findings:?}");
    let mut entries = Vec::new();
    for topic in curriculum.topics() {
        if curriculum.course_of(curriculum.idx_of(topic.id.as_str()).unwrap()) != "foundations" {
            continue;
        }
        for kp in &topic.knowledge_points {
            if kp.visuals.is_empty() {
                continue;
            }
            entries.push(serde_json::json!({
                "kp_id": format!("{}/{}", topic.id, kp.id),
                "visuals": kp.visuals,
            }));
        }
    }
    entries.sort_by(|a, b| a["kp_id"].as_str().cmp(&b["kp_id"].as_str()));
    let path = root.join("docs/content-visuals/foundations-reference-manifest.json");
    let mut text = serde_json::to_string_pretty(&entries).expect("json");
    text.push('\n');
    std::fs::write(&path, text).expect("write manifest");
    let figures: usize = entries
        .iter()
        .map(|e| e["visuals"].as_array().unwrap().len())
        .sum();
    println!(
        "{} entries, {} figures -> {}",
        entries.len(),
        figures,
        path.display()
    );
}
