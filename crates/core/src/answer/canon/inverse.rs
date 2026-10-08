//! Inverse trigonometric functions at their standard arguments.
//!
//! `acos(1/2)` and `pi/3` are one value, and so are `atan(1)` and `pi/4`. The
//! table holds the arguments whose angle is a rational multiple of `pi` on the
//! principal branch. Every other argument stays one call atom.

use num_bigint::BigInt;
use num_rational::BigRational;

use super::sum::atom_value;
use super::{Atom, Canon, Undecidable, Work};

/// One standard argument and its angle as the multiple `numerator / denominator` of `pi`.
type Angle = (&'static str, i64, i64);

/// `acos` at its standard arguments.
const ACOS: [Angle; 9] = [
    ("1", 0, 1),
    ("sqrt(3)/2", 1, 6),
    ("sqrt(2)/2", 1, 4),
    ("1/2", 1, 3),
    ("0", 1, 2),
    ("-1/2", 2, 3),
    ("-sqrt(2)/2", 3, 4),
    ("-sqrt(3)/2", 5, 6),
    ("-1", 1, 1),
];

/// `asin` at its standard arguments.
const ASIN: [Angle; 9] = [
    ("0", 0, 1),
    ("1/2", 1, 6),
    ("sqrt(2)/2", 1, 4),
    ("sqrt(3)/2", 1, 3),
    ("1", 1, 2),
    ("-1/2", -1, 6),
    ("-sqrt(2)/2", -1, 4),
    ("-sqrt(3)/2", -1, 3),
    ("-1", -1, 2),
];

/// `atan` at its standard arguments.
const ATAN: [Angle; 7] = [
    ("0", 0, 1),
    ("sqrt(3)/3", 1, 6),
    ("1", 1, 4),
    ("sqrt(3)", 1, 3),
    ("-sqrt(3)/3", -1, 6),
    ("-1", -1, 4),
    ("-sqrt(3)", -1, 3),
];

impl Work {
    /// The angle of `name(argument)` as a multiple of `pi`, or `None` when the
    /// argument is not a standard one.
    pub(super) fn inverse_trig(
        &mut self,
        name: &str,
        argument: &Canon,
    ) -> Result<Option<Canon>, Undecidable> {
        let table: &[Angle] = match name {
            "acos" => &ACOS,
            "asin" => &ASIN,
            "atan" => &ATAN,
            _ => return Ok(None),
        };
        if !matches!(argument, Canon::Rational(_) | Canon::Radical(_)) {
            return Ok(None);
        }
        for (text, numerator, denominator) in table {
            let Ok(known) = crate::answer::canonical_form(text) else {
                continue;
            };
            if &known != argument {
                continue;
            }
            let multiple = Canon::Rational(BigRational::new(
                BigInt::from(*numerator),
                BigInt::from(*denominator),
            ));
            return self.multiply(&multiple, &atom_value(Atom::Pi)).map(Some);
        }
        Ok(None)
    }
}
