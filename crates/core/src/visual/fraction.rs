//! The fraction figure: one or more wholes, each divided into equal parts.

use serde::{Deserialize, Serialize};

use super::{MAX_PARTS, MAX_WHOLES, VisualError};

/// The two shapes a fraction figure draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FractionShape {
    /// A rectangle divided into equal vertical parts.
    #[default]
    Bar,
    /// A circle divided into equal sectors.
    Circle,
}

impl FractionShape {
    /// The singular noun the accessible equivalent uses.
    const fn noun(self) -> &'static str {
        match self {
            Self::Bar => "fraction bar",
            Self::Circle => "fraction circle",
        }
    }

    /// The noun for the parts of the shape.
    const fn part_noun(self) -> &'static str {
        match self {
            Self::Bar => "equal parts",
            Self::Circle => "equal sectors",
        }
    }
}

/// A part-of-a-whole figure.
///
/// `shaded` above `parts` draws more than one whole, which is how an author
/// shows an improper fraction such as 5/3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FractionFigure {
    /// The equal parts of one whole: the denominator.
    pub parts: i64,
    /// The shaded parts over every drawn whole: the numerator.
    pub shaded: i64,
    /// The shape of one whole.
    #[serde(default)]
    pub shape: FractionShape,
    /// The sentence that leads the accessible equivalent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

impl FractionFigure {
    /// A bar that shows `shaded`/`parts`.
    #[must_use]
    pub const fn bar(shaded: i64, parts: i64) -> Self {
        Self {
            parts,
            shaded,
            shape: FractionShape::Bar,
            caption: None,
        }
    }

    /// A circle that shows `shaded`/`parts`.
    #[must_use]
    pub const fn circle(shaded: i64, parts: i64) -> Self {
        Self {
            parts,
            shaded,
            shape: FractionShape::Circle,
            caption: None,
        }
    }

    /// The count of wholes the figure draws: one for a proper fraction, more for
    /// an improper one.
    #[must_use]
    pub fn wholes(&self) -> i64 {
        if self.parts <= 0 || self.shaded <= self.parts {
            return 1;
        }
        self.shaded.div_euclid(self.parts) + i64::from(self.shaded.rem_euclid(self.parts) != 0)
    }

    /// Whether the figure carries mathematical meaning.
    pub fn validate(&self) -> Result<(), VisualError> {
        if self.parts <= 0 || self.parts > MAX_PARTS {
            return Err(VisualError::BadPartCount { parts: self.parts });
        }
        if self.shaded < 0 || self.shaded > self.parts.saturating_mul(MAX_WHOLES) {
            return Err(VisualError::BadShadedCount {
                shaded: self.shaded,
                parts: self.parts,
            });
        }
        Ok(())
    }

    /// The accessible equivalent.
    #[must_use]
    pub fn text_equivalent(&self) -> String {
        let wholes = self.wholes();
        let head = if wholes > 1 {
            format!(
                "{wholes} {}s, each divided into {} {}",
                self.shape.noun(),
                self.parts,
                self.shape.part_noun()
            )
        } else {
            format!(
                "A {} divided into {} {}",
                self.shape.noun(),
                self.parts,
                self.shape.part_noun()
            )
        };
        format!(
            "{head}, with {} of them shaded. The figure shows {}/{}.",
            self.shaded, self.shaded, self.parts
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{FractionFigure, FractionShape};
    use crate::visual::{MAX_PARTS, MAX_WHOLES, VisualError};

    #[test]
    fn a_proper_fraction_draws_one_whole_and_reads_out_loud() {
        let figure = FractionFigure::bar(3, 4);
        assert!(figure.validate().is_ok());
        assert_eq!(figure.wholes(), 1);
        assert_eq!(
            figure.text_equivalent(),
            "A fraction bar divided into 4 equal parts, with 3 of them shaded. \
             The figure shows 3/4."
        );
    }

    #[test]
    fn the_circle_shape_names_sectors() {
        let figure = FractionFigure::circle(1, 2);
        assert_eq!(figure.shape, FractionShape::Circle);
        assert!(figure.text_equivalent().contains("fraction circle"));
        assert!(figure.text_equivalent().contains("2 equal sectors"));
    }

    #[test]
    fn the_two_degenerate_shading_counts_stay_valid() {
        assert!(FractionFigure::bar(0, 4).validate().is_ok());
        assert_eq!(FractionFigure::bar(0, 4).wholes(), 1);
        assert!(FractionFigure::bar(4, 4).validate().is_ok());
        assert_eq!(FractionFigure::bar(4, 4).wholes(), 1);
        assert!(FractionFigure::bar(0, 4).text_equivalent().contains("0/4"));
    }

    #[test]
    fn an_improper_fraction_draws_the_wholes_it_needs() {
        assert_eq!(FractionFigure::bar(5, 3).wholes(), 2);
        assert_eq!(FractionFigure::bar(6, 3).wholes(), 2);
        assert_eq!(FractionFigure::bar(7, 3).wholes(), 3);
        let text = FractionFigure::bar(5, 3).text_equivalent();
        assert!(text.starts_with("2 fraction bars, each divided into 3 equal parts"));
        assert!(text.contains("shows 5/3"));
    }

    #[test]
    fn a_bad_part_count_and_a_bad_shaded_count_fail() {
        assert!(matches!(
            FractionFigure::bar(1, 0).validate(),
            Err(VisualError::BadPartCount { parts: 0 })
        ));
        assert!(matches!(
            FractionFigure::bar(1, -3).validate(),
            Err(VisualError::BadPartCount { parts: -3 })
        ));
        assert!(FractionFigure::bar(1, MAX_PARTS).validate().is_ok());
        assert!(matches!(
            FractionFigure::bar(1, MAX_PARTS + 1).validate(),
            Err(VisualError::BadPartCount { .. })
        ));
        assert!(matches!(
            FractionFigure::bar(-1, 4).validate(),
            Err(VisualError::BadShadedCount { shaded: -1, .. })
        ));
        assert!(FractionFigure::bar(4 * MAX_WHOLES, 4).validate().is_ok());
        assert!(matches!(
            FractionFigure::bar(4 * MAX_WHOLES + 1, 4).validate(),
            Err(VisualError::BadShadedCount { .. })
        ));
    }

    #[test]
    fn the_whole_count_never_divides_by_zero() {
        assert_eq!(FractionFigure::bar(9, 0).wholes(), 1);
        assert_eq!(FractionFigure::bar(9, -2).wholes(), 1);
    }
}
