//! U1 acceptance, part 4: the numeric literal forms 2.0 refuses, the merge
//! key, and the scalar forms 1.0 coerces (spec section 7, "2.0 strictness").
//!
//! Every message below is a literal from `docs/reference/curriculum-1.0-spec.md`
//! section 7. No expected value comes from the code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::curriculum::parse_curriculum;
use common::parsed::triples;
use common::paths::fixture;

// --------------------------------------------------------------------------- //
// The numeric literal forms 2.0 refuses
// --------------------------------------------------------------------------- //

#[test]
fn every_rejected_numeric_literal_form_names_the_form_and_the_file() {
    // Review findings 3, 4, 5, 8, 10 and 14. 2.0 accepts a plain decimal integer
    // and a plain decimal float in `order`, `difficulty`, `expected_time_secs`
    // and `weight`, and refuses every other spelling (spec section 7, "2.0
    // strictness"). Each unit file below writes one refused spelling.
    //
    // 1.0 reads the tree the other way: it loads `08` as 8, `060` as 48, `0b101`
    // as 5, `0x1F` as 31, `1_000` as 1000, `1:30` as 90 and `0.7_5` as 0.75, and
    // it drops the three files that write `1e3`, `1.0e2` and `0o17` with
    // `Input should be a valid integer, unable to parse string as an integer`.
    // The two loaders therefore disagree on every file of this tree, which is
    // why 2.0 names the form and the fix in its own words.
    let parsed = parse_curriculum(&fixture("loader-numeric-forms"));
    let rejected = |file: &'static str, line: u32, raw: &str| {
        (
            "schema".to_owned(),
            format!(
                "c/{file}:{line}: numeric literal form '{raw}' is not accepted; \
                 write a plain decimal number"
            ),
            Some(format!("c/{file}")),
        )
    };
    let found: Vec<(String, String, Option<String>)> = parsed
        .findings
        .iter()
        .map(|f| (f.code.clone(), f.message.clone(), f.file.clone()))
        .collect();
    assert_eq!(
        found,
        vec![
            rejected("00-leading-zero-08.yaml", 9, "08"),
            rejected("01-leading-zero-060.yaml", 9, "060"),
            rejected("02-exponent-1e3.yaml", 9, "1e3"),
            rejected("03-exponent-1-0e2.yaml", 9, "1.0e2"),
            rejected("04-octal-0o17.yaml", 9, "0o17"),
            rejected("05-binary-0b101.yaml", 9, "0b101"),
            rejected("06-hex-0x1F.yaml", 9, "0x1F"),
            rejected("07-underscore-1_000.yaml", 9, "1_000"),
            rejected("08-underscore-float-0-7_5.yaml", 7, "0.7_5"),
            rejected("09-sexagesimal-1-30.yaml", 9, "1:30"),
            rejected("10-nan.yaml", 7, ".nan"),
            rejected("11-inf.yaml", 7, ".inf"),
            // The flow form `{id: a, weight: 0.7_5, key: false}`, with a comment
            // behind the closing brace.
            rejected("12-flow-weight.yaml", 16, "0.7_5"),
        ]
    );
    assert!(parsed.units.is_empty(), "every file is dropped");
    assert!(parsed.findings.iter().all(|finding| finding.fatal));
}

#[test]
fn a_rejected_numeric_literal_in_the_catalog_drops_the_catalog() {
    // The same rule on the `order` key of `courses.yaml`. 1.0 drops the whole
    // tree here too, with `courses.0.order: Input should be a valid integer,
    // unable to parse string as an integer`.
    let parsed = parse_curriculum(&fixture("loader-numeric-forms-order"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "schema",
            "courses.yaml:4: numeric literal form '1e3' is not accepted; write a plain decimal \
             number",
            Some("courses.yaml"),
        )]
    );
    assert!(parsed.catalog.is_none(), "the catalog is dropped");
    assert!(parsed.units.is_empty());
}

