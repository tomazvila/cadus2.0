//! Write every served figure as SVG with a sidecar JSON of the item text.
//!
//! Usage: `cargo run -p cadus-core --example figure_dump [CURRICULUM_DIR] [OUT_DIR]`
use cadus_core::curriculum::load_curriculum;
use cadus_core::visual::{RenderOptions, render};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = args.next().unwrap_or_else(|| "curriculum".to_owned());
    let out = args.next().unwrap_or_else(|| "figures".to_owned());
    std::fs::create_dir_all(&out)?;
    let (curriculum, _) = load_curriculum(std::path::Path::new(&root))?;
    for topic in curriculum.topics() {
        for kp in &topic.knowledge_points {
            for (at, exemplar) in kp.exemplars.iter().enumerate() {
                let Some(index) = exemplar.visual else {
                    continue;
                };
                let name = format!("{}__{}__{at}", topic.id, kp.id);
                let spec = &kp.visuals[index];
                let svg = render(spec, &RenderOptions::with_prefix("f"));
                let (svg, error) = match svg {
                    Ok(svg) => (svg, String::new()),
                    Err(error) => (String::new(), error.to_string()),
                };
                std::fs::write(format!("{out}/{name}.svg"), svg)?;
                let side = serde_json::json!({
                    "name": name, "problem": exemplar.problem, "answer": exemplar.answer,
                    "error": error, "text": spec.text_equivalent(),
                    "spec": spec,
                });
                std::fs::write(format!("{out}/{name}.json"), side.to_string())?;
            }
        }
    }
    Ok(())
}
