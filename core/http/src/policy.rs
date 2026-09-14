//! Parsers for the duration and rate strings used to configure request body
//! policy.
//!
//! The same strings appear in global configuration (`Rocket.toml`, environment
//! variables) and, eventually, in route attributes, so they are parsed
//! identically at runtime and at compile time. These parsers live here, in
//! `rkt_http`, because `rkt_codegen` depends on this crate and needs them.
//!
//! # Durations
//!
//! A duration is a sequence of `<integer><unit>` components in strictly
//! descending unit order, where the unit is one of `h`, `m`, `s`, or `ms`. A
//! string consisting of a bare integer, and nothing else, is a count of
//! seconds. A duration must be non-zero.
//!
//! ```rust
//! use std::time::Duration;
//! use rkt::http::parse_duration;
//!
//! assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
//! assert_eq!(parse_duration("250ms").unwrap(), Duration::from_millis(250));
//! assert_eq!(parse_duration("1m30s").unwrap(), Duration::from_secs(90));
//! assert_eq!(parse_duration("60").unwrap(), Duration::from_secs(60));
//!
//! assert!(parse_duration("0s").is_err());
//! assert!(parse_duration("30s1m").is_err());
//! assert!(parse_duration("-5s").is_err());
//! ```
//!
//! # Rates
//!
//! A [`Rate`] is a size and a period separated by a `/`: `8KiB/s`, `1MiB/10s`,
//! `512/s`. The size is parsed by [`ByteUnit`], so a bare integer is a count of
//! bytes. A period given as a bare unit, as in `8KiB/s`, means one of that
//! unit.
//!
//! ```rust
//! use std::time::Duration;
//! use rkt::http::Rate;
//!
//! let rate: Rate = "8KiB/s".parse().unwrap();
//! assert_eq!(rate.bytes, 8192);
//! assert_eq!(rate.per, Duration::from_secs(1));
//!
//! assert!("30".parse::<Rate>().is_err());
//! assert!("0/s".parse::<Rate>().is_err());
//! ```

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use ubyte::ByteUnit;

/// The millisecond value and ordering rank of a duration unit.
///
/// Components must appear in strictly descending unit order, which the rank
/// makes cheap to check: `h` < `m` < `s` < `ms`.
fn unit(suffix: &str) -> Option<(u8, u128)> {
    match suffix {
        "h" => Some((1, 3_600_000)),
        "m" => Some((2, 60_000)),
        "s" => Some((3, 1_000)),
        "ms" => Some((4, 1)),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DurationErrorKind {
    Empty,
    Unexpected(char),
    UnknownUnit,
    UnitOrder,
    Overflow,
    Zero,
}

/// An error returned by [`parse_duration()`] when a duration string is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurationParseError(DurationErrorKind);

impl fmt::Display for DurationParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            DurationErrorKind::Empty => f.write_str("the input was empty"),
            DurationErrorKind::Unexpected(c) => write!(f, "unexpected character {c:?}"),
            DurationErrorKind::UnknownUnit => {
                f.write_str("unknown duration unit: expected one of `h`, `m`, `s`, `ms`")
            }
            DurationErrorKind::UnitOrder => {
                f.write_str("duration units must be unique and in descending order")
            }
            DurationErrorKind::Overflow => f.write_str("duration is too large"),
            DurationErrorKind::Zero => f.write_str("duration must be non-zero"),
        }
    }
}

impl std::error::Error for DurationParseError {}