#[test]
fn the_accepted_numeric_forms_load_with_the_1_0_values() {
    // The other half of the rule: a plain decimal integer, a plain decimal float
    // and a plain decimal float with a signed exponent. A 1.0 load of this
    // fixture reads `difficulty` 0.15 and 0.75, `expected_time_secs` 60 and
    // 1200, `weight` 0.0 and `order` 2, and writes no parse-stage finding.
    let parsed = parse_curriculum(&fixture("loader-numeric-accepted"));
    assert_eq!(triples(&parsed), Vec::new());
    let catalog = parsed.catalog.clone().expect("courses.yaml loads");
    assert_eq!(catalog.courses[0].order, 2);
    let topics = &parsed.units[0].unit.topics;
    assert_eq!(topics[0].difficulty, 0.15);
    assert_eq!(topics[0].expected_time_secs, 60);
    assert_eq!(topics[1].difficulty, 0.75);
    assert_eq!(topics[1].expected_time_secs, 1200);
    assert_eq!(topics[1].prerequisites[0].weight, 0.0);
}

#[test]
fn a_numeric_value_on_a_continuation_line_is_not_scanned() {
    // The documented limit of the pre-scan (spec section 7, "2.0 strictness").
    // The fixture writes `difficulty:` on one line and `0.7_5` on the next, so
    // the line scan sees no value and the type check reports the scalar the
    // YAML 1.2 parser made of it. 1.0 reads the value 0.75 and loads the file.
    let parsed = parse_curriculum(&fixture("loader-numeric-continuation"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "schema",
            "topics.0.difficulty: Input should be a valid number, unable to parse string as a \
             number",
            Some("c/00.yaml"),
        )]
    );
}

#[test]
fn an_integer_past_the_unsigned_range_is_the_same_schema_finding() {
    // Review finding 21. `18446744073709551616` fits no `serde_norway` number,
    // so the parser refuses the whole document. The port reports the same code,
    // the same dotted location and the same message as the literal one step
    // below the range (the test above), and not the parser's own text.
    let parsed = parse_curriculum(&fixture("loader-huge-integer"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "schema",
            "topics.0.expected_time_secs: integer literal outside the 64-bit range",
            Some("c/00.yaml"),
        )]
    );
}

#[test]
fn a_merge_key_is_one_schema_finding() {
    // Review finding 19. PyYAML flattens `<<` into the mapping and 1.0 loads
    // both topics. 2.0 refuses the form, and reports it once: the merged fields
    // are absent only because of the merge key, so a "Field required" finding
    // for each of them names a phantom defect.
    let parsed = parse_curriculum(&fixture("loader-merge-key"));
    assert_eq!(
        triples(&parsed),
        vec![(
            "schema",
            "topics.1.<<: merge keys are not accepted; write the fields out",
            Some("c/00.yaml"),
        )]
    );
}

#[test]
fn the_scalar_forms_1_0_coerces_carry_the_2_0_message() {
    // Review findings 16 and 27. 1.0 validates in pydantic lax mode, so it reads
    // `"0.3"` as 0.3, `"30"` as 30, `1` as true, `"true"` as true, and `true` as
    // 1.0 in a float field. 2.0 refuses each one, and refuses it in its own
    // words: pydantic's wording is reserved for the values pydantic also
    // refuses. The two messages for `topics.4.core` and `topics.5.core` below
    // are the 1.0 output for this fixture; 1.0 reports nothing for the other
    // five topics.
    let parsed = parse_curriculum(&fixture("loader-strict-scalars"));
    assert_eq!(
        triples(&parsed),
        vec![
            (
                "schema",
                "topics.0.difficulty: string '0.3' is not accepted; write the number unquoted",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.1.expected_time_secs: string '30' is not accepted; write 30",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.2.core: number 1 is not accepted; write true or false",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.3.core: string 'true' is not accepted; write true or false",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.4.core: Input should be a valid boolean, unable to interpret input",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.5.core: Input should be a valid boolean",
                Some("c/00.yaml"),
            ),
            (
                "schema",
                "topics.6.difficulty: boolean true is not accepted; write a number",
                Some("c/00.yaml"),
            ),
        ]
    );
}
