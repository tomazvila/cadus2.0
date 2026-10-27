//! The optional parts of a teach page: `terms`, `picture`, `watch_out` and `second_example`.
//!
//! D-TM1 and D-TD1. A reply carries each key only when the approved body holds
//! it, so a page authored before the fields existed sends none of them.

use serde_json::{Value, json};

use super::TeachDoc;

/// Add the optional parts of `page` to `reply`.
///
/// Both teach routes call this one helper, so the two replies cannot drift.
pub(crate) fn add_page_extras(reply: &mut Value, page: &TeachDoc) {
    if let Some(picture) = &page.picture {
        reply["picture"] = json!(picture);
    }
    if let Some(watch_out) = &page.watch_out {
        reply["watch_out"] = json!(watch_out);
    }
    if let Some(second) = &page.second_example {
        reply["second_example"] = json!({"problem": second.problem, "steps": second.steps});
    }
    if !page.terms.is_empty() {
        reply["terms"] = json!(page.terms);
    }
}
