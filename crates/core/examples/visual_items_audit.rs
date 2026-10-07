//! Print the items whose figure and statement disagree, as a table.
//!
//! One tab-separated row per `visual_*` finding of the full lint: file, topic,
//! knowledge point, exemplar index, visual index (or `none`), reason.
//!
//! Usage: `cargo run -p cadus-core --example visual_items_audit [CURRICULUM_DIR]`
use cadus_core::curriculum::lint_curriculum_full;

fn main() {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "curriculum".to_owned());
    println!("file\ttopic\tkp\texemplar\tvisual\treason");
    for finding in lint_curriculum_full(std::path::Path::new(&root)) {
        if !finding.code.starts_with("visual_") {
            continue;
        }
        let context = finding.context.clone().unwrap_or_default();
        let cell = |at: usize| context.get(at).map_or("", String::as_str);
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}: {}",
            finding.file.as_deref().unwrap_or(""),
            finding.topic.as_deref().unwrap_or(""),
            cell(0),
            cell(1),
            cell(2),
            finding.code,
            finding.message.replace(['\t', '\n'], " ")
        );
    }
}
