//! The number line: a range, a tick step, marked points, and marked intervals.

use serde::{Deserialize, Serialize};

use super::{Scalar, VisualError, inside, tick_count};

/// One marked point of a number line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkedPoint {
    /// Where the point sits.
    pub at: Scalar,
    /// The text beside the point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// A filled point marks a value the range holds. An open point marks a value
    /// the range excludes.
    #[serde(default = "yes")]
    pub filled: bool,
}

/// One marked interval of a number line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkedInterval {
    /// The left end.
    pub from: Scalar,
    /// The right end.
    pub to: Scalar,
    /// Whether the left end belongs to the interval.
    #[serde(default = "yes")]
    pub closed_start: bool,
    /// Whether the right end belongs to the interval.
    #[serde(default = "yes")]
    pub closed_end: bool,
    /// The text beside the interval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// A number line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumberLineFigure {
    /// The left end of the drawn range.
    pub min: Scalar,
    /// The right end of the drawn range.
    pub max: Scalar,
    /// The distance between two ticks.
    pub tick: Scalar,
    /// The marked points, in authored order.
    #[serde(default)]
    pub points: Vec<MarkedPoint>,
    /// The marked intervals, in authored order.
    #[serde(default)]
    pub intervals: Vec<MarkedInterval>,
    /// The sentence that leads the accessible equivalent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

/// The serde default of a boolean field that defaults to `true`.
const fn yes() -> bool {
    true
}

impl NumberLineFigure {
    /// A line from `min` to `max` with a tick every `tick` and nothing marked.
    #[must_use]
    pub fn new(min: impl Into<Scalar>, max: impl Into<Scalar>, tick: impl Into<Scalar>) -> Self {
        Self {
            min: min.into(),
            max: max.into(),
            tick: tick.into(),
            points: Vec::new(),
            intervals: Vec::new(),
            caption: None,
        }
    }

    /// Add one filled point.
    #[must_use]
    pub fn with_point(mut self, at: impl Into<Scalar>, label: Option<&str>) -> Self {
        self.points.push(MarkedPoint {
            at: at.into(),
            label: label.map(ToOwned::to_owned),
            filled: true,
        });
        self
    }

    /// The count of ticks the line draws.
    pub fn ticks(&self) -> Result<i64, VisualError> {
        tick_count("line", &self.min, &self.max, &self.tick)
    }

    /// Whether the line carries mathematical meaning.
    pub fn validate(&self) -> Result<(), VisualError> {
        self.ticks()?;
        for point in &self.points {
            inside("a point", &self.min, &self.max, &point.at)?;
        }
        for interval in &self.intervals {
            inside("an interval end", &self.min, &self.max, &interval.from)?;
            inside("an interval end", &self.min, &self.max, &interval.to)?;
            if interval.to.value()? < interval.from.value()? {
                return Err(VisualError::RangeNotAscending { axis: "line" });
            }
        }
        Ok(())
    }

    /// The accessible equivalent.
    #[must_use]
    pub fn text_equivalent(&self) -> String {
        let mut out = format!(
            "A number line from {} to {} with a tick every {}.",
            self.min, self.max, self.tick
        );
        for point in &self.points {
            let fill = if point.filled {
                "A filled point"
            } else {
                "An open point"
            };
            out.push_str(&format!(" {fill} at {}", point.at));
            if let Some(label) = label_of(point.label.as_deref()) {
                out.push_str(&format!(", labeled {label}"));
            }
            out.push('.');
        }
        for interval in &self.intervals {
            out.push_str(&format!(
                " {} from {} to {}",
                bracket_words(interval.closed_start, interval.closed_end),
                interval.from,
                interval.to
            ));
            if let Some(label) = label_of(interval.label.as_deref()) {
                out.push_str(&format!(", labeled {label}"));
            }
            out.push('.');
        }
        out
    }
}

/// The words that name the two ends of one interval.
fn bracket_words(closed_start: bool, closed_end: bool) -> &'static str {
    match (closed_start, closed_end) {
        (true, true) => "A closed interval",
        (false, false) => "An open interval",
        (true, false) => "An interval closed at the left end and open at the right end",
        (false, true) => "An interval open at the left end and closed at the right end",
    }
}

