use std::fmt;
use std::ops::Deref;
use std::str::FromStr;
use std::time::Duration;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::http::{format_duration, parse_duration, DurationParseError};

/// A [`Duration`] that reads and writes rkt's duration syntax.
///
/// [`Duration`]'s own `serde` implementation is a `{ secs, nanos }` structure,
/// which is unusable in a configuration file. This `serde` implementation
/// deserializes from either a duration string, as parsed by
/// [`parse_duration()`], or a bare integer number of seconds, and always
/// serializes to the canonical string produced by [`format_duration()`]. Zero
/// and negative durations are rejected.
///
/// # Example
///
/// ```rust
/// use std::time::Duration;
/// use rkt::config::ConfigDuration;
///
/// let thirty_seconds = ConfigDuration::from(Duration::from_secs(30));
/// assert_eq!(thirty_seconds.to_string(), "30s");
/// assert_eq!("30s".parse::<ConfigDuration>().unwrap(), thirty_seconds);
/// assert_eq!(*thirty_seconds, Duration::from_secs(30));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConfigDuration(pub Duration);

impl From<Duration> for ConfigDuration {
    fn from(duration: Duration) -> Self {
        ConfigDuration(duration)
    }
}

impl From<ConfigDuration> for Duration {
    fn from(duration: ConfigDuration) -> Self {
        duration.0
    }
}

impl Deref for ConfigDuration {
    type Target = Duration;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for ConfigDuration {
    type Err = DurationParseError;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        parse_duration(string).map(ConfigDuration)
    }
}

impl fmt::Display for ConfigDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format_duration(self.0))
    }
}

impl Serialize for ConfigDuration {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format_duration(self.0))
    }
}

struct DeVisitor;

const EXPECTED: &str = "a duration string such as `30s` or a number of seconds";

impl<'de> de::Visitor<'de> for DeVisitor {
    type Value = ConfigDuration;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(EXPECTED)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        // A bare integer is a count of seconds, exactly as in a duration
        // string, so route it through the same parser to reject zero.
        self.visit_str(&v.to_string())
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        u64::try_from(v)
            .map_err(|_| E::invalid_value(de::Unexpected::Signed(v), &EXPECTED))
            .and_then(|v| self.visit_u64(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        ConfigDuration::from_str(v).map_err(|_| E::invalid_value(de::Unexpected::Str(v), &self))
    }
}

impl<'de> Deserialize<'de> for ConfigDuration {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        de.deserialize_str(DeVisitor)
    }
}
