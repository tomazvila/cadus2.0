//! The value-with-unit production of D-F3 (unit f2-grammar, V1, C4).
//!
//! A number followed by one unit token of the Foundations table reads into a
//! quantity. The same unit on both sides compares the values; a different unit
//! of the same kind converts through the table; a unit of another kind is
//! incorrect; a unit on one side alone is undecidable. The parse tests pin the
//! tree, the canon tests pin the form, and the check tests pin the verdicts on
//! both answer kinds.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::answer::Quantity;
use common::grammar::*;

/// Build a quantity node over a whole number.
fn quantity(value: Ast, unit: &'static str) -> Ast {
    Ast::Quantity {
        value: Box::new(value),
        unit,
    }
}

/// Build the canonical quantity of a rational count of base units.
fn measured(kind: Quantity, value: BigRational) -> Canon {
    Canon::Quantity {
        quantity: kind,
        value: Box::new(Canon::Rational(value)),
    }
}

// ---------------------------------------------------------------------------
// Parse
// ---------------------------------------------------------------------------
#[test]
fn a_number_and_one_unit_token_read_into_a_quantity() {
    assert_eq!(ast("5 cm"), quantity(int(5), "cm"));
    assert_eq!(ast("5cm"), quantity(int(5), "cm"));
    assert_eq!(ast("5 m"), quantity(int(5), "m"));
    assert_eq!(ast("16 m"), quantity(int(16), "m"));
    assert_eq!(ast("25 L"), quantity(int(25), "L"));
    assert_eq!(ast("2 l"), quantity(int(2), "l"));
    assert_eq!(ast("60 km/h"), quantity(int(60), "km/h"));
    assert_eq!(ast("60km/h"), quantity(int(60), "km/h"));
    assert_eq!(ast("5 m/s"), quantity(int(5), "m/s"));
    assert_eq!(ast("12 cm^2"), quantity(int(12), "cm^2"));
    assert_eq!(ast("12cm²"), quantity(int(12), "cm^2"));
    assert_eq!(ast("3 m^3"), quantity(int(3), "m^3"));
    assert_eq!(ast("0.54 m^3"), quantity(ast("0.54"), "m^3"));
    assert_eq!(ast("90°"), quantity(int(90), "°"));
    assert_eq!(ast("90 °"), quantity(int(90), "°"));
    assert_eq!(ast("5€"), quantity(int(5), "€"));
    assert_eq!(ast("5 $"), quantity(int(5), "$"));
    assert_eq!(ast("$5"), quantity(int(5), "$"));
    assert_eq!(ast("€5"), quantity(int(5), "€"));
    assert_eq!(ast("1.5 h"), quantity(ast("1.5"), "h"));
    assert_eq!(ast("90 min"), quantity(int(90), "min"));
    assert_eq!(ast("2 kg"), quantity(int(2), "kg"));
    assert_eq!(ast("250 ml"), quantity(int(250), "ml"));
    assert_eq!(ast("3 mm"), quantity(int(3), "mm"));
    assert_eq!(ast("4 s"), quantity(int(4), "s"));
    assert_eq!(ast("7 g"), quantity(int(7), "g"));
    // The value is one expression in front of the unit.
    assert_eq!(ast("-5 cm"), quantity(Ast::Neg(Box::new(int(5))), "cm"));
    assert_eq!(ast("1/2 kg"), quantity(ast("1/2"), "kg"));
    assert_eq!(ast("1 1/2 h"), quantity(ast("1 1/2"), "h"));
    assert_eq!(ast("2π cm^2"), quantity(ast("2π"), "cm^2"));
    assert_eq!(ast("2\\pi cm^2"), quantity(ast("2*pi"), "cm^2"));
    assert_eq!(ast("(5) cm"), quantity(int(5), "cm"));
    // A label in front of the quantity stays a label.
    assert_eq!(
        ast("d = 5 cm"),
        Ast::Assign {
            var: "d".to_string(),
            value: Box::new(quantity(int(5), "cm")),
        }
    );
}

