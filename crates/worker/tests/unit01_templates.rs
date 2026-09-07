//! Unit01 drafts pass the production gate, exhaust the domain, and stay independent.
#![allow(clippy::unwrap_used)]
mod common;

crate::reviewed_template_tests!(
    "docs/content-foundations/fractions-decimals/templates",
    79,
    "target/unit01/regression",
    948,
    "fraction-of-a-number/kp1"
);
