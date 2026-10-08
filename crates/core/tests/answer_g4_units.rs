//! Grader pass 4, the unit families: units that were missing from the table,
//! spoken and Unicode spellings, and prices in two currencies. Every accepted
//! row has a wrong row beside it.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

fn verdict(key: &str, quantity: &str, unit: &str, learner: &str) -> Option<bool> {
    let json = format!(r#"{{"kind":"unit","quantity":"{quantity}","unit":"{unit}"}}"#);
    let contract: AnswerContract = serde_json::from_str(&json).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(verdict) => Some(verdict.correct),
        Outcome::Undecidable(_) => None,
    }
}

/// One contract, the learner answers that must be right and those that must
/// not be right.
fn rows(key: &str, quantity: &str, unit: &str, right: &[&str], wrong: &[&str]) {
    for learner in right {
        assert_eq!(
            verdict(key, quantity, unit, learner),
            Some(true),
            "{key} / {learner}"
        );
    }
    for learner in wrong {
        assert_ne!(
            verdict(key, quantity, unit, learner),
            Some(true),
            "{key} / {learner}"
        );
    }
}

#[test]
fn mass_in_tonnes_with_the_letter_t() {
    rows("3 t", "mass", "t", &["3 t", "3000 kg"], &["3 kg", "4 t"]);
}

#[test]
fn kelvin_reads_with_its_offset() {
    rows(
        "300 K",
        "temperature",
        "K",
        &["300 K", "26.85 °C", "26.85 C"],
        &["300 °C", "273 K"],
    );
    rows("0 °C", "temperature", "°C", &["273.15 K"], &["273 K"]);
}

#[test]
fn a_bare_c_is_celsius_under_a_temperature_contract() {
    rows("36 °C", "temperature", "°C", &["36 C", "36 °C"], &["37 C"]);
    rows("25 °C", "temperature", "°C", &["25 C"], &["25 F"]);
}

#[test]
fn micrometres_in_every_spelling() {
    rows(
        "5 um",
        "length",
        "um",
        &["5 micrometres", "5 µm", "5 μm", "0.005 mm"],
        &["5 mm", "6 um"],
    );
}

#[test]
fn lengths_areas_and_volumes_added_to_the_table() {
    rows("3 dm", "length", "dm", &["30 cm"], &["30 mm", "3 cm"]);
    rows("3 km^2", "area", "km^2", &["3 km^2"], &["3 m^2"]);
    rows("500 cm^2", "area", "cm^2", &["5 dm^2"], &["5 cm^2"]);
    rows("2 cup", "volume", "cup", &["2 cups"], &["3 cups"]);
    rows("5 gallon", "volume", "gallon", &["5 gallon"], &["5 L"]);
    rows(
        "5 megaliter",
        "volume",
        "megaliter",
        &["5 megaliter"],
        &["5 L"],
    );
}

#[test]
fn speeds_and_rates_in_spoken_and_short_spellings() {
    rows(
        "80 km/h",
        "speed",
        "km/h",
        &["80 kph", "80 km per hour", "80 km/hour"],
        &["80 mph", "8 kph"],
    );
    rows(
        "4 km/day",
        "speed",
        "km/day",
        &["4 km per day", "4 km/day"],
        &["4 km per hour"],
    );
    rows(
        "5 cm/s",
        "speed",
        "cm/s",
        &["5 cm/s", "0.05 m/s"],
        &["5 m/s"],
    );
    rows("5 m^3/s", "flow", "m^3/s", &["5 m^3/s"], &["5 L/s"]);
    rows(
        "-100 g/day",
        "mass_rate",
        "g/day",
        &["-100 g/day"],
        &["-100 g/h"],
    );
}

#[test]
fn degrees_in_the_short_spelling() {
    rows("30°", "angle", "°", &["30 deg", "30 degrees"], &["31 deg"]);
    rows("90°", "angle", "°", &["90 deg"], &["9 deg"]);
}

#[test]
fn physics_units_have_their_own_quantities() {
    rows(
        "5 kW",
        "power",
        "kW",
        &["5 kW", "5000 W", "5 kilowatts"],
        &["5 W"],
    );
    rows("5 watt", "power", "watt", &["5 W", "5 watt"], &["5 kW"]);
    rows(
        "588000 newton",
        "force",
        "newton",
        &["588000 N"],
        &["588 N"],
    );
    rows("5 newton", "force", "newton", &["5 N"], &["5 kN"]);
    rows("5 volt", "voltage", "volt", &["5 V", "5000 mV"], &["5 kV"]);
    rows("2 kPa", "pressure", "kPa", &["2000 Pa"], &["2 Pa"]);
    rows("2 m/s^2", "acceleration", "m/s^2", &["2 m/s^2"], &["2 m/s"]);
    rows(
        "7.8 g/cm^3",
        "density",
        "g/cm^3",
        &["7.8 g/cm^3", "7800 kg/m^3"],
        &["7.8 kg/m^3"],
    );
    rows(
        "2 kg/L",
        "concentration",
        "kg/L",
        &["2 kg/L", "2000 g/L"],
        &["2 g/L"],
    );
    rows("300 N/m", "force_constant", "N/m", &["300 N/m"], &["300 N"]);
    rows("2 Ω", "resistance", "Ω", &["2 Ω", "2 ohms"], &["3 Ω"]);
}

#[test]
fn a_dollar_price_per_mass_is_not_a_euro_price() {
    rows(
        "2.8 $/kg",
        "dollar_per_mass",
        "$/kg",
        &["$2.8 per kg", "$2.8/kg", "2.8 dollars/kg"],
        &["€2.8 per kg", "$2.9 per kg"],
    );
    rows(
        "2.8 euro/kg",
        "euro_per_mass",
        "euro/kg",
        &["€2.8 per kg", "2.8 €/kg"],
        &["$2.8 per kg"],
    );
}

#[test]
fn an_exponential_magnitude_reads_with_its_unit() {
    rows(
        "e^4 - 1 m",
        "length",
        "m",
        &["e^4 - 1 m", "(e^4-1)*100 cm"],
        &["e^4 m", "53 m"],
    );
}

#[test]
fn single_letter_units_stay_variables_outside_a_unit_contract() {
    let contract = r#"{"kind":"function","vars":["t"]}"#;
    let contract: AnswerContract = serde_json::from_str(contract).unwrap();
    assert!(matches!(
        check_contract("3*t", "3 t", contract),
        Outcome::Decided(v) if v.correct
    ));
}

#[test]
fn the_word_degrees_after_a_number_is_the_degree_sign() {
    rows(
        "180°",
        "angle",
        "°",
        &["180 degrees", "180 degree", "180 deg", "180°"],
        &["90 degrees", "180 radians"],
    );
}
