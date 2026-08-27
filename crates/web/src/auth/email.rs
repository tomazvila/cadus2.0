//! Email normalization: NFKC, then strip, then lower.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1, row "Email
//! normalization" (1.0 `passwords.py:148-156`).
//!
//! The normalized address is the unique key of the `users` table and the key of
//! the per-email rate counter (section 3.2), so two spellings of one address
//! must give one string. The three steps run in this order for a reason:
//!
//! 1. NFKC folds the compatibility variants. A full-width `ｅ` (U+FF45) becomes
//!    `e`, and a no-break space (U+00A0) becomes a plain space.
//! 2. `trim` then drops the surrounding whitespace, the spaces that step 1 just
//!    made included.
//! 3. `to_lowercase` case-folds what is left.
//!
//! The answer is idempotent: a second pass over the answer gives the same
//! string. A key that is not idempotent lets one address own two rows.

use unicode_normalization::UnicodeNormalization;

/// The storage and lookup form of an email address.
///
/// The function normalizes the WHOLE address, the domain and the local part
/// alike. 1.0 does the same, and the `users.email` unique index holds this form.
pub fn normalize_email(email: &str) -> String {
    email.nfkc().collect::<String>().trim().to_lowercase()
}
