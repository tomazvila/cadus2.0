//! The demo figures of the SPA and the first authored example of a figure with a slider
//! (D-FA1). The laws and the refusals are in `visual_motion.rs`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use cadus_core::visual::{MotionTable, RenderOptions, VisualSpec, render_all, render_with_motion};
use serde_json::{Value, json};

fn spec(value: Value) -> VisualSpec {
    serde_json::from_value(value).expect("a visual spec")
}

fn table_of(figure: &VisualSpec) -> (String, MotionTable) {
    let (svg, table) = render_with_motion(figure, &RenderOptions::default()).expect("render");
    (svg, table.expect("a motion table"))
}

/// The two demo figures of the SPA are the real output of the renderer. If the renderer
/// changes, this test names the file to refresh.
#[test]
fn the_demo_figures_of_the_spa_are_the_current_render_output() {
    let path = format!(
        "{}/../../web/src/api/demo-motion.ts",
        env!("CARGO_MANIFEST_DIR")
    );
    let source = std::fs::read_to_string(path).expect("the demo figure file");
    let shaded = spec(json!({
        "kind": "fraction", "parts": 6, "shaded": 4, "caption": "A bar of 6 equal parts",
        "motion": {"parameter": {"name": "shaded parts", "min": "0", "max": "6", "default": "4", "step": "1"},
                   "drives": [{"kind": "shaded"}]}
    }));
    let slope = spec(json!({
        "kind": "coordinate", "x_min": "0", "x_max": "8", "y_min": "0", "y_max": "8",
        "x_tick": "1", "y_tick": "1",
        "points": [{"x": "4", "y": "2", "label": "T"}],
        "segments": [{"from": {"x": "0", "y": "0"}, "to": {"x": "4", "y": "2"}}],
        "caption": "A line from the origin to the point T",
        "motion": {"parameter": {"name": "slope", "min": "0", "max": "2", "default": "1/2", "step": "1/2"},
                   "drives": [{"kind": "line_slope", "segment": 0}]}
    }));
    let drawn = render_all(&[shaded, slope], "teach");
    for (name, figure) in [
        ("DEMO_SHADED_FIGURE", &drawn[0]),
        ("DEMO_SLOPE_FIGURE", &drawn[1]),
    ] {
        let head = format!("export const {name}: RenderedVisual = ");
        let from = source.find(&head).expect("the constant") + head.len();
        let to = source[from..].find(";\n").expect("the end") + from;
        let kept: Value = serde_json::from_str(&source[from..to]).expect("JSON");
        assert_eq!(
            kept,
            serde_json::to_value(figure).unwrap(),
            "{name} is stale"
        );
    }
}

/// An example figure for a slope slider. The spec sits in this file, not in the live tree.
#[test]
fn the_first_authored_example_holds_and_renders() {
    let figure = spec(json!({
        "kind": "coordinate", "x_min": "0", "x_max": "8", "y_min": "0", "y_max": "8",
        "x_tick": "1", "y_tick": "1",
        "points": [{"x": "0", "y": "0", "label": "O"}, {"x": "4", "y": "2", "label": "T"}],
        "segments": [{"from": {"x": "0", "y": "0", "label": null}, "to": {"x": "4", "y": "2", "label": null}}],
        "caption": "A straight line from the origin O to the marked point T, with a slider for the slope",
        "motion": {"parameter": {"name": "slope", "min": "0", "max": "2", "default": "1/2", "step": "1/2"},
                   "drives": [{"kind": "line_slope", "segment": 0}]},
    }));
    let (_, table) = table_of(&figure);
    assert_eq!(table.parameter.labels, ["0", "1/2", "1", "3/2", "2"]);
    assert_eq!(table.groups.len(), 2);
    assert_eq!(table.readouts[0].texts[3], "y = 3/2 x");
}
