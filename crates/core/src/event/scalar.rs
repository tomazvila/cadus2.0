//! The envelope scalars: the schema version, the instant, the curriculum id, and
//! the checked numbers of the 1.0 models.

use std::fmt;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use crate::numeric;

use super::{EventError, SCHEMA_VERSION};

/// `Deserialize` a checked scalar: read the raw wire value, then apply the
/// checked constructor, whose error becomes the serde error.
macro_rules! checked_deserialize {
    ($name:ident, $raw:ty, $build:expr) => {
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = <$raw>::deserialize(deserializer)?;
                $build(value).map_err(|error| de::Error::custom(error.to_string()))
            }
        }
    };
}

/// The `v` key of the envelope. It reads and writes the value `1` and nothing else.
///
/// 1.0 reads `v`, then applies `SHIMS[v]` while `v` is below `SCHEMA_VERSION`. The
/// table is empty and `SCHEMA_VERSION` is `1`, so every other version raises there
/// and is an error value here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SchemaVersion;

impl SchemaVersion {
    /// The numeric value on the wire.
    #[must_use]
    pub const fn get(self) -> i64 {
        SCHEMA_VERSION
    }
}

impl Serialize for SchemaVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(SCHEMA_VERSION)
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = i64::deserialize(deserializer)?;
        if value == SCHEMA_VERSION {
            Ok(Self)
        } else {
            Err(de::Error::custom(format!(
                "unsupported event schema version v{value}; this build reads v{SCHEMA_VERSION} only"
            )))
        }
    }
}

/// A UTC instant, held as microseconds since the Unix epoch (trap T8).
///
/// The wire form is RFC 3339. A value with no offset is read as UTC, exactly as
/// 1.0's `as_utc` reads it. The value is written back with a `Z` suffix: second
/// precision when the microsecond part is zero, and six fractional digits when it is
/// not, which is the form 1.0 writes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp(i64);

impl Timestamp {
    /// Build an instant from microseconds since the Unix epoch.
    #[must_use]
    pub const fn from_micros(micros: i64) -> Self {
        Self(micros)
    }

    /// The instant as microseconds since the Unix epoch.
    #[must_use]
    pub const fn micros(self) -> i64 {
        self.0
    }

    /// Parse an RFC 3339 date-time, or a naive date-time that is read as UTC.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::InvalidTimestamp`] when no accepted form matches.
    pub fn parse(text: &str) -> Result<Self, EventError> {
        if let Ok(offset) = DateTime::parse_from_rfc3339(text) {
            return Ok(Self(offset.with_timezone(&Utc).timestamp_micros()));
        }
        for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(text, format) {
                return Ok(Self(numeric::from_naive_utc(naive)));
            }
        }
        Err(EventError::InvalidTimestamp(text.to_owned()))
    }

    /// The wire form: RFC 3339 with a `Z` suffix.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::InvalidTimestamp`] when the instant is outside the range
    /// `chrono` represents.
    pub fn to_wire_string(self) -> Result<String, EventError> {
        let stamp = numeric::to_datetime(self.0)
            .map_err(|_| EventError::InvalidTimestamp(self.0.to_string()))?;
        let format = if self.0.rem_euclid(1_000_000) == 0 {
            "%Y-%m-%dT%H:%M:%SZ"
        } else {
            "%Y-%m-%dT%H:%M:%S%.6fZ"
        };
        Ok(stamp.format(format).to_string())
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_wire_string() {
            Ok(text) => f.write_str(&text),
            Err(_) => write!(f, "{}us", self.0),
        }
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text = self
            .to_wire_string()
            .map_err(|error| serde::ser::Error::custom(error.to_string()))?;
        serializer.serialize_str(&text)
    }
}

checked_deserialize!(Timestamp, String, |text: String| Timestamp::parse(&text));