/// The label text, or `None` for an absent or blank label.
pub(super) fn label_of(label: Option<&str>) -> Option<&str> {
    label.map(str::trim).filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{MarkedInterval, MarkedPoint, NumberLineFigure};
    use crate::visual::{Scalar, VisualError};

    fn interval(from: &str, to: &str, closed_start: bool, closed_end: bool) -> MarkedInterval {
        MarkedInterval {
            from: Scalar::from(from),
            to: Scalar::from(to),
            closed_start,
            closed_end,
            label: None,
        }
    }

    #[test]
    fn a_line_with_a_point_on_a_tick_validates_and_reads_out_loud() {
        let line = NumberLineFigure::new(0_i64, 5_i64, 1_i64).with_point(3_i64, Some(" x "));
        assert!(line.validate().is_ok());
        assert_eq!(line.ticks().unwrap(), 6);
        assert_eq!(
            line.text_equivalent(),
            "A number line from 0 to 5 with a tick every 1. A filled point at 3, labeled x."
        );
    }

    #[test]
    fn the_boundary_points_of_the_range_stay_inside_and_one_step_outside_fails() {
        let inside = NumberLineFigure::new(0_i64, 5_i64, 1_i64)
            .with_point(0_i64, None)
            .with_point(5_i64, None);
        assert!(inside.validate().is_ok());

        let outside = NumberLineFigure::new(0_i64, 5_i64, 1_i64).with_point("5.0001", None);
        assert!(matches!(
            outside.validate(),
            Err(VisualError::OutOfRange { .. })
        ));
        let below = NumberLineFigure::new(0_i64, 5_i64, 1_i64).with_point("-1/1000", None);
        assert!(matches!(
            below.validate(),
            Err(VisualError::OutOfRange { .. })
        ));
    }

    #[test]
    fn a_collapsed_range_a_zero_tick_and_a_fine_tick_all_fail() {
        assert!(matches!(
            NumberLineFigure::new(5_i64, 5_i64, 1_i64).validate(),
            Err(VisualError::RangeNotAscending { axis: "line" })
        ));
        assert!(matches!(
            NumberLineFigure::new(5_i64, 0_i64, 1_i64).validate(),
            Err(VisualError::RangeNotAscending { axis: "line" })
        ));
        assert!(matches!(
            NumberLineFigure::new(0_i64, 5_i64, 0_i64).validate(),
            Err(VisualError::TickNotPositive { axis: "line" })
        ));
        assert!(matches!(
            NumberLineFigure::new(0_i64, 5_i64, "-1").validate(),
            Err(VisualError::TickNotPositive { axis: "line" })
        ));
        let too_fine = NumberLineFigure::new(0_i64, 5_i64, "1/1000");
        assert!(matches!(
            too_fine.validate(),
            Err(VisualError::TooManyTicks {
                axis: "line",
                ticks: 5000
            })
        ));
    }

    #[test]
    fn the_exact_tick_limit_passes_and_one_more_tick_fails() {
        assert_eq!(
            NumberLineFigure::new(0_i64, 200_i64, 1_i64)
                .ticks()
                .unwrap(),
            201
        );
        assert!(matches!(
            NumberLineFigure::new(0_i64, 201_i64, 1_i64).ticks(),
            Err(VisualError::TooManyTicks { ticks: 201, .. })
        ));
    }

    #[test]
    fn every_interval_form_reads_out_loud_and_a_reversed_one_fails() {
        let mut line = NumberLineFigure::new(0_i64, 5_i64, 1_i64);
        line.intervals = vec![
            interval("1", "2", true, true),
            interval("2", "3", false, false),
            interval("3", "4", true, false),
            interval("4", "5", false, true),
        ];
        assert!(line.validate().is_ok());
        let text = line.text_equivalent();
        assert!(text.contains("A closed interval from 1 to 2."));
        assert!(text.contains("An open interval from 2 to 3."));
        assert!(text.contains("closed at the left end and open at the right end from 3 to 4."));
        assert!(text.contains("open at the left end and closed at the right end from 4 to 5."));

        let mut reversed = NumberLineFigure::new(0_i64, 5_i64, 1_i64);
        reversed.intervals = vec![interval("4", "1", true, true)];
        assert!(matches!(
            reversed.validate(),
            Err(VisualError::RangeNotAscending { axis: "line" })
        ));

        let mut empty = NumberLineFigure::new(0_i64, 5_i64, 1_i64);
        empty.intervals = vec![interval("2", "2", true, true)];
        assert!(empty.validate().is_ok());

        let mut outside = NumberLineFigure::new(0_i64, 5_i64, 1_i64);
        outside.intervals = vec![interval("1", "9", true, true)];
        assert!(matches!(
            outside.validate(),
            Err(VisualError::OutOfRange { .. })
        ));
    }

    #[test]
    fn an_open_point_and_a_labeled_interval_reach_the_accessible_text() {
        let mut line = NumberLineFigure::new(0_i64, 5_i64, 1_i64);
        line.points = vec![MarkedPoint {
            at: Scalar::from("2"),
            label: Some("   ".to_owned()),
            filled: false,
        }];
        let mut labeled = interval("1", "2", true, true);
        labeled.label = Some("A".to_owned());
        line.intervals = vec![labeled];
        let text = line.text_equivalent();
        assert!(text.contains("An open point at 2."));
        assert!(text.contains("A closed interval from 1 to 2, labeled A."));
    }

    #[test]
    fn a_bad_number_stops_the_check_before_the_range_check() {
        let line = NumberLineFigure::new("oops", 5_i64, 1_i64);
        assert!(matches!(
            line.validate(),
            Err(VisualError::NotANumber { .. })
        ));
    }
}
