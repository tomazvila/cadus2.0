//! The checks of the program fields (D-VBE1): exam weights, row numbers and reading entries.

use serde_norway::Value;

use super::Checker;
use super::integer_of;

impl Checker<'_> {
    /// The four area weights are whole percents and the sum is 100 (D-VBE1).
    pub(super) fn check_exam_weights(&mut self, value: &Value) {
        const FIELDS: [&str; 4] = ["numbers", "models", "geometry", "data"];
        let Some(map) = self.struct_map(value, "ExamWeights") else {
            return;
        };
        let mut sum: i64 = 0;
        let mut complete = true;
        for name in FIELDS {
            self.field(map, name, true, Self::check_percent);
            match map.get(name).map(integer_of) {
                Some(Ok(number)) if (0..=100).contains(&number) => sum += number,
                _ => complete = false,
            }
        }
        if complete && sum != 100 {
            self.report(&format!(
                "The four area weights must add up to 100, not {sum}"
            ));
        }
        self.extras(map, &FIELDS);
    }

    /// A whole number from 0 to 100.
    fn check_percent(&mut self, value: &Value) {
        match integer_of(value) {
            Err(message) => self.report(&message),
            Ok(number) if number < 0 => {
                self.report_range("Input should be greater than or equal to 0");
            }
            Ok(number) if number > 100 => {
                self.report_range("Input should be less than or equal to 100");
            }
            Ok(_) => {}
        }
    }

    /// A row number of the program registry: a whole number above 0.
    pub(super) fn check_program_row(&mut self, value: &Value) {
        match integer_of(value) {
            Err(message) => self.report(&message),
            Ok(number) if number <= 0 => self.report_range("Input should be greater than 0"),
            Ok(number) if number > i64::from(u32::MAX) => {
                self.report_range("Input should be less than or equal to 4294967295");
            }
            Ok(_) => {}
        }
    }

    pub(super) fn check_reading_entry(&mut self, value: &Value) {
        const FIELDS: [&str; 5] = ["title", "author", "access", "where", "note"];
        let Some(map) = self.struct_map(value, "ReadingEntry") else {
            return;
        };
        for name in FIELDS {
            self.field(map, name, true, Self::check_string);
        }
        self.extras(map, &FIELDS);
    }
}
