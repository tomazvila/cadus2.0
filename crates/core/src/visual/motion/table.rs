//! The wire types of the motion table: what the server sends beside the SVG.
//!
//! The browser reads these numbers and writes `transform` and `opacity`. It does
//! no geometry. A matrix entry at slider index `i` is `k * i + c`.

use serde::{Deserialize, Serialize};

/// One law that moves one group of the figure, in the slider index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Law {
    /// The six entries `a b c d e f` of a CSS `matrix()`, each as `[k, c]`.
    Matrix {
        /// The entries, in the order `a b c d e f`.
        m: [[f64; 2]; 6],
    },
    /// A turn about one point of the view box, in degrees as `[k, c]`.
    Rotate {
        /// The clockwise angle on the screen.
        deg: [f64; 2],
        /// The x of the pivot.
        cx: f64,
        /// The y of the pivot.
        cy: f64,
    },
    /// The group shows (opacity 1) from the index `from` up, and hides below it.
    Reveal {
        /// The first index at which the group shows.
        from: u32,
    },
}

/// The law of one moving group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupLaw {
    /// The `id` of the `<g>` in the SVG.
    pub id: String,
    /// How the group moves.
    pub law: Law,
}

/// The slider: its name and the exact text of every step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterTable {
    /// The plain name of the slider.
    pub name: String,
    /// The unit, or an empty text.
    pub unit: String,
    /// The text of every value, `n + 1` of them.
    pub labels: Vec<String>,
    /// The index of the default value.
    pub default_index: usize,
    /// The count of steps.
    pub n: usize,
}

/// One readout line: a name and the text at every step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Readout {
    /// The name of the line.
    pub label: String,
    /// The text at every index, `n + 1` of them.
    pub texts: Vec<String>,
}

/// Everything the browser needs to move one figure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionTable {
    /// The slider.
    pub parameter: ParameterTable,
    /// The law of each moving group.
    pub groups: Vec<GroupLaw>,
    /// The readout lines under the slider.
    pub readouts: Vec<Readout>,
    /// The exact text equivalent at every index.
    pub summary: Vec<String>,
}

/// Round to four places once, and never print a negative zero.
#[must_use]
pub fn round4(value: f64) -> f64 {
    let rounded = (value * 10_000.0).round() / 10_000.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

/// The entry `k * i + c` as a rounded pair.
#[must_use]
pub fn affine(k: f64, c: f64) -> [f64; 2] {
    [round4(k), round4(c)]
}

impl MotionTable {
    /// Build the table of a figure with motion from the laws the render made.
    ///
    /// # Errors
    ///
    /// Returns the refusal when a step figure does not hold, or when the table
    /// is larger than [`super::MAX_TABLE_BYTES`].
    pub(in crate::visual) fn build(
        spec: &crate::visual::VisualSpec,
        motion: &super::Motion,
        steps: &super::Steps,
        groups: Vec<GroupLaw>,
    ) -> Result<Self, crate::visual::VisualError> {
        let shown = super::step_specs(spec, motion, steps)?;
        for figure in &shown {
            figure.validate()?;
        }
        let table = Self {
            parameter: ParameterTable {
                name: motion.parameter.name.trim().to_owned(),
                unit: motion.parameter.unit.clone().unwrap_or_default(),
                labels: (0..=steps.n)
                    .map(|index| motion.value_text(&steps.value(index)))
                    .collect(),
                default_index: steps.default_index,
                n: steps.n,
            },
            groups,
            readouts: super::readouts(motion, &shown)?,
            summary: shown
                .iter()
                .map(crate::visual::VisualSpec::text_equivalent)
                .collect(),
        };
        let bytes = serde_json::to_string(&table).map_or(usize::MAX, |text| text.len());
        if bytes > super::MAX_TABLE_BYTES {
            return Err(super::MotionError::TableTooLarge {
                bytes,
                limit: super::MAX_TABLE_BYTES,
            }
            .into());
        }
        Ok(table)
    }
}
