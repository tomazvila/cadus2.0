//! The unit table of the value-with-unit production (D-F3).
//!
//! A number followed by one unit token of this table reads into a quantity: the
//! value, scaled into the base unit of its kind, and the kind. Two quantities of
//! one kind compare by value, so `1 m` is `100 cm` and `1.5 h` is `90 min`. Two
//! quantities of two kinds are two answers. The table is the Foundations table
//! of `docs/plans/FRAMEWORK.md` and nothing else: a spelling outside it is no
//! unit, and its letters keep their variable reading. `%` is not a unit; it is
//! the postfix of the grammar that divides by 100.

use num_bigint::BigInt;
use num_rational::BigRational;

/// The kind of a measured value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Quantity {
    /// A length, in centimeters.
    Length,
    /// A mass, in grams.
    Mass,
    /// A volume of liquid, in milliliters.
    Volume,
    /// A time, in seconds.
    Time,
    /// A speed, in meters per second.
    Speed,
    /// An area, in square centimeters.
    Area,
    /// A volume of space, in cubic centimeters.
    CubicVolume,
    /// An amount of euros.
    Euro,
    /// An amount of dollars.
    Dollar,
    /// An angle, in degrees.
    Angle,
}

/// One unit of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unit {
    /// The spelling the answer writes.
    pub spelling: &'static str,
    /// The kind the unit measures.
    pub quantity: Quantity,
    /// The count of base units in one of this unit, as a numerator and a denominator.
    pub scale: (u32, u32),
}

/// The Foundations units.
const UNITS: [Unit; 21] = [
    unit("mm", Quantity::Length, (1, 10)),
    unit("cm", Quantity::Length, (1, 1)),
    unit("m", Quantity::Length, (100, 1)),
    unit("km", Quantity::Length, (100_000, 1)),
    unit("g", Quantity::Mass, (1, 1)),
    unit("kg", Quantity::Mass, (1_000, 1)),
    unit("ml", Quantity::Volume, (1, 1)),
    unit("l", Quantity::Volume, (1_000, 1)),
    unit("L", Quantity::Volume, (1_000, 1)),
    unit("s", Quantity::Time, (1, 1)),
    unit("min", Quantity::Time, (60, 1)),
    unit("h", Quantity::Time, (3_600, 1)),
    unit("m/s", Quantity::Speed, (1, 1)),
    unit("km/h", Quantity::Speed, (5, 18)),
    unit("cm^2", Quantity::Area, (1, 1)),
    unit("m^2", Quantity::Area, (10_000, 1)),
    unit("cm^3", Quantity::CubicVolume, (1, 1)),
    unit("m^3", Quantity::CubicVolume, (1_000_000, 1)),
    unit("€", Quantity::Euro, (1, 1)),
    unit("$", Quantity::Dollar, (1, 1)),
    unit("°", Quantity::Angle, (1, 1)),
];

/// Build one table entry.
const fn unit(spelling: &'static str, quantity: Quantity, scale: (u32, u32)) -> Unit {
    Unit {
        spelling,
        quantity,
        scale,
    }
}

impl Unit {
    /// The count of base units in one of this unit, as an exact rational.
    #[must_use]
    pub fn factor(&self) -> BigRational {
        BigRational::new(BigInt::from(self.scale.0), BigInt::from(self.scale.1))
    }
}

/// Look up a unit by the spelling the answer writes.
#[must_use]
pub fn lookup(spelling: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|unit| unit.spelling == spelling)
}
