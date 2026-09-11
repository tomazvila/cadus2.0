//! The visual heuristic of D-F5: which topics need a picture.
//!
//! No curriculum field says "this topic needs a visual", so the audit reads the
//! text. The list below is a HEURISTIC and the report says so: it over-reports
//! a topic that names a shape in prose and under-reports a topic that needs a
//! picture and never says a word about one. Unit f9 replaces it with an
//! authored field.

/// The words that mark a topic as one that needs a visual.
///
/// Every word is lowercase, and the match is a substring match over the
/// lowercased text, so `graphing` matches `graph` and `number lines` matches
/// `number line`. The five families of the assignment are: a graph, a number
/// line, a diagram, a coordinate, and a shape.
pub const VISUAL_WORDS: [&str; 24] = [
    // graphs and plots
    "graph",
    "plot",
    "histogram",
    "chart",
    // number lines and intervals
    "number line",
    "numberline",
    // diagrams
    "diagram",
    "figure",
    "picture",
    "model",
    // coordinates and the plane
    "coordinate",
    "quadrant",
    "axis",
    "axes",
    "slope",
    // shapes and geometry
    "shape",
    "geometry",
    "triangle",
    "rectangle",
    "circle",
    "polygon",
    "angle",
    "area",
    "perimeter",
];

/// Whether the text of a topic names a visual (a heuristic, see [`VISUAL_WORDS`]).
///
/// The caller passes either one knowledge-point name or an unambiguous one-KP
/// topic name. One word in that text is enough.
#[must_use]
pub fn visual_needed(text: &str) -> bool {
    let lowered = text.to_lowercase();
    VISUAL_WORDS.iter().any(|word| lowered.contains(word))
}

#[cfg(test)]
mod tests {
    use super::{VISUAL_WORDS, visual_needed};

    #[test]
    fn the_heuristic_reads_the_five_families_and_ignores_plain_arithmetic() {
        assert!(visual_needed("read-a-bar-graph Read a bar graph"));
        assert!(visual_needed("Plot 3 on the NUMBER LINE"));
        assert!(visual_needed("area-of-a-triangle"));
        assert!(visual_needed("coordinate-plane"));
        assert!(!visual_needed(
            "add-two-digit-numbers Add two-digit numbers"
        ));
        assert!(!visual_needed(""));
        assert_eq!(VISUAL_WORDS.len(), 24);
    }
}
