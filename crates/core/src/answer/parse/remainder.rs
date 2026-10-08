//! Whether a minus sign may follow the remainder marker.
//!
//! `3r-10` is the algebra `3r - 10`. Only an item whose contract asks for a
//! quotient and a remainder reads it as the quotient 3 and the remainder -10.

use std::cell::Cell;

thread_local! {
    static NEGATIVE_REMAINDER: Cell<bool> = const { Cell::new(false) };
}

/// Restores the reading that stood before [`with_negative_remainder`].
struct Restore(bool);

impl Drop for Restore {
    fn drop(&mut self) {
        NEGATIVE_REMAINDER.with(|flag| flag.set(self.0));
    }
}

/// Run `read` with `r` before a minus sign read as the remainder marker.
pub(crate) fn with_negative_remainder<T>(read: impl FnOnce() -> T) -> T {
    let _restore = Restore(NEGATIVE_REMAINDER.with(|flag| flag.replace(true)));
    read()
}

/// Whether a minus sign may follow the marker now.
pub(super) fn allowed() -> bool {
    NEGATIVE_REMAINDER.with(Cell::get)
}
