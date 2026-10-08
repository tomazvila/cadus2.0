//! The unit table of the value-with-unit production (D-F3).
//!
//! A number followed by one unit token of this table reads into a quantity: the
//! value, scaled into the base unit of its kind, and the kind. Two quantities of
//! one kind compare by value, so `1 m` is `100 cm` and `1.5 h` is `90 min`. Two
//! quantities of two kinds are two answers. The table contains explicit measured
//! units and bounded aliases used by curriculum answers. A spelling outside it
//! keeps its ordinary grammar reading. `%` is not a unit; it is the postfix of
//! the grammar that divides by 100.

use num_bigint::BigInt;
use num_rational::BigRational;
use serde::{Deserialize, Serialize};

/// The kind of a measured value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    /// A temperature, in degrees Celsius.
    Temperature,
    /// A volume flow rate, in milliliters per second.
    Flow,
    /// An energy, in joules.
    Energy,
    /// A euro price per kilogram.
    EuroPerMass,
    /// A length per volume, in centimeters per milliliter.
    LengthPerVolume,
    /// A mass per volume of liquid, in milligrams per liter.
    Concentration,
    /// An amount of substance per volume, in moles per liter.
    Molarity,
    /// A power, in watts.
    Power,
    /// A force, in newtons.
    Force,
    /// An electric potential, in volts.
    Voltage,
    /// A pressure, in pascals.
    Pressure,
    /// An acceleration, in meters per second squared.
    Acceleration,
    /// A mass per volume of space, in grams per cubic centimeter.
    Density,
    /// A mass flow rate, in grams per second.
    MassRate,
    /// A dollar price per kilogram.
    DollarPerMass,
    /// An electric resistance, in ohms.
    Resistance,
    /// A volume of liquid per length, in milliliters per centimeter.
    VolumePerLength,
    /// A spring constant, in newtons per meter.
    ForceConstant,
}

/// One unit of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unit {
    /// The spelling the answer writes.
    pub spelling: &'static str,
    /// The kind the unit measures.
    pub quantity: Quantity,
    /// The count of base units in one of this unit, as a numerator and a denominator.
    pub scale: (u64, u64),
}