/// A non-empty curriculum id: a topic, a course, or a knowledge point.
///
/// 1.0 declares these as
/// `Annotated[str, StringConstraints(min_length=1, strip_whitespace=True)]`
/// (`model.py:23`), so pydantic REMOVES the outer whitespace and then applies the
/// length rule. The port does the same: the value keeps its trimmed form, and a
/// value with nothing left after the trim is an error. Without the trim a padded
/// topic id folds onto a phantom topic and the real one keeps no credit.
///
/// [`crate::curriculum::model::Slug`] is the same 1.0 type on the curriculum side
/// and carries the same rule. The two stay separate types because they report
/// through different error enums.
///
/// The trim is Rust `str::trim`, which removes the Unicode `White_Space` set.
/// Python `str.strip()` removes that set AND the four separators `U+001C` to
/// `U+001F`, so an id padded with one of those four keeps it here. The same rule
/// holds on the curriculum side, and no authored id carries such a character.
///
/// The type orders by byte, which is the code-point order Python's `sorted()` gives
/// for a `str` (trap T18).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Slug(String);

impl Slug {
    /// Build a slug from text. The outer whitespace goes away.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Json`] when nothing is left after the trim, which
    /// covers the empty string and a whitespace-only id.
    pub fn new(text: impl Into<String>) -> Result<Self, EventError> {
        let text = text.into();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(EventError::Json("a curriculum id must not be empty".into()));
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Slug {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

checked_deserialize!(Slug, String, Slug::new);

/// A duration in whole seconds that is zero or more (`secs` in 1.0, `ge=0`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Secs(i64);

impl Secs {
    /// Build a duration.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Json`] when `value` is negative.
    pub fn new(value: i64) -> Result<Self, EventError> {
        if value < 0 {
            return Err(EventError::Json(format!("secs {value} must be 0 or more")));
        }
        Ok(Self(value))
    }
}

checked_deserialize!(Secs, i64, Secs::new);

/// A duration in whole seconds that is more than zero (`expected_time_secs`, `gt=0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct PositiveSecs(i64);

impl PositiveSecs {
    /// Build a duration.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Json`] when `value` is zero or negative.
    pub fn new(value: i64) -> Result<Self, EventError> {
        if value <= 0 {
            return Err(EventError::Json(format!(
                "expected_time_secs {value} must be more than 0"
            )));
        }
        Ok(Self(value))
    }
}

checked_deserialize!(PositiveSecs, i64, PositiveSecs::new);

/// A diagnostic answer weight in the closed range 0.0 to 1.0.
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Weight(f64);

impl Weight {
    /// Build a weight.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Json`] when `value` is outside 0.0 to 1.0, `NaN`
    /// included.
    pub fn new(value: f64) -> Result<Self, EventError> {
        if !(0.0..=1.0).contains(&value) {
            return Err(EventError::Json(format!(
                "weight {value} must be between 0.0 and 1.0"
            )));
        }
        Ok(Self(value))
    }

    /// The weight.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

checked_deserialize!(Weight, f64, Weight::new);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scalars_check_their_range_and_write_their_wire_form() {
        let stamp = Timestamp::parse("2026-03-02T09:00:00.5").expect("a naive form reads");
        assert_eq!(stamp.to_string(), "2026-03-02T09:00:00.500000Z");
        assert_eq!(
            Timestamp::from_micros(i64::MAX).to_string(),
            "9223372036854775807us"
        );
        assert!(Timestamp::parse("soon").is_err());
        let slug = Slug::new(" a ").expect("a padded id trims");
        assert_eq!(slug.to_string(), "a");
        assert_eq!(slug.as_ref(), "a");
        assert!(Slug::new(" ").is_err());
        assert!(Secs::new(-1).is_err() && Secs::new(0).is_ok());
        assert!(PositiveSecs::new(0).is_err() && PositiveSecs::new(1).is_ok());
        assert!(Weight::new(1.5).is_err());
        assert_eq!(Weight::new(0.5).expect("in range").get(), 0.5);
        assert_eq!(SchemaVersion.get(), SCHEMA_VERSION);
    }
}
