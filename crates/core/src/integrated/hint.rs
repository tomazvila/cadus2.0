//! The hint ladder of an integrated task, served one rung at a time.
//!
//! D-F10 asks for progressive hints that fade before independent assessment.
//! The rungs stay on the server: the view tells the client HOW MANY rungs a
//! field has, and this module hands over one rung when the learner asks for it.
//! The count of opened rungs is what marks a field assisted at grade time.

use super::grade::FINAL_FIELD_ID;
use super::model::{Field, IntegratedItem};

/// The field of `item` named by `field_id`, or `None` for an unknown id.
fn field_of<'item>(item: &'item IntegratedItem, field_id: &str) -> Option<&'item Field> {
    if field_id == FINAL_FIELD_ID {
        return Some(&item.final_answer.ask);
    }
    item.step(field_id).map(|step| &step.ask)
}

/// The `index`-th hint of one field, counted from zero.
///
/// The ladder runs from the widest hint to the narrowest, so a learner who asks
/// again gets a narrower hint and never the answer: the answer is not a rung.
#[must_use]
pub fn hint<'item>(
    item: &'item IntegratedItem,
    field_id: &str,
    index: usize,
) -> Option<&'item str> {
    let hints = &field_of(item, field_id)?.hints;
    hints.get(index).map(String::as_str)
}

/// How many hints one field offers.
#[must_use]
pub fn hints_available(item: &IntegratedItem, field_id: &str) -> usize {
    field_of(item, field_id).map_or(0, |field| field.hints.len())
}