/// The Foundations units.
const UNITS: &[Unit] = &[
    unit("mm", Quantity::Length, (1, 10)),
    unit("millimeter", Quantity::Length, (1, 10)),
    unit("millimeters", Quantity::Length, (1, 10)),
    unit("millimetre", Quantity::Length, (1, 10)),
    unit("millimetres", Quantity::Length, (1, 10)),
    unit("cm", Quantity::Length, (1, 1)),
    unit("centimeter", Quantity::Length, (1, 1)),
    unit("centimeters", Quantity::Length, (1, 1)),
    unit("centimetre", Quantity::Length, (1, 1)),
    unit("centimetres", Quantity::Length, (1, 1)),
    unit("m", Quantity::Length, (100, 1)),
    unit("meter", Quantity::Length, (100, 1)),
    unit("meters", Quantity::Length, (100, 1)),
    unit("metre", Quantity::Length, (100, 1)),
    unit("metres", Quantity::Length, (100, 1)),
    unit("km", Quantity::Length, (100_000, 1)),
    unit("micrometer", Quantity::Length, (1, 10_000)),
    unit("micrometers", Quantity::Length, (1, 10_000)),
    unit("micrometre", Quantity::Length, (1, 10_000)),
    unit("micrometres", Quantity::Length, (1, 10_000)),
    unit("kilometer", Quantity::Length, (100_000, 1)),
    unit("kilometers", Quantity::Length, (100_000, 1)),
    unit("kilometre", Quantity::Length, (100_000, 1)),
    unit("kilometres", Quantity::Length, (100_000, 1)),
    unit("in", Quantity::Length, (254, 100)),
    unit("inch", Quantity::Length, (254, 100)),
    unit("inches", Quantity::Length, (254, 100)),
    unit("ft", Quantity::Length, (3048, 100)),
    unit("foot", Quantity::Length, (3048, 100)),
    unit("feet", Quantity::Length, (3048, 100)),
    unit("yd", Quantity::Length, (9144, 100)),
    unit("yard", Quantity::Length, (9144, 100)),
    unit("yards", Quantity::Length, (9144, 100)),
    unit("mi", Quantity::Length, (1_609_344, 10)),
    unit("mile", Quantity::Length, (1_609_344, 10)),
    unit("miles", Quantity::Length, (1_609_344, 10)),
    unit("mg", Quantity::Mass, (1, 1_000)),
    unit("milligram", Quantity::Mass, (1, 1_000)),
    unit("milligrams", Quantity::Mass, (1, 1_000)),
    unit("g", Quantity::Mass, (1, 1)),
    unit("gram", Quantity::Mass, (1, 1)),
    unit("grams", Quantity::Mass, (1, 1)),
    unit("kg", Quantity::Mass, (1_000, 1)),
    unit("tonne", Quantity::Mass, (1_000_000, 1)),
    unit("tonnes", Quantity::Mass, (1_000_000, 1)),
    unit("kilogram", Quantity::Mass, (1_000, 1)),
    unit("kilograms", Quantity::Mass, (1_000, 1)),
    unit("lb", Quantity::Mass, (45_359_237, 100_000)),
    unit("lbs", Quantity::Mass, (45_359_237, 100_000)),
    unit("pound", Quantity::Mass, (45_359_237, 100_000)),
    unit("pounds", Quantity::Mass, (45_359_237, 100_000)),
    unit("oz", Quantity::Mass, (226_796_185, 8_000_000)),
    unit("ounce", Quantity::Mass, (226_796_185, 8_000_000)),
    unit("ounces", Quantity::Mass, (226_796_185, 8_000_000)),
    unit("ml", Quantity::Volume, (1, 1)),
    unit("mL", Quantity::Volume, (1, 1)),
    unit("milliliter", Quantity::Volume, (1, 1)),
    unit("milliliters", Quantity::Volume, (1, 1)),
    unit("millilitre", Quantity::Volume, (1, 1)),
    unit("millilitres", Quantity::Volume, (1, 1)),
    unit("l", Quantity::Volume, (1_000, 1)),
    unit("L", Quantity::Volume, (1_000, 1)),
    unit("liter", Quantity::Volume, (1_000, 1)),
    unit("liters", Quantity::Volume, (1_000, 1)),
    unit("litre", Quantity::Volume, (1_000, 1)),
    unit("litres", Quantity::Volume, (1_000, 1)),
    unit("s", Quantity::Time, (1, 1)),
    unit("second", Quantity::Time, (1, 1)),
    unit("seconds", Quantity::Time, (1, 1)),
    unit("min", Quantity::Time, (60, 1)),
    unit("minute", Quantity::Time, (60, 1)),
    unit("minutes", Quantity::Time, (60, 1)),
    unit("h", Quantity::Time, (3_600, 1)),
    unit("hr", Quantity::Time, (3_600, 1)),
    unit("hrs", Quantity::Time, (3_600, 1)),
    unit("hour", Quantity::Time, (3_600, 1)),
    unit("hours", Quantity::Time, (3_600, 1)),
    unit("day", Quantity::Time, (86_400, 1)),
    unit("days", Quantity::Time, (86_400, 1)),
    unit("week", Quantity::Time, (604_800, 1)),
    unit("weeks", Quantity::Time, (604_800, 1)),
    // A month is a twelfth of a 365-day year.
    unit("month", Quantity::Time, (2_628_000, 1)),
    unit("months", Quantity::Time, (2_628_000, 1)),
    unit("year", Quantity::Time, (31_536_000, 1)),
    unit("years", Quantity::Time, (31_536_000, 1)),
    unit("m/s", Quantity::Speed, (1, 1)),
    unit("m/min", Quantity::Speed, (1, 60)),
    unit("cm/h", Quantity::Speed, (1, 360_000)),
    unit("cm/hour", Quantity::Speed, (1, 360_000)),
    unit("km/h", Quantity::Speed, (5, 18)),
    unit("mph", Quantity::Speed, (1_397, 3_125)),
    unit("cm^2", Quantity::Area, (1, 1)),
    unit("m^2", Quantity::Area, (10_000, 1)),
    unit("cm^3", Quantity::CubicVolume, (1, 1)),
    unit("m^3", Quantity::CubicVolume, (1_000_000, 1)),
    unit("€", Quantity::Euro, (1, 1)),
    unit("euro", Quantity::Euro, (1, 1)),
    unit("euros", Quantity::Euro, (1, 1)),
    unit("cent", Quantity::Euro, (1, 100)),
    unit("cents", Quantity::Euro, (1, 100)),
    unit("$", Quantity::Dollar, (1, 1)),
    unit("dollar", Quantity::Dollar, (1, 1)),
    unit("dollars", Quantity::Dollar, (1, 1)),
    unit("°", Quantity::Angle, (1, 1)),
    unit("degree", Quantity::Angle, (1, 1)),
    unit("degrees", Quantity::Angle, (1, 1)),
    unit("°C", Quantity::Temperature, (1, 1)),
    unit("celsius", Quantity::Temperature, (1, 1)),
    unit("Celsius", Quantity::Temperature, (1, 1)),
    unit("°F", Quantity::Temperature, (5, 9)),
    unit("F", Quantity::Temperature, (5, 9)),
    unit("fahrenheit", Quantity::Temperature, (5, 9)),
    unit("Fahrenheit", Quantity::Temperature, (5, 9)),
    unit("L/min", Quantity::Flow, (50, 3)),
    unit("l/min", Quantity::Flow, (50, 3)),
    unit("L/h", Quantity::Flow, (5, 18)),
    unit("l/h", Quantity::Flow, (5, 18)),
    unit("L/hour", Quantity::Flow, (5, 18)),
    unit("l/hour", Quantity::Flow, (5, 18)),
    unit("L/hr", Quantity::Flow, (5, 18)),
    unit("ml/s", Quantity::Flow, (1, 1)),
    unit("Wh", Quantity::Energy, (3_600, 1)),
    unit("kWh", Quantity::Energy, (3_600_000, 1)),
    unit("joule", Quantity::Energy, (1, 1)),
    unit("joules", Quantity::Energy, (1, 1)),
    unit("J", Quantity::Energy, (1, 1)),
    unit("kJ", Quantity::Energy, (1_000, 1)),
    unit("kilojoule", Quantity::Energy, (1_000, 1)),
    unit("kilojoules", Quantity::Energy, (1_000, 1)),
    unit("€/kg", Quantity::EuroPerMass, (1, 1)),
    unit("euro/kg", Quantity::EuroPerMass, (1, 1)),
    unit("euros/kg", Quantity::EuroPerMass, (1, 1)),
    unit("cent/kg", Quantity::EuroPerMass, (1, 100)),
    unit("cents/kg", Quantity::EuroPerMass, (1, 100)),
    unit("€/g", Quantity::EuroPerMass, (1_000, 1)),
    unit("euro/g", Quantity::EuroPerMass, (1_000, 1)),
    unit("cent/g", Quantity::EuroPerMass, (10, 1)),
    unit("cents/g", Quantity::EuroPerMass, (10, 1)),
    unit("km/L", Quantity::LengthPerVolume, (100, 1)),
    unit("km/l", Quantity::LengthPerVolume, (100, 1)),
    unit("kilometer/liter", Quantity::LengthPerVolume, (100, 1)),
    unit("kilometers/liter", Quantity::LengthPerVolume, (100, 1)),
    unit("kilometre/litre", Quantity::LengthPerVolume, (100, 1)),
    unit("kilometres/litre", Quantity::LengthPerVolume, (100, 1)),
    unit("m/L", Quantity::LengthPerVolume, (1, 10)),
    unit("m/l", Quantity::LengthPerVolume, (1, 10)),
    unit("cm/L", Quantity::LengthPerVolume, (1, 1_000)),
    unit("cm/l", Quantity::LengthPerVolume, (1, 1_000)),
    unit("mg/L", Quantity::Concentration, (1, 1)),
    unit("mg/l", Quantity::Concentration, (1, 1)),
    unit("g/L", Quantity::Concentration, (1_000, 1)),
    unit("g/l", Quantity::Concentration, (1_000, 1)),
    unit("mol/L", Quantity::Molarity, (1, 1)),
    unit("mol/l", Quantity::Molarity, (1, 1)),
    unit("mmol/L", Quantity::Molarity, (1, 1_000)),
    unit("mmol/l", Quantity::Molarity, (1, 1_000)),
    unit("t", Quantity::Mass, (1_000_000, 1)),
    unit("dm", Quantity::Length, (10, 1)),
    unit("decimeter", Quantity::Length, (10, 1)),
    unit("decimeters", Quantity::Length, (10, 1)),
    unit("decimetre", Quantity::Length, (10, 1)),
    unit("decimetres", Quantity::Length, (10, 1)),
    unit("um", Quantity::Length, (1, 10_000)),
    unit("µm", Quantity::Length, (1, 10_000)),
    unit("μm", Quantity::Length, (1, 10_000)),
    unit("cup", Quantity::Volume, (2_365_882_365, 10_000_000)),
    unit("cups", Quantity::Volume, (2_365_882_365, 10_000_000)),
    unit("gallon", Quantity::Volume, (3_785_411_784, 1_000_000)),
    unit("gallons", Quantity::Volume, (3_785_411_784, 1_000_000)),
    unit("megaliter", Quantity::Volume, (1_000_000_000, 1)),
    unit("megaliters", Quantity::Volume, (1_000_000_000, 1)),
    unit("megalitre", Quantity::Volume, (1_000_000_000, 1)),
    unit("megalitres", Quantity::Volume, (1_000_000_000, 1)),
    unit("kph", Quantity::Speed, (5, 18)),
    unit("km/hour", Quantity::Speed, (5, 18)),
    unit("km/day", Quantity::Speed, (5, 432)),
    unit("cm/s", Quantity::Speed, (1, 100)),
    unit("km^2", Quantity::Area, (10_000_000_000, 1)),
    unit("mm^2", Quantity::Area, (1, 100)),
    unit("dm^2", Quantity::Area, (100, 1)),
    unit("mm^3", Quantity::CubicVolume, (1, 1_000)),
    unit("dm^3", Quantity::CubicVolume, (1_000, 1)),
    unit("N/m", Quantity::ForceConstant, (1, 1)),
    unit("kN/m", Quantity::ForceConstant, (1_000, 1)),
    unit("N/cm", Quantity::ForceConstant, (100, 1)),
    unit("m^3/s", Quantity::Flow, (1_000_000, 1)),
    unit("m^3/h", Quantity::Flow, (2_500, 9)),
    unit("deg", Quantity::Angle, (1, 1)),
    unit("C", Quantity::Temperature, (1, 1)),
    unit("K", Quantity::Temperature, (1, 1)),
    unit("kelvin", Quantity::Temperature, (1, 1)),
    unit("Kelvin", Quantity::Temperature, (1, 1)),
    unit("W", Quantity::Power, (1, 1)),
    unit("watt", Quantity::Power, (1, 1)),
    unit("watts", Quantity::Power, (1, 1)),
    unit("kW", Quantity::Power, (1_000, 1)),
    unit("kilowatt", Quantity::Power, (1_000, 1)),
    unit("kilowatts", Quantity::Power, (1_000, 1)),
    unit("MW", Quantity::Power, (1_000_000, 1)),
    unit("N", Quantity::Force, (1, 1)),
    unit("newton", Quantity::Force, (1, 1)),
    unit("newtons", Quantity::Force, (1, 1)),
    unit("kN", Quantity::Force, (1_000, 1)),
    unit("kilonewton", Quantity::Force, (1_000, 1)),
    unit("kilonewtons", Quantity::Force, (1_000, 1)),
    unit("V", Quantity::Voltage, (1, 1)),
    unit("volt", Quantity::Voltage, (1, 1)),
    unit("volts", Quantity::Voltage, (1, 1)),
    unit("kV", Quantity::Voltage, (1_000, 1)),
    unit("mV", Quantity::Voltage, (1, 1_000)),
    unit("Pa", Quantity::Pressure, (1, 1)),
    unit("kPa", Quantity::Pressure, (1_000, 1)),
    unit("MPa", Quantity::Pressure, (1_000_000, 1)),
    unit("bar", Quantity::Pressure, (100_000, 1)),
    unit("m/s^2", Quantity::Acceleration, (1, 1)),
    unit("cm/s^2", Quantity::Acceleration, (1, 100)),
    unit("km/h^2", Quantity::Acceleration, (1, 12_960)),
    unit("g/cm^3", Quantity::Density, (1, 1)),
    unit("kg/m^3", Quantity::Density, (1, 1_000)),
    unit("kg/L", Quantity::Concentration, (1_000_000, 1)),
    unit("kg/l", Quantity::Concentration, (1_000_000, 1)),
    unit("g/s", Quantity::MassRate, (1, 1)),
    unit("g/min", Quantity::MassRate, (1, 60)),
    unit("g/h", Quantity::MassRate, (1, 3_600)),
    unit("g/day", Quantity::MassRate, (1, 86_400)),
    unit("mg/s", Quantity::MassRate, (1, 1_000)),
    unit("mg/min", Quantity::MassRate, (1, 60_000)),
    unit("mg/h", Quantity::MassRate, (1, 3_600_000)),
    unit("kg/s", Quantity::MassRate, (1_000, 1)),
    unit("kg/min", Quantity::MassRate, (50, 3)),
    unit("kg/h", Quantity::MassRate, (5, 18)),
    unit("kg/day", Quantity::MassRate, (5, 432)),
    unit("$/kg", Quantity::DollarPerMass, (1, 1)),
    unit("dollar/kg", Quantity::DollarPerMass, (1, 1)),
    unit("dollars/kg", Quantity::DollarPerMass, (1, 1)),
    unit("ohm", Quantity::Resistance, (1, 1)),
    unit("ohms", Quantity::Resistance, (1, 1)),
    unit("Ω", Quantity::Resistance, (1, 1)),
    unit("L/km", Quantity::VolumePerLength, (1_000, 100_000)),
    unit("l/km", Quantity::VolumePerLength, (1_000, 100_000)),
];

