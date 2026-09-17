use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::data::ByteUnit;
use crate::http::{format_duration, Rate};

/// The progress deadline applied when no progress policy is configured.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// The grace period applied when [`Config::body_min_rate`] is configured
/// without [`Config::body_grace`].
const DEFAULT_GRACE: Duration = Duration::from_secs(15);

/// The value of the [`body_policy`](Config::body_policy) configuration key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BodyPolicySetting {
    /// Impose no bound at all on the time taken to receive a request body. The
    /// hard size limit in [`Config::body_limit`] continues to apply.
    Unlimited,
}

/// How the progress of an incoming request body is bounded.
///
/// This is the resolved form of the mutually exclusive [`Config::body_timeout`],
/// [`Config::body_min_rate`], and [`Config::body_policy`] settings. Obtain it
/// via [`Config::body_policy()`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyProgress {
    /// The complete body must be received within this duration, measured from
    /// the moment the body becomes available. Receiving data neither resets nor
    /// extends the deadline.
    Timeout(Duration),
    /// The body must be received at an average of at least `rate` once `grace`
    /// has elapsed, which is to say by `grace + received bytes / rate`.
    MinRate {
        /// The minimum average rate at which payload bytes must arrive.
        rate: Rate,
        /// The initial period during which no rate is required.
        grace: Duration,
    },
    /// The time taken to receive the body is not bounded.
    Unlimited,
}

/// The request body policy in effect: a progress bound and a hard size limit.
///
/// The two are independent: a body that arrives fast enough is still rejected
/// once it exceeds [`limit`](BodyPolicy::limit). Obtain the globally configured
/// policy via [`Config::body_policy()`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyPolicy {
    /// The bound on how long the body may take to arrive.
    pub progress: BodyProgress,
    /// The hard limit on the size of the complete request body.
    pub limit: ByteUnit,
}

impl fmt::Display for BodyProgress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BodyProgress::Timeout(timeout) => write!(f, "deadline {}", format_duration(*timeout)),
            BodyProgress::MinRate { rate, grace } => {
                write!(f, "min rate {rate} after {} grace", format_duration(*grace))
            }
            BodyProgress::Unlimited => f.write_str("unlimited"),
        }
    }
}

impl Config {
    /// Returns the request body policy resolved from this configuration.
    ///
    /// The progress bound is [`BodyProgress::Unlimited`] if
    /// [`body_policy`](Config::body_policy) is `"unlimited"`,
    /// [`BodyProgress::MinRate`] if [`body_min_rate`](Config::body_min_rate) is
    /// set, and a [`BodyProgress::Timeout`] otherwise. Unset values take their
    /// documented defaults: a `60s` deadline and a `15s` grace period.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use rkt::Config;
    /// use rkt::data::{BodyProgress, ToByteUnit};
    ///
    /// let config = Config::default();
    /// let policy = config.body_policy();
    /// assert_eq!(policy.progress, BodyProgress::Timeout(Duration::from_secs(60)));
    /// assert_eq!(policy.limit, 8.mebibytes());
    /// ```
    pub fn body_policy(&self) -> BodyPolicy {
        let progress = if self.body_policy == Some(BodyPolicySetting::Unlimited) {
            BodyProgress::Unlimited
        } else if let Some(rate) = self.body_min_rate {
            let grace = self.body_grace.map_or(DEFAULT_GRACE, |grace| grace.0);
            BodyProgress::MinRate { rate, grace }
        } else {
            BodyProgress::Timeout(self.body_timeout.map_or(DEFAULT_TIMEOUT, |t| t.0))
        };

        BodyPolicy { progress, limit: self.body_limit }
    }

    /// Checks the body policy keys for combinations the deserializer cannot
    /// reject on its own: at most one progress policy may be selected, and a
    /// grace period is meaningless without a minimum rate.
    pub(crate) fn validate_body_policy(&self) -> figment::error::Result<()> {
        let mut selected = Vec::new();
        if self.body_timeout.is_some() {
            selected.push(Self::BODY_TIMEOUT);
        }
        if self.body_min_rate.is_some() {
            selected.push(Self::BODY_MIN_RATE);
        }
        if self.body_policy.is_some() {
            selected.push(Self::BODY_POLICY);
        }

        if selected.len() > 1 {
            let keys = selected.join("`, `");
            return Err(format!("`{keys}` are mutually exclusive: set at most one").into());
        }

        if self.body_grace.is_some() && self.body_min_rate.is_none() {
            let (grace, rate) = (Self::BODY_GRACE, Self::BODY_MIN_RATE);
            return Err(format!("`{grace}` is only meaningful with `{rate}`").into());
        }

        Ok(())
    }
}
