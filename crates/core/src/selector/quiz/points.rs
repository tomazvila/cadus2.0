//! The points of a quiz round (D-VBE3).
//!
//! An item with no `points` counts 1. An item with `points` and a multipart
//! answer gives partial credit by named part. A round with 1-point items only
//! scores as it did before: the share of correct items.

/// The points that one answered item earned, and the points it could earn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemPoints {
    /// The points the learner earned.
    pub earned: f64,
    /// The points of the item.
    pub total: f64,
}

/// An authored point count as a float.
fn as_float(points: u8) -> f64 {
    f64::from(points)
}

/// The points of one item.
///
/// `points` is the authored value. `parts` is `(correct parts, all parts)` of a
/// multipart answer. An item with no `points` ignores `parts`, so it scores 1 or
/// 0 as before. A correct item earns all its points.
#[must_use]
pub fn item_points(points: Option<u8>, parts: Option<(usize, usize)>, correct: bool) -> ItemPoints {
    let Some(value) = points else {
        return ItemPoints {
            earned: if correct { 1.0 } else { 0.0 },
            total: 1.0,
        };
    };
    let total = as_float(value);
    let earned = if correct {
        total
    } else {
        match parts {
            Some((right, all)) if all > 0 => {
                total
                    * (f64::from(u32::try_from(right.min(all)).unwrap_or(0))
                        / f64::from(u32::try_from(all).unwrap_or(1)))
            }
            _ => 0.0,
        }
    };
    ItemPoints { earned, total }
}

/// The sum of the earned points and the sum of the item points.
#[must_use]
pub fn sum_points(items: &[ItemPoints]) -> ItemPoints {
    items.iter().fold(
        ItemPoints {
            earned: 0.0,
            total: 0.0,
        },
        |sum, item| ItemPoints {
            earned: sum.earned + item.earned,
            total: sum.total + item.total,
        },
    )
}

/// The score of a round: earned points over all points. An empty round scores 0.
#[must_use]
pub fn score_of(items: &[ItemPoints]) -> f64 {
    let sum = sum_points(items);
    if sum.total > 0.0 {
        sum.earned / sum.total
    } else {
        0.0
    }
}

/// A point count rounded to 2 decimals, for a reply.
#[must_use]
pub fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