/// Build one table entry.
const fn unit(spelling: &'static str, quantity: Quantity, scale: (u64, u64)) -> Unit {
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

    /// The additive offset from this unit to the canonical unit.
    #[must_use]
    pub fn offset(&self) -> BigRational {
        match self.spelling {
            // Bare "F" is not a registered spelling (`UNITS` holds only
            // "°F", "fahrenheit", and "Fahrenheit"): the parser reads a
            // lone "F" as an identifier, never as this unit, so that arm
            // was dead.
            "°F" | "F" | "fahrenheit" | "Fahrenheit" => {
                BigRational::new(BigInt::from(-160), BigInt::from(9))
            }
            "K" | "kelvin" | "Kelvin" => BigRational::new(BigInt::from(-5_463), BigInt::from(20)),
            _ => BigRational::from_integer(BigInt::from(0)),
        }
    }
}

/// The answer without one trailing unit spelling: `2√3 m` and `2√3m` give
/// `2√3`. The longest spelling wins (`cm` before `m`), and the unit must follow
/// a space, a digit or a closing bracket, so `2√3` and `x` stay whole.
#[must_use]
pub fn magnitude(text: &str) -> &str {
    let trimmed = text.trim();
    let mut best: Option<&str> = None;
    for unit in UNITS {
        let Some(prefix) = trimmed.strip_suffix(unit.spelling) else {
            continue;
        };
        let joined = prefix.chars().next_back().is_some_and(|c| {
            c.is_whitespace() || c.is_ascii_digit() || matches!(c, ')' | '}' | ']')
        });
        if joined && best.is_none_or(|kept| prefix.len() < kept.len()) {
            best = Some(prefix);
        }
    }
    best.map_or(trimmed, str::trim)
}

