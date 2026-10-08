//! Trigonometric functions at exact special angles.
//!
//! `sin(pi/6)` and `1/2` are one value. The table holds the angles that are a
//! rational multiple of `pi` with a denominator of 1, 2, 3, 4, or 6, and the
//! angle 0. A function that is undefined at the angle (`tan(pi/2)`) and every
//! other argument stay one call atom.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{ToPrimitive, Zero};

use super::{Basis, Canon, Undecidable, Work};

/// The sine of the angles 0, 30, 45, 60, and 90 degrees.
const SINE: [&str; 5] = ["0", "1/2", "sqrt(2)/2", "sqrt(3)/2", "1"];

/// The angle `argument` in degrees, when it is a special angle.
///
/// The argument is zero or a rational multiple of `pi` whose denominator is 1,
/// 2, 3, 4, or 6. The result is reduced into 0 up to (not including) 360.
fn degrees(argument: &Canon) -> Option<i64> {
    let multiple = match argument {
        Canon::Rational(value) if value.is_zero() => return Some(0),
        Canon::Radical(terms) if terms.len() == 1 => {
            let (basis, coefficient) = terms.iter().next()?;
            let pure_pi = *basis
                == Basis {
                    radicand: BigInt::from(1),
                    pi: 1,
                    e: 0,
                };
            if !pure_pi {
                return None;
            }
            coefficient
        }
        _ => return None,
    };
    let denominator = multiple.denom().to_i64()?;
    if !matches!(denominator, 1..=4 | 6) {
        return None;
    }
    let turns = BigRational::new(multiple.numer().clone() * 180, multiple.denom().clone());
    let whole = turns.to_integer().to_i64()?;
    Some(whole.rem_euclid(360))
}

/// The exact sine of `angle` degrees, as a canonical text.
fn sine(angle: i64) -> String {
    let angle = angle.rem_euclid(360);
    let (negative, within) = if angle > 180 {
        (true, angle - 180)
    } else {
        (false, angle)
    };
    let reference = if within > 90 { 180 - within } else { within };
    let value = match reference {
        30 => SINE[1],
        45 => SINE[2],
        60 => SINE[3],
        90 => SINE[4],
        _ => SINE[0],
    };
    if negative && value != "0" {
        format!("-{value}")
    } else {
        value.to_string()
    }
}

impl Work {
    /// The exact value of `name(argument)` at a special angle, or `None`.
    pub(super) fn exact_trig(
        &mut self,
        name: &str,
        argument: &Canon,
    ) -> Result<Option<Canon>, Undecidable> {
        if !matches!(name, "sin" | "cos" | "tan" | "sec" | "csc" | "cot") {
            return Ok(None);
        }
        let Some(angle) = degrees(argument) else {
            return Ok(None);
        };
        let sin = sine(angle);
        let cos = sine(angle + 90);
        let (top, bottom) = match name {
            "sin" => (sin, "1".to_string()),
            "cos" => (cos, "1".to_string()),
            "tan" => (sin, cos),
            "cot" => (cos, sin),
            "sec" => ("1".to_string(), cos),
            _ => ("1".to_string(), sin),
        };
        if bottom == "0" {
            return Ok(None);
        }
        self.spend(1)?;
        Ok(crate::answer::canonical_form(&format!("({top})/({bottom})")).ok())
    }
}