/// Parses a duration string such as `"30s"`, `"250ms"`, `"1m30s"`, or `"2h"`.
///
/// A string consisting of a bare integer is a count of seconds: `"60"` is
/// `"60s"`. Any other component must carry one of the units `h`, `m`, `s`, or
/// `ms`, and components must appear in strictly descending unit order. The
/// resulting duration must be non-zero.
///
/// # Example
///
/// ```rust
/// use std::time::Duration;
/// use rkt::http::parse_duration;
///
/// assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(7200));
/// assert_eq!(parse_duration("1m30s").unwrap(), Duration::from_secs(90));
/// ```
pub fn parse_duration(string: &str) -> Result<Duration, DurationParseError> {
    use DurationErrorKind::*;

    if string.is_empty() {
        return Err(DurationParseError(Empty));
    }

    let bytes = string.as_bytes();
    let (mut i, mut millis, mut last_rank) = (0, 0u128, 0u8);
    while i < bytes.len() {
        let digits = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }

        if i == digits {
            let c = string[digits..].chars().next().expect("non-empty remainder");
            return Err(DurationParseError(Unexpected(c)));
        }

        let value: u128 = string[digits..i].parse().map_err(|_| DurationParseError(Overflow))?;

        let suffix = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }

        // A bare integer, and only a bare integer, is a count of seconds.
        let (rank, scale) = match &string[suffix..i] {
            "" if digits == 0 && i == bytes.len() => (3, 1_000),
            suffix => unit(suffix).ok_or(DurationParseError(UnknownUnit))?,
        };

        if rank <= last_rank {
            return Err(DurationParseError(UnitOrder));
        }

        last_rank = rank;
        millis = value
            .checked_mul(scale)
            .and_then(|v| millis.checked_add(v))
            .ok_or(DurationParseError(Overflow))?;
    }

    let millis = u64::try_from(millis).map_err(|_| DurationParseError(Overflow))?;
    match millis {
        0 => Err(DurationParseError(Zero)),
        millis => Ok(Duration::from_millis(millis)),
    }
}

/// Renders `duration` in the canonical form read by [`parse_duration()`].
///
/// Sub-millisecond precision is truncated. This is the inverse of
/// [`parse_duration()`] for any duration it produces.
///
/// # Example
///
/// ```rust
/// use std::time::Duration;
/// use rkt::http::format_duration;
///
/// assert_eq!(format_duration(Duration::from_secs(90)), "1m30s");
/// assert_eq!(format_duration(Duration::from_millis(250)), "250ms");
/// ```
pub fn format_duration(duration: Duration) -> String {
    let mut remaining = duration.as_millis();
    if remaining == 0 {
        return "0s".into();
    }

    let mut string = String::new();
    for (suffix, scale) in [("h", 3_600_000u128), ("m", 60_000), ("s", 1_000), ("ms", 1)] {
        let count = remaining / scale;
        if count > 0 {
            string.push_str(&count.to_string());
            string.push_str(suffix);
            remaining -= count * scale;
        }
    }

    string
}

/// A data rate: [`bytes`](Rate::bytes) bytes per [`per`](Rate::per).
///
/// Parsed from a size and a period separated by a `/`, as in `"8KiB/s"`,
/// `"1MiB/10s"`, or `"512/s"`. See the [module docs](self) for the accepted
/// forms.
///
/// # Example
///
/// ```rust
/// use std::time::Duration;
/// use rkt::http::Rate;
///
/// let rate: Rate = "1MiB/s".parse().unwrap();
/// assert_eq!(rate.bytes, 1024 * 1024);
/// assert_eq!(rate.per, Duration::from_secs(1));
/// assert_eq!(rate.to_string(), "1MiB/1s");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rate {
    /// The number of bytes transferred per [`per`](Rate::per). Always non-zero.
    pub bytes: u64,
    /// The period over which [`bytes`](Rate::bytes) are transferred. Always
    /// non-zero.
    pub per: Duration,
}

#[derive(Debug, Clone)]
enum RateErrorKind {
    MissingPeriod,
    BadSize(ubyte::Error),
    ZeroSize,
    BadPeriod(DurationParseError),
}

/// An error returned when a [`Rate`] string is invalid.
#[derive(Debug, Clone)]
pub struct RateParseError(RateErrorKind);

impl fmt::Display for RateParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            RateErrorKind::MissingPeriod => {
                f.write_str("a rate must be a size and a period separated by `/`, as in `8KiB/s`")
            }
            RateErrorKind::BadSize(e) => write!(f, "invalid rate size: {e}"),
            RateErrorKind::ZeroSize => f.write_str("rate size must be non-zero"),
            RateErrorKind::BadPeriod(e) => write!(f, "invalid rate period: {e}"),
        }
    }
}

impl std::error::Error for RateParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            RateErrorKind::BadPeriod(e) => Some(e),
            _ => None,
        }
    }
}

impl FromStr for Rate {
    type Err = RateParseError;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        use RateErrorKind::*;

        let (size, period) = string.split_once('/').ok_or(RateParseError(MissingPeriod))?;
        let bytes = ByteUnit::from_str(size).map_err(|e| RateParseError(BadSize(e)))?.as_u64();
        if bytes == 0 {
            return Err(RateParseError(ZeroSize));
        }