/// Look up a unit by the spelling the answer writes.
#[must_use]
pub fn lookup(spelling: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|unit| unit.spelling == spelling)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bare "F" is not in `UNITS`: `lookup` misses it, and the parser's
    /// `temperature_unit` only ever builds "°C" or "°F" from a one-letter
    /// name, so no looked-up `Unit` ever carries the spelling "F".
    #[test]
    fn bare_f_reads_as_fahrenheit_with_the_offset() {
        let fahrenheit = BigRational::new(BigInt::from(-160), BigInt::from(9));
        assert_eq!(lookup("F").unwrap().offset(), fahrenheit);
    }

    /// Every spelling of Fahrenheit in `UNITS` carries the same offset, and a
    /// unit with no offset (Celsius, the canonical temperature spelling)
    /// stays at zero.
    #[test]
    fn offset_covers_every_registered_fahrenheit_spelling() {
        let fahrenheit = BigRational::new(BigInt::from(-160), BigInt::from(9));
        for spelling in ["°F", "fahrenheit", "Fahrenheit"] {
            assert_eq!(lookup(spelling).unwrap().offset(), fahrenheit, "{spelling}");
        }
        for spelling in ["°C", "celsius", "Celsius"] {
            assert_eq!(
                lookup(spelling).unwrap().offset(),
                BigRational::from_integer(BigInt::from(0)),
                "{spelling}"
            );
        }
    }
}
