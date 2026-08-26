//! CPython-parity numeric and time helpers (spec section 7, traps T1-T4, T8, T9).
//!
//! The M3 fold reproduces 1.0's arithmetic bit for bit. CPython and Rust disagree
//! on three primitives, so the port routes every affected site through this module:
//!
//! - `sum()` over floats is Neumaier compensated summation in CPython 3.12 and
//!   later, while `Iterator::sum` is a naive loop. Use [`neumaier_sum`] at every
//!   1.0 `sum()` site. Where 1.0 uses `+=`, keep the naive loop (trap T2).
//! - `round(x)` is round-half-even in Python, while `f64::round` is
//!   half-away-from-zero. Use [`round_half_even`] and [`round_half_even_i64`].
//! - `round(x, n)` is correctly-rounded decimal in Python, not scale-round-divide.
//!   Use [`round_dp`].
//!
//! Instants are UTC microseconds since the Unix epoch, held in an `i64`. Time zones
//! enter only through [`local_day`]. The core reads no clock: `now` is always a
//! parameter.

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use chrono_tz::Tz;
use thiserror::Error;

use crate::curriculum::python_repr_f64;

/// Microseconds in one second. The divisor of the 1.0 `total_seconds()` first step.
const MICROSECONDS_PER_SECOND: f64 = 1_000_000.0;

/// Seconds in one day. The divisor of the 1.0 `days_since` second step.
const SECONDS_PER_DAY: f64 = 86_400.0;

/// A time error that the fold reports as a value. The core never panics.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TimeError {
    /// The time-zone name is absent from the compiled IANA database.
    #[error("unknown time zone `{0}`")]
    UnknownTimezone(String),
    /// The microsecond instant is outside the range `chrono` represents.
    #[error("timestamp {0} microseconds is outside the representable range")]
    TimestampOutOfRange(i64),
}

/// Sum `values` the way CPython's built-in `sum()` sums floats (trap T1).
///
/// CPython 3.12 and later apply the improved Kahan-Babuska algorithm of Neumaier:
/// the loop keeps a compensation term and adds it once at the end. `sum([0.1; 10])`
/// is therefore exactly `1.0`, while a naive loop gives `0.9999999999999999`.
///
/// Use this function at every site where 1.0 calls `sum()`. Do not use it where 1.0
/// accumulates with `+=`; that is a different algorithm and trap T2 keeps them apart.
#[must_use]
pub fn neumaier_sum(values: &[f64]) -> f64 {
    let mut total = 0.0_f64;
    let mut compensation = 0.0_f64;
    for &x in values {
        let t = total + x;
        if total.abs() >= x.abs() {
            compensation += (total - t) + x;
        } else {
            compensation += (x - t) + total;
        }
        total = t;
    }
    total + compensation
}

/// Round `x` to an integral value the way Python's one-argument `round()` does
/// (trap T3): half-away-from-zero first, then a half-way case corrected to even.
///
/// This is CPython's `float.__round__` with `ndigits` of `None`. `f64::round` alone
/// diverges: it gives `3.0` for `2.5` where Python gives `2`.
///
/// The result keeps the sign of a negative half-way case, so `round_half_even(-0.5)`
/// is `-0.0`. `-0.0` compares equal to `0.0`, and [`round_half_even_i64`] converts it
/// to `0`, which is the value Python's `int` result holds.
#[must_use]
pub fn round_half_even(x: f64) -> f64 {
    let rounded = x.round();
    if (x - rounded).abs() == 0.5 {
        2.0 * (x / 2.0).round()
    } else {
        rounded
    }
}

/// The lowest value an `i64` holds, as an `f64`. `-2**63` is exact in both types.
const I64_MIN_AS_FLOAT: f64 = -9_223_372_036_854_775_808.0;

/// `2**63`, the first value above the `i64` range. `i64::MAX` itself is not an
/// `f64`, so the test of the upper bound is a STRICT `<` against this number: the
/// largest `f64` the range holds is `9223372036854774784.0`, which is `2**63 - 1024`.
const I64_BOUND_AS_FLOAT: f64 = 9_223_372_036_854_775_808.0;

/// A rounded value that the `i64` range does not hold.
///
/// Python `int()` has unbounded precision, so 1.0 keeps the exact big integer where
/// this error stops the 2.0 fold. Spec section 7, trap T22, records the divergence.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("the rounded value {value} is outside the i64 range")]
pub struct OutOfRangeError {
    /// The rounded value, in the Python `repr` text of the port.
    pub value: String,
}

/// Round `x` to an integer the way Python's `int(round(x))` does (trap T3).
///
/// # Errors
///
/// Returns [`OutOfRangeError`] for `NaN`, for an infinity, and for a finite value
/// outside the `i64` range. Python `int(round(x))` returns an exact big integer for
/// the finite case and raises for the other two. The 2.0 fold reports the error at
/// all three, because the learner model holds an `i64`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the bounds test proves the value is inside the i64 range"
)]
pub fn round_half_even_i64(x: f64) -> Result<i64, OutOfRangeError> {
    let rounded = round_half_even(x);
    if (I64_MIN_AS_FLOAT..I64_BOUND_AS_FLOAT).contains(&rounded) {
        return Ok(rounded as i64);
    }
    Err(OutOfRangeError {
        value: python_repr_f64(rounded),
    })
}