        if period.is_empty() {
            return Err(RateParseError(MissingPeriod));
        }

        // A period written as a bare unit, as in `8KiB/s`, means one of it.
        let per = match period.as_bytes()[0].is_ascii_digit() {
            true => parse_duration(period),
            false => parse_duration(&format!("1{period}")),
        };

        Ok(Rate { bytes, per: per.map_err(|e| RateParseError(BadPeriod(e)))? })
    }
}

impl fmt::Display for Rate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", ByteUnit::from(self.bytes), format_duration(self.per))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! assert_parses {
        ($($string:expr => $duration:expr),* $(,)?) => ($(
            match parse_duration($string) {
                Ok(actual) => assert_eq!(actual, $duration, "parsing {:?}", $string),
                Err(e) => panic!("{:?} failed to parse: {}", $string, e),
            }
        )*)
    }

    macro_rules! assert_rejects {
        ($($string:expr),* $(,)?) => ($(
            if let Ok(d) = parse_duration($string) {
                panic!("{:?} parsed as {:?}", $string, d);
            }
        )*)
    }

    #[test]
    fn parses_durations() {
        assert_parses! {
            "30s" => Duration::from_secs(30),
            "250ms" => Duration::from_millis(250),
            "1ms" => Duration::from_millis(1),
            "1m" => Duration::from_secs(60),
            "1m30s" => Duration::from_secs(90),
            "2h" => Duration::from_secs(7200),
            "1h2m3s4ms" => Duration::from_millis(3_723_004),
            "60" => Duration::from_secs(60),
            "1" => Duration::from_secs(1),
        }
    }

    #[test]
    fn rejects_bad_durations() {
        assert_rejects! {
            "", "0", "0s", "0ms", "0h0m0s",
            "-5s", "+5s", "5.5s", "s", "ms", "5x", "5S", "5MS",
            "1m 30s", " 30s", "30s ", "30s,1m",
            "30s1m", "1s1s", "1ms1s", "1m30", "30s5",
            "99999999999999999999999999999999999999999h",
            "18446744073709551616ms",
        }
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(Duration::from_secs(30)), "30s");
        assert_eq!(format_duration(Duration::from_millis(250)), "250ms");
        assert_eq!(format_duration(Duration::from_secs(90)), "1m30s");
        assert_eq!(format_duration(Duration::from_secs(7200)), "2h");
        assert_eq!(format_duration(Duration::from_millis(3_723_004)), "1h2m3s4ms");
        assert_eq!(format_duration(Duration::ZERO), "0s");
    }

    #[test]
    fn duration_round_trips() {
        for string in ["30s", "250ms", "1m30s", "2h", "1h2m3s4ms"] {
            let duration = parse_duration(string).unwrap();
            assert_eq!(format_duration(duration), string);
            assert_eq!(parse_duration(&format_duration(duration)).unwrap(), duration);
        }
    }

    #[test]
    fn parses_rates() {
        let cases = [
            ("8KiB/s", 8 * 1024, Duration::from_secs(1)),
            ("1MiB/s", 1024 * 1024, Duration::from_secs(1)),
            ("512/s", 512, Duration::from_secs(1)),
            ("1MiB/m", 1024 * 1024, Duration::from_secs(60)),
            ("1MiB/10s", 1024 * 1024, Duration::from_secs(10)),
            ("1kB/500ms", 1000, Duration::from_millis(500)),
        ];

        for (string, bytes, per) in cases {
            let rate: Rate = string.parse().unwrap_or_else(|e| panic!("{string:?}: {e}"));
            assert_eq!(rate, Rate { bytes, per }, "parsing {string:?}");
        }
    }

    #[test]
    fn rejects_bad_rates() {
        for string in ["", "30", "8KiB", "/s", "8KiB/", "0/s", "0KiB/s", "8KiB/0s",
                       "8KiB/x", "8XiB/s", "8KiB/s/s", "-8KiB/s", "8 KiB / s"] {
            if let Ok(rate) = string.parse::<Rate>() {
                panic!("{string:?} parsed as {rate:?}");
            }
        }
    }

    #[test]
    fn rate_round_trips() {
        for string in ["8KiB/1s", "1MiB/1m", "512B/500ms"] {
            let rate: Rate = string.parse().unwrap();
            assert_eq!(rate.to_string(), string);
            assert_eq!(rate.to_string().parse::<Rate>().unwrap(), rate);
        }
    }
}
