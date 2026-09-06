//! Exact radical figures: a 45-45-90 or 30-60-90 right triangle, and the area
//! of a triangle given two sides and their included angle.
//!
//! An author never types a radical here. Every irrational side length or area
//! is *computed* by this module from a plain rational input (one leg, or two
//! sides and a degree measure), using the fixed, well-known exact ratio of the
//! family. There is therefore nothing for an author to get wrong about a
//! square root: the number never passes through their hands.

use num_rational::BigRational;
use num_traits::{One, Signed, Zero};
use serde::{Deserialize, Serialize};

use super::plane::exact_text;
use super::{Scalar, VisualError};

/// One exact figure this module draws.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpecialTriangleShape {
    /// A right triangle with two equal legs and a hypotenuse of `leg * √2`.
    FortyFiveFortyFiveNinety {
        /// The length of each leg. Positive.
        leg: Scalar,
    },
    /// A right triangle with sides `short_leg`, `short_leg * √3`, and
    /// `2 * short_leg`, opposite the 30°, 60°, and 90° angles in turn.
    ThirtySixtyNinety {
        /// The leg opposite the 30° angle. Positive.
        short_leg: Scalar,
    },
    /// The area of a triangle given two sides and their included angle,
    /// `(1/2) * side_a * side_b * sin(included_angle_degrees)`.
    SasArea {
        /// One side. Positive.
        side_a: Scalar,
        /// The other side. Positive.
        side_b: Scalar,
        /// The angle between them, in degrees. Must be one of the angles
        /// [`EXACT_SINES`] names, so the area is exact rather than merely
        /// numeric.
        included_angle_degrees: Scalar,
    },
}

/// A drawn exact figure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpecialTriangleFigure {
    /// The drawn figure.
    pub figure: SpecialTriangleShape,
    /// The sentence that leads the accessible equivalent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

/// `sin(degrees) = numerator / denominator * √radicand`, for the degree
/// measures this module accepts as an included angle. `radicand = 1` means a
/// rational sine with no radical.
const EXACT_SINES: &[(i64, i64, i64, i64)] = &[
    (30, 1, 2, 1),
    (45, 1, 2, 2),
    (60, 1, 2, 3),
    (90, 1, 1, 1),
    (120, 1, 2, 3),
    (135, 1, 2, 2),
    (150, 1, 2, 1),
];

/// The exact sine of `degrees`, as `coefficient * √radicand`, or `None` when
/// `degrees` is not one of [`EXACT_SINES`].
fn exact_sine(degrees: &BigRational) -> Option<(BigRational, i64)> {
    for (deg, num, den, radicand) in EXACT_SINES {
        if *degrees == BigRational::new((*deg).into(), 1.into()) {
            return Some((BigRational::new((*num).into(), (*den).into()), *radicand));
        }
    }
    None
}

/// `coefficient * √radicand`, in words: `"4"`, `"√2"`, `"4√2"`, `"-4√2"`.
/// `radicand <= 1` prints the coefficient alone.
pub(super) fn radical_text(coefficient: &BigRational, radicand: i64) -> String {
    if radicand <= 1 || coefficient.is_zero() {
        return exact_text(coefficient);
    }
    if coefficient.is_one() {
        format!("√{radicand}")
    } else if (-coefficient).is_one() {
        format!("-√{radicand}")
    } else {
        format!("{}√{radicand}", exact_text(coefficient))
    }
}

/// An error unless `value` is positive.
fn positive(what: &'static str, value: &Scalar) -> Result<BigRational, VisualError> {
    let exact = value.value()?;
    if exact.is_positive() {
        Ok(exact)
    } else {
        Err(VisualError::Degenerate {
            reason: format!("{what} is {value} and must be positive"),
        })
    }
}

impl SpecialTriangleFigure {
    /// Whether the figure's inputs carry mathematical meaning.
    pub fn validate(&self) -> Result<(), VisualError> {
        match &self.figure {
            SpecialTriangleShape::FortyFiveFortyFiveNinety { leg } => {
                positive("a 45-45-90 triangle's leg", leg)?;
                Ok(())
            }
            SpecialTriangleShape::ThirtySixtyNinety { short_leg } => {
                positive("a 30-60-90 triangle's short leg", short_leg)?;
                Ok(())
            }
            SpecialTriangleShape::SasArea {
                side_a,
                side_b,
                included_angle_degrees,
            } => {
                positive("a triangle side", side_a)?;
                positive("a triangle side", side_b)?;
                let degrees = included_angle_degrees.value()?;
                if exact_sine(&degrees).is_none() {
                    return Err(VisualError::Degenerate {
                        reason: format!(
                            "the included angle {included_angle_degrees}° has no exact known sine"
                        ),
                    });
                }
                Ok(())
            }
        }
    }