/// [`round_half_even_i64`], saturating at the `i64` bounds, with `NaN` at `0`.
///
/// Use it ONLY where the input is bounded by construction and no learner number
/// reaches it: the two selector budgets, which round an authored count and a fixed
/// window. Every 1.0 `int(round(...))` over learner data uses
/// [`round_half_even_i64`] and reports the out-of-range value.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "the saturating cast is the documented rule of the bounded call sites"
)]
pub fn round_half_even_i64_saturating(x: f64) -> i64 {
    round_half_even(x) as i64
}

/// Round `x` to `digits` decimal places the way Python's `round(x, n)` does (trap T4).
///
/// Python formats the exact binary value of `x` to `n` decimal places with
/// round-half-even, then parses the decimal back to the nearest `f64`. Rust's
/// `{:.n$}` formatting takes the same exact-value path, so a format-then-parse
/// reproduces it. Scale-round-divide diverges: it gives `2.68` for
/// `round_dp(2.675, 2)` where Python gives `2.67`.
///
/// A non-finite `x` comes back unchanged, as Python returns it unchanged.
#[must_use]
pub fn round_dp(x: f64, digits: u32) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let text = format!("{x:.digits$}", digits = digits as usize);
    text.parse::<f64>().unwrap_or(x)
}

/// The number of days between two instants, as 1.0 computes it (trap T8).
///
/// 1.0 evaluates `(t - t0).total_seconds() / 86400.0`. `total_seconds()` divides the
/// microsecond difference by `10**6`, and that quotient is divided by `86400.0`, so
/// the value carries TWO roundings. A single division by `86_400_000_000.0` diverges
/// from it for about one input in four, so this function keeps both steps.
///
/// The first step is exact while the microsecond difference stays inside `2**53`
/// microseconds, which is about 285 years. Beyond that the conversion of the
/// difference to `f64` rounds before the division, and the result can differ from
/// Python by one unit in the last place.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "the conversion is exact below 2**53 microseconds, which is 285 years"
)]
pub fn days_between(t0_us: i64, t_us: i64) -> f64 {
    let delta = (i128::from(t_us) - i128::from(t0_us))
        .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
    (delta as f64 / MICROSECONDS_PER_SECOND) / SECONDS_PER_DAY
}

/// Resolve an IANA time-zone name. `None` means UTC, as 1.0's `_zone` defines it.
///
/// # Errors
///
/// Returns [`TimeError::UnknownTimezone`] when the compiled database has no such name.
pub fn resolve_timezone(tz: Option<&str>) -> Result<Tz, TimeError> {
    match tz {
        None => Ok(Tz::UTC),
        Some(name) => name
            .parse::<Tz>()
            .map_err(|_| TimeError::UnknownTimezone(name.to_owned())),
    }
}

/// Convert a UTC microsecond instant to a `chrono` date-time.
///
/// # Errors
///
/// Returns [`TimeError::TimestampOutOfRange`] when the instant is outside the range
/// `chrono` represents.
pub fn to_datetime(us: i64) -> Result<DateTime<Utc>, TimeError> {
    DateTime::<Utc>::from_timestamp_micros(us).ok_or(TimeError::TimestampOutOfRange(us))
}

/// Convert a `chrono` naive date-time, read as UTC, to a microsecond instant.
///
/// A naive timestamp is UTC everywhere in 1.0 and in 2.0 (trap T8).
///
/// The whole `chrono` date range fits in `i64` microseconds, so this cannot overflow.
#[must_use]
pub fn from_naive_utc(naive: NaiveDateTime) -> i64 {
    naive.and_utc().timestamp_micros()
}

/// The local calendar date of a UTC microsecond instant under time zone `tz`
/// (trap T9). `None` means UTC.
///
/// Time zones enter the fold only here: the streak, the "today" XP total, and the
/// velocity window read a local DATE, never a local date-time.
///
/// # Errors
///
/// Returns [`TimeError::UnknownTimezone`] for an unknown name and
/// [`TimeError::TimestampOutOfRange`] for an unrepresentable instant.
pub fn local_day(us: i64, tz: Option<&str>) -> Result<NaiveDate, TimeError> {
    let zone = resolve_timezone(tz)?;
    local_day_in(us, zone)
}

/// The local calendar date of a UTC microsecond instant in an already resolved zone.
///
/// A caller that folds many events resolves the zone once and calls this.
///
/// # Errors
///
/// Returns [`TimeError::TimestampOutOfRange`] for an unrepresentable instant.
pub fn local_day_in(us: i64, zone: Tz) -> Result<NaiveDate, TimeError> {
    Ok(to_datetime(us)?.with_timezone(&zone).date_naive())
}