#[test]
fn a_letter_outside_the_unit_position_keeps_its_variable_reading() {
    // A glued one-letter unit is the algebra product, as the corpus writes it.
    assert_eq!(ast("5m"), Ast::Mul(vec![int(5), v("m")]));
    assert_eq!(ast("3s"), Ast::Mul(vec![int(3), v("s")]));
    assert_eq!(ast("4h"), Ast::Mul(vec![int(4), v("h")]));
    assert_eq!(
        ast("5m^2"),
        Ast::Mul(vec![int(5), Ast::Pow(Box::new(v("m")), 2)])
    );
    // A unit inside a longer answer is no unit.
    assert_eq!(
        ast("2x + h"),
        Ast::Add(vec![Ast::Mul(vec![int(2), v("x")]), v("h")])
    );
    assert_eq!(ast("m"), v("m"));
    assert_eq!(ast("cm"), Ast::Mul(vec![v("c"), v("m")]));
    assert_eq!(
        ast("12 m/s^2"),
        Ast::Div(
            Box::new(Ast::Mul(vec![int(12), v("m")])),
            Box::new(Ast::Pow(Box::new(v("s")), 2))
        )
    );
    // A spelling outside the table is no unit: `5 M/S` and `5 mi`.
    assert_eq!(
        ast("5 M/S"),
        Ast::Div(Box::new(Ast::Mul(vec![int(5), v("M")])), Box::new(v("S")))
    );
    assert_eq!(refusal("5 mi"), "a name that is not a function or variable");
    // A head that is no number expression falls back to the ordinary read.
    assert_eq!(ast("x m"), Ast::Mul(vec![v("x"), v("m")]));
    assert_eq!(ast("2x cm"), ast("2x*cm"));
    assert_eq!(ast("5 cm cm"), ast("5*cm*cm"));
}

#[test]
fn a_unit_glyph_inside_an_expression_is_refused() {
    assert_eq!(refusal("5 € + 3 €"), "a unit inside an expression");
    assert_eq!(refusal("sin(30°)"), "a unit inside an expression");
    assert_eq!(refusal("cos 70°"), "a unit inside an expression");
    assert_eq!(refusal("(30°, 45°)"), "a unit inside an expression");
    assert_eq!(refusal("°"), "a unit inside an expression");
    assert_eq!(refusal("$"), "a unit inside an expression");
    assert_eq!(refusal("$5 cm"), "a unit inside an expression");
}

// ---------------------------------------------------------------------------
// Canon
// ---------------------------------------------------------------------------
#[test]
fn a_quantity_scales_into_the_base_unit_of_its_kind() {
    assert_eq!(form("5 cm"), measured(Quantity::Length, whole(5)));
    assert_eq!(form("1 m"), measured(Quantity::Length, whole(100)));
    assert_eq!(form("1 m"), form("100 cm"));
    assert_eq!(form("2 km"), form("2000 m"));
    assert_eq!(form("3 mm"), form("0.3 cm"));
    assert_eq!(form("1.5 h"), form("90 min"));
    assert_eq!(form("1.5 h"), measured(Quantity::Time, whole(5400)));
    assert_eq!(form("2 kg"), form("2000 g"));
    assert_eq!(form("2 l"), form("2000 ml"));
    assert_eq!(form("2 L"), form("2 l"));
    assert_eq!(form("36 km/h"), form("10 m/s"));
    assert_eq!(form("60 km/h"), measured(Quantity::Speed, ratio(50, 3)));
    assert_eq!(form("1 m^2"), form("10000 cm^2"));
    assert_eq!(form("1 m^3"), form("1000000 cm^3"));
    assert_eq!(form("90°"), measured(Quantity::Angle, whole(90)));
    assert_eq!(form("$5"), measured(Quantity::Dollar, whole(5)));
    assert_eq!(form("5 $"), form("$5"));
    assert_eq!(form("5€"), measured(Quantity::Euro, whole(5)));
    // A radical value stays exact.
    assert_eq!(
        form("sqrt(2) m"),
        Canon::Quantity {
            quantity: Quantity::Length,
            value: Box::new(radical(2, whole(100))),
        }
    );
    // Two kinds are two answers, and so are two values.
    assert_ne!(form("5 cm"), form("5 g"));
    assert_ne!(form("5 €"), form("5 $"));
    assert_ne!(form("5 cm"), form("6 cm"));
    assert_ne!(form("5 cm"), Canon::Rational(whole(5)));
}

