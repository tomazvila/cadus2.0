//! Readers of a parse result, as the literal lists the loader tests compare.

use cadus_core::curriculum::Parsed;

/// The findings of a parse result, as `(code, message, file)` triples.
#[must_use]
pub fn triples(parsed: &Parsed) -> Vec<(&str, &str, Option<&str>)> {
    parsed
        .findings
        .iter()
        .map(|f| (f.code.as_str(), f.message.as_str(), f.file.as_deref()))
        .collect()
}

/// The unit file names of a parse result, in load order.
#[must_use]
pub fn unit_names(parsed: &Parsed) -> Vec<&str> {
    parsed.units.iter().map(|u| u.file_name.as_str()).collect()
}

/// The id of the first topic of every unit file, in load order.
#[must_use]
pub fn first_topic_ids(parsed: &Parsed) -> Vec<&str> {
    parsed
        .units
        .iter()
        .map(|u| u.unit.topics[0].id.as_str())
        .collect()
}
