//! Print the authored Foundations visual inventory without a database or model.
use cadus_core::{
    curriculum::load_curriculum,
    readiness::{EmptyContent, ReadinessIndex},
};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "curriculum".to_owned());
    let (curriculum, _) = load_curriculum(std::path::Path::new(&root))?;
    let readiness = ReadinessIndex::build(&curriculum).resolve(&EmptyContent);
    let mut rows = Vec::new();
    for topic in curriculum.topics() {
        let Some(index) = curriculum.idx_of(topic.id.as_str()) else {
            continue;
        };
        if curriculum.course_of(index) != "foundations" {
            continue;
        }
        for kp in &topic.knowledge_points {
            let key = format!("{}/{}", topic.id, kp.id);
            let Some(state) = readiness.get(&key) else {
                continue;
            };
            if state.visual_needed {
                rows.push(json!({"kp_id":key,"topic":topic.name,"name":kp.name,"visual_present":state.visual_present,"broken_visuals":state.broken_visuals,"visuals":kp.visuals,"rendered":cadus_core::visual::render_all(&kp.visuals,&key),"exemplars":kp.exemplars}));
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}