    /// The accessible equivalent, stating every side or the area as an exact
    /// value: an integer, a fraction, or `coefficient√radicand`.
    #[must_use]
    pub fn text_equivalent(&self) -> String {
        match &self.figure {
            SpecialTriangleShape::FortyFiveFortyFiveNinety { leg } => {
                let leg_value = leg.value().unwrap_or_else(|_| BigRational::zero());
                format!(
                    "A 45-45-90 right triangle with legs {leg} and {leg} and hypotenuse {}.",
                    radical_text(&leg_value, 2)
                )
            }
            SpecialTriangleShape::ThirtySixtyNinety { short_leg } => {
                let short = short_leg.value().unwrap_or_else(|_| BigRational::zero());
                format!(
                    "A 30-60-90 right triangle with the short leg {short_leg} opposite the 30° \
                     angle, the long leg {} opposite the 60° angle, and the hypotenuse {} \
                     opposite the 90° angle.",
                    radical_text(&short, 3),
                    exact_text(&(short * num_bigint::BigInt::from(2)))
                )
            }
            SpecialTriangleShape::SasArea {
                side_a,
                side_b,
                included_angle_degrees,
            } => {
                let (a, b) = (
                    side_a.value().unwrap_or_else(|_| BigRational::zero()),
                    side_b.value().unwrap_or_else(|_| BigRational::zero()),
                );
                let area_text = exact_sine(
                    &included_angle_degrees
                        .value()
                        .unwrap_or_else(|_| BigRational::zero()),
                )
                .map_or_else(
                    || "unknown".to_owned(),
                    |(coeff, radicand)| {
                        radical_text(
                            &(a * b * coeff / BigRational::from_integer(2.into())),
                            radicand,
                        )
                    },
                );
                format!(
                    "A triangle with two sides {side_a} and {side_b} and an included angle of \
                     {included_angle_degrees}°. The exact area is {area_text} square units."
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SpecialTriangleFigure, SpecialTriangleShape};
    use crate::visual::{Scalar, VisualError};

    #[test]
    fn a_forty_five_triangle_computes_its_hypotenuse_and_never_lets_an_author_type_one() {
        let figure = SpecialTriangleFigure {
            figure: SpecialTriangleShape::FortyFiveFortyFiveNinety {
                leg: Scalar::from("4"),
            },
            caption: None,
        };
        assert!(figure.validate().is_ok());
        assert_eq!(
            figure.text_equivalent(),
            "A 45-45-90 right triangle with legs 4 and 4 and hypotenuse 4√2."
        );

        let unit = SpecialTriangleFigure {
            figure: SpecialTriangleShape::FortyFiveFortyFiveNinety {
                leg: Scalar::from("1"),
            },
            caption: None,
        };
        assert!(unit.text_equivalent().contains("hypotenuse √2."));

        let bad = SpecialTriangleFigure {
            figure: SpecialTriangleShape::FortyFiveFortyFiveNinety {
                leg: Scalar::from("0"),
            },
            caption: None,
        };
        assert!(matches!(
            bad.validate(),
            Err(VisualError::Degenerate { .. })
        ));
    }

    #[test]
    fn a_thirty_sixty_triangle_computes_the_long_leg_and_the_hypotenuse() {
        let figure = SpecialTriangleFigure {
            figure: SpecialTriangleShape::ThirtySixtyNinety {
                short_leg: Scalar::from("3"),
            },
            caption: Some("Reference example".to_owned()),
        };
        assert!(figure.validate().is_ok());
        assert_eq!(
            figure.text_equivalent(),
            "A 30-60-90 right triangle with the short leg 3 opposite the 30° angle, the long \
             leg 3√3 opposite the 60° angle, and the hypotenuse 6 opposite the 90° angle."
        );
    }

    #[test]
    fn a_sas_area_is_exact_at_a_known_angle_and_refused_elsewhere() {
        let figure = SpecialTriangleFigure {
            figure: SpecialTriangleShape::SasArea {
                side_a: Scalar::from("4"),
                side_b: Scalar::from("5"),
                included_angle_degrees: Scalar::from("60"),
            },
            caption: None,
        };
        assert!(figure.validate().is_ok());
        assert_eq!(
            figure.text_equivalent(),
            "A triangle with two sides 4 and 5 and an included angle of 60°. The exact area \
             is 5√3 square units."
        );

        let right_angle = SpecialTriangleFigure {
            figure: SpecialTriangleShape::SasArea {
                side_a: Scalar::from("3"),
                side_b: Scalar::from("7"),
                included_angle_degrees: Scalar::from("90"),
            },
            caption: None,
        };
        assert!(
            right_angle
                .text_equivalent()
                .contains("area is 21/2 square units.")
        );

        let unknown = SpecialTriangleFigure {
            figure: SpecialTriangleShape::SasArea {
                side_a: Scalar::from("4"),
                side_b: Scalar::from("5"),
                included_angle_degrees: Scalar::from("40"),
            },
            caption: None,
        };
        assert!(matches!(
            unknown.validate(),
            Err(VisualError::Degenerate { .. })
        ));
    }
}