#[test]
fn a_hand_built_quantity_outside_the_production_is_refused() {
    // The parser reads a number expression in front of a unit and nothing
    // else, so these trees come from a hand only.
    let symbolic = quantity(v("x"), "m");
    assert_eq!(
        canon(&symbolic).unwrap_err().reason,
        "a quantity whose value is not a number"
    );
    let tuple = quantity(Ast::Tuple(vec![int(1), int(2)]), "cm");
    assert_eq!(
        canon(&tuple).unwrap_err().reason,
        "a quantity whose value is not a number"
    );
    // A quantity carries no arithmetic; a hand-built tree that adds one is refused.
    let sum = Ast::Add(vec![quantity(int(5), "cm"), int(1)]);
    assert_eq!(canon(&sum).unwrap_err().reason, "arithmetic on a quantity");
    let outside = quantity(int(5), "mi");
    assert_eq!(
        canon(&outside).unwrap_err().reason,
        "a unit outside the table"
    );
}

// ---------------------------------------------------------------------------
// Check
// ---------------------------------------------------------------------------
#[test]
fn the_task_pairs_of_d_f3_are_decided_on_both_kinds() {
    run_table(&[
        ("1 m", "100 cm", N, true),
        ("100 cm", "1 m", N, true),
        ("1 m", "100 cm", E, true),
        ("1.5 h", "90 min", N, true),
        ("90 min", "1.5 h", N, true),
        ("1.5 h", "5400 s", N, true),
        ("5 cm", "5 cm", N, true),
        ("5 cm", "5cm", N, true),
        ("5 cm", "50 mm", N, true),
        ("5 cm", "0.05 m", N, true),
        ("60 km/h", "60 km/h", N, true),
        ("36 km/h", "10 m/s", N, true),
        ("2π cm^2", "2*pi cm^2", E, true),
        ("2π cm^2", "2π cm²", E, true),
        ("$5", "5 $", N, true),
        ("$5", "5$", N, true),
        ("90°", "90 °", N, true),
        ("16 m", "1600 cm", N, true),
        ("25 L", "25 l", N, true),
        ("25 L", "25000 ml", N, true),
        // A different value, or a different kind, is incorrect.
        ("1 m", "10 cm", N, false),
        ("1 m", "1 cm", N, false),
        ("5 cm", "5 g", N, false),
        ("5 cm", "5 s", N, false),
        ("5 €", "5 $", N, false),
        ("1.5 h", "1.5 min", N, false),
        ("60 km/h", "60 m/s", N, false),
        ("1 m^2", "1 m^3", N, false),
        ("1 m^2", "100 cm^2", N, false),
        ("90°", "45°", N, false),
    ]);
}

#[test]
fn a_unit_on_one_side_alone_gives_no_verdict() {
    assert_undecidable("5 cm", "5", N, "a unit is missing");
    assert_undecidable("1.5 h", "1.5", N, "a unit is missing");
    assert_undecidable("30°", "30", N, "a unit is missing");
    assert_undecidable("$5", "5", N, "a unit is missing");
    assert_undecidable("5 cm", "5x", E, "a unit is missing");
    assert_undecidable("d = 5 cm", "5", N, "a unit is missing");
    assert_undecidable("5", "5 cm", N, "a unit on the learner side only");
    assert_undecidable("30", "30°", N, "a unit on the learner side only");
    // A glued one-letter unit is the product, so `5m` for `5 m` has no unit.
    assert_undecidable("5 m", "5m", N, "a unit is missing");
    // A unit inside an expression leaves the grammar (V2).
    assert_undecidable("30°", "sin(30°)", N, "a unit inside an expression");
    // The string rung still decides an equal spelling first (rung 2).
    assert_eq!(check("5 cm", "5 CM", N), decided(true, false));
}

#[test]
fn a_rounded_decimal_reads_in_the_unit_the_learner_typed() {
    // 80 min is 4/3 h, and `1.33 h` is its rounding to two digits.
    assert_eq!(check("80 min", "1.33 h", N), rounded());
    assert_eq!(check("80 min", "1.3 h", N), rounded());
    assert_eq!(check("80 min", "1.34 h", N), decided(false, false));
    assert_eq!(check("80 min", "1.33 min", N), decided(false, false));
    assert_eq!(check("80 min", "1.33 kg", N), decided(false, false));
    assert_eq!(check("1 m", "100.0 cm", N), decided(true, false));
    assert_eq!(check("1/3 m", "33.33 cm", N), rounded());
    assert_eq!(check("sqrt(2) m", "141.42 cm", N), rounded());
    assert_eq!(check("sqrt(2) m", "141.43 cm", N), decided(false, false));
    assert_eq!(
        cadus_core::answer::notation_note("80 min", "1.33 h", N),
        Some(
            "Correct value. One note on form: the exact value is $80 min$; \
             $1.33 h$ is a rounding of it."
                .to_string()
        )
    );
}
