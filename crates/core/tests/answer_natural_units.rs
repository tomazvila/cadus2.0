//! Units, money, and thousands commas in learner answers. Every accepted row
//! has a wrong row beside it.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

fn verdict(key: &str, json: &str, learner: &str) -> Option<bool> {
    let contract: AnswerContract = serde_json::from_str(json).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(verdict) => Some(verdict.correct),
        Outcome::Undecidable(_) => None,
    }
}

fn rows(key: &str, json: &str, right: &[&str], wrong: &[&str]) {
    for learner in right {
        assert_eq!(verdict(key, json, learner), Some(true), "{key} / {learner}");
    }
    for learner in wrong {
        assert_ne!(verdict(key, json, learner), Some(true), "{key} / {learner}");
    }
}

fn unit(quantity: &str, unit: &str) -> String {
    format!(r#"{{"kind":"unit","quantity":"{quantity}","unit":"{unit}","allow_omitted":true}}"#)
}

#[test]
fn thousands_commas_read_with_a_unit() {
    rows(
        "4743 km",
        &unit("length", "km"),
        &["4,743 km", "4,743", "4743 km"],
        &["4,744 km", "4,74 km"],
    );
    rows(
        "2574000 m^3",
        &unit("cubic_volume", "m^3"),
        &["2,574,000 m^3", "2,574,000"],
        &["2,574,001 m^3"],
    );
}

#[test]
fn money_reads_with_commas_signs_and_words() {
    let dollar = unit("dollar", "dollar");
    rows(
        "1710",
        &dollar,
        &["$1,710", "1,710 dollars", "$1710", "1710 dollars"],
        &["$1,711", "$171"],
    );
    rows(
        "-20",
        &dollar,
        &["-$20", "-20 dollars", "$-20", "minus $20"],
        &["$20", "-$21"],
    );
    rows("-3.65", &dollar, &["-$3.65"], &["-$3.56"]);
    rows("12700", &dollar, &["$12,700", "$12700"], &["$1,270"]);
    rows("1800", &dollar, &["$1800", "$1,800"], &["$180"]);
    rows("210000", &dollar, &["$210,000"], &["$21,000"]);
    rows("-40", &dollar, &["-$40", "-40 dollars"], &["$40"]);
    let euro = unit("euro", "euro");
    rows("-1.5", &euro, &["-€1.50", "-1.5 euros"], &["€1.5"]);
    rows("12000", &euro, &["€12,000"], &["€1,200"]);
}

#[test]
fn minus_in_words_reads_as_a_sign() {
    rows(
        "-11",
        r#"{"kind":"exact"}"#,
        &["minus 11", "-11"],
        &["11", "minus 12"],
    );
}

#[test]
fn a_squared_unit_after_a_radical_reads() {
    let area = r#"{"kind":"unit","quantity":"area","unit":"cm^2","allow_omitted":true,"form":"simplest_radical"}"#;
    rows(
        "16*sqrt(3) cm^2",
        area,
        &["16√3 cm²", "16√3 cm^2", "16 sqrt(3) cm2", "16√3"],
        &["√768 cm²", "17√3 cm²"],
    );
}

#[test]
fn spoken_compound_units_read() {
    rows(
        "18 km/h",
        &unit("speed", "km/h"),
        &[
            "18 km per hour",
            "18 kilometres per hour",
            "18 kilometers per hour",
            "18",
        ],
        &["19 km per hour", "18 m per hour"],
    );
    rows(
        "22.5 L/min",
        &unit("flow", "L/min"),
        &["22.5 litres per minute", "22.5 liters per minute"],
        &["22 litres per minute"],
    );
    rows(
        "20 m/s",
        &unit("speed", "m/s"),
        &["20 metres per second", "20 meters per second"],
        &["21 metres per second"],
    );
    rows(
        "15 mg/L",
        &unit("concentration", "mg/L"),
        &["15 mg per litre"],
        &["16 mg per litre"],
    );
    rows(
        "8 euro/kg",
        &unit("euro_per_mass", "euro/kg"),
        &["8 euros per kg"],
        &["9 euros per kg"],
    );
    rows(
        "175 m/min",
        &unit("speed", "m/min"),
        &["175 m/min"],
        &["176 m/min"],
    );
    rows(
        "10.5 km/h",
        &unit("speed", "km/h"),
        &["175 m/min"],
        &["176 m/min"],
    );
}

#[test]
fn degrees_after_a_temperature_is_the_number() {
    let celsius = unit("temperature", "°C");
    rows(
        "-15 °C",
        &celsius,
        &["-15 degrees", "-15°", "-15 °C", "-15"],
        &["-14 degrees", "15 degrees"],
    );
    rows(
        "50 °F",
        &unit("temperature", "°F"),
        &["50 F", "50 degrees", "50°F"],
        &["51 F"],
    );
}

#[test]
fn new_units_read() {
    rows(
        "3 year",
        &unit("time", "year"),
        &["3 years", "36 months"],
        &["4 years"],
    );
    rows(
        "4 week",
        &unit("time", "week"),
        &["4 weeks", "28 days"],
        &["5 weeks"],
    );
    rows(
        "2 tonne",
        &unit("mass", "tonne"),
        &["2 tonnes", "2000 kg", "2,000 kg"],
        &["3 tonnes"],
    );
    rows(
        "48 J",
        &unit("energy", "J"),
        &["48 J", "0.048 kJ", "48 joules"],
        &["49 J"],
    );
    rows(
        "452 mL",
        &unit("volume", "mL"),
        &["452 ml", "452 mL"],
        &["453 mL"],
    );
    rows(
        "5 micrometre",
        &unit("length", "micrometre"),
        &["5 micrometres", "0.0005 cm"],
        &["6 micrometres"],
    );
    rows(
        "0.5 mol/L",
        &unit("molarity", "mol/L"),
        &["0.5 mol/L", "500 mmol/L"],
        &["0.6 mol/L"],
    );
}

#[test]
fn a_time_in_two_units_reads_as_one_number() {
    let hours = unit("time", "h");
    rows(
        "4.5 h",
        &hours,
        &["4 hours 30 minutes", "270 minutes", "4.5 h", "9/2"],
        &["4 hours 31 minutes", "4 hours 3 minutes"],
    );
    rows(
        "1.25 h",
        &hours,
        &["1 hour 15 minutes"],
        &["1 hour 20 minutes"],
    );
    rows(
        "475 min",
        &unit("time", "min"),
        &["7 hours 55 minutes", "475 min"],
        &["7 hours 5 minutes"],
    );
}

#[test]
fn a_slope_is_a_value_and_exact_takes_every_spelling() {
    let exact = r#"{"kind":"exact"}"#;
    rows(
        "-3/2",
        exact,
        &["-1.5", "-6/4", "-3/2", "-1 1/2"],
        &["-1.4", "3/2", "-2/3"],
    );
}
