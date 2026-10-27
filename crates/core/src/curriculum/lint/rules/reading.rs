//! The reading-list rules of the full lint.
//!
//! - `reading_entry`: one entry has an empty field, a wrong `access` value, a
//!   bad `where` value or a note that is too long.
//! - `reading_too_long`: a course lists more than six entries.

use super::super::super::finding::Finding;
use super::super::super::model::{Catalog, ReadingEntry};
use super::super::Lint;

/// The most entries one course lists.
const MAX_ENTRIES: usize = 6;
/// The most characters in one note.
const MAX_NOTE: usize = 159;

pub(in crate::curriculum::lint) fn check_reading(lint: &mut Lint<'_>, catalog: &Catalog) {
    for course in &catalog.courses {
        let id = course.id.as_str();
        for (at, entry) in course.reading.iter().enumerate() {
            if let Some(problem) = first_problem(entry) {
                lint.findings.push(Finding::new(
                    "reading_entry",
                    format!("course {id}: reading entry {}: {problem}", at + 1),
                ));
            }
        }
        if course.reading.len() > MAX_ENTRIES {
            lint.findings.push(Finding::new(
                "reading_too_long",
                format!(
                    "course {id}: reading list has {} entries; the limit is {MAX_ENTRIES}",
                    course.reading.len()
                ),
            ));
        }
    }
}

fn first_problem(entry: &ReadingEntry) -> Option<String> {
    for (name, text) in [
        ("title", &entry.title),
        ("author", &entry.author),
        ("note", &entry.note),
    ] {
        if text.trim().is_empty() {
            return Some(format!("{name} is empty"));
        }
    }
    if entry.access != "free" && entry.access != "book" {
        return Some("access must be free or book".to_owned());
    }
    let place = &entry.where_to_read;
    if place.trim().is_empty() {
        return Some("where is empty".to_owned());
    }
    let lower = place.to_lowercase();
    // Only text that starts with an address scheme is an address. Any other
    // text is a chapter note.
    if (lower.starts_with("http://") || lower.starts_with("https://")) && !is_https_address(place) {
        return Some("where must be an https address or a chapter note".to_owned());
    }
    if entry.note.chars().count() > MAX_NOTE {
        return Some("note is longer than 159 characters".to_owned());
    }
    None
}

/// An address that starts with `https://`, has a host, and holds no whitespace.
fn is_https_address(place: &str) -> bool {
    place.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty() && !rest.starts_with('/') && !place.contains(char::is_whitespace)
    })
}
