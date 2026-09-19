use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use std::{fmt, io};

use futures::stream::Stream;
use hyper::body::Bytes;
use serde::{Deserialize, Serialize};
use tokio::time::{Instant, Sleep};

use crate::config::Config;
use crate::data::data_stream::RawStream;
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

/// An error produced when a request body violates its [`BodyPolicy`].
///
/// Readers of [`Data`](crate::data::Data) and [`DataStream`](crate::data::DataStream)
/// receive this error as an [`io::Error`] whose [`kind()`](io::Error::kind) is
/// [`io::ErrorKind::TimedOut`] for a progress failure and
/// [`io::ErrorKind::FileTooLarge`] for a size failure, and whose inner error
/// is the `BodyPolicyError` itself. Use [`BodyPolicyError::from_io()`] to
/// recover it from an `io::Error`, including one that wraps it.
///
/// A policy failure is terminal: once a body has failed, every subsequent read
/// of any [`Data`](crate::data::Data) or [`DataStream`](crate::data::DataStream)
/// derived from it returns the same error.
///
/// # Example
///
/// ```rust
/// use std::io;
/// use rkt::data::{Data, BodyPolicyError, ToByteUnit};
///
/// async fn handler(data: Data<'_>) -> io::Result<String> {
///     match data.open(1.mebibytes()).into_string().await {
///         Ok(string) => Ok(string.into_inner()),
///         Err(e) => match BodyPolicyError::from_io(&e) {
///             Some(BodyPolicyError::LimitExceeded { limit, .. }) => {
///                 Err(io::Error::other(format!("body larger than {limit}")))
///             }
///             Some(policy_error) => {
///                 Err(io::Error::other(format!("body too slow: {policy_error}")))
///             }
///             None => Err(e),
///         }
///     }
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodyPolicyError {
    /// The body was not completely received within the
    /// [`BodyProgress::Timeout`] deadline.
    Timeout {
        /// The deadline, measured from the moment the body became available.
        timeout: Duration,
        /// The time that had elapsed since the body became available.
        elapsed: Duration,
        /// The number of payload bytes received before the failure.
        received: u64,
    },
    /// The body's average rate fell below the [`BodyProgress::MinRate`] rate
    /// once the grace period had elapsed.
    MinRate {
        /// The minimum average rate required.
        rate: Rate,
        /// The initial period during which no rate was required.
        grace: Duration,
        /// The time that had elapsed since the body became available.
        elapsed: Duration,
        /// The number of payload bytes received before the failure.
        received: u64,
    },
    /// More payload bytes were received than the [`BodyPolicy::limit`] allows.
    LimitExceeded {
        /// The hard limit on the size of the body.
        limit: ByteUnit,
        /// The time that had elapsed since the body became available.
        elapsed: Duration,
        /// The number of payload bytes received, including those that exceeded
        /// the limit.
        received: u64,
    },
}

impl BodyPolicyError {
    /// Returns the `BodyPolicyError` that `error` carries, if any.
    ///
    /// The error is found whether it is the inner error of `error` itself or
    /// is reachable through any chain of wrapping errors, such as the wrapping
    /// applied when a multipart field is read from a body that has failed.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::io;
    /// use rkt::data::BodyPolicyError;
    ///
    /// fn is_policy_failure(error: &io::Error) -> bool {
    ///     BodyPolicyError::from_io(error).is_some()
    /// }
    ///
    /// let error = io::Error::new(io::ErrorKind::Other, "some other error");
    /// assert!(!is_policy_failure(&error));
    /// ```
    pub fn from_io(error: &io::Error) -> Option<&Self> {
        let mut error: &(dyn std::error::Error + 'static) = error.get_ref()?;
        loop {
            if let Some(policy_error) = error.downcast_ref::<Self>() {
                return Some(policy_error);
            }

            // `io::Error::source()` skips its inner error, so unwrap it by hand.
            error = match error.downcast_ref::<io::Error>() {
                Some(io_error) => io_error.get_ref()?,
                None => error.source()?,
            };
        }
    }

    /// The [`io::ErrorKind`] of the [`io::Error`] this error is surfaced as:
    /// [`TimedOut`](io::ErrorKind::TimedOut) for a progress failure and
    /// [`FileTooLarge`](io::ErrorKind::FileTooLarge) for a size failure.
    pub fn kind(&self) -> io::ErrorKind {
        match self {
            BodyPolicyError::Timeout { .. } | BodyPolicyError::MinRate { .. } => {
                io::ErrorKind::TimedOut
            }
            BodyPolicyError::LimitExceeded { .. } => io::ErrorKind::FileTooLarge,
        }
    }

    /// The time that had elapsed since the body became available when the
    /// failure occurred.
    pub fn elapsed(&self) -> Duration {
        match *self {
            BodyPolicyError::Timeout { elapsed, .. }
            | BodyPolicyError::MinRate { elapsed, .. }
            | BodyPolicyError::LimitExceeded { elapsed, .. } => elapsed,
        }
    }

    /// The number of payload bytes received when the failure occurred.
    pub fn received(&self) -> u64 {
        match *self {
            BodyPolicyError::Timeout { received, .. }
            | BodyPolicyError::MinRate { received, .. }
            | BodyPolicyError::LimitExceeded { received, .. } => received,
        }
    }
}

impl fmt::Display for BodyPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (elapsed, received) = (format_duration(self.elapsed()), ByteUnit::from(self.received()));
        match self {
            BodyPolicyError::Timeout { timeout, .. } => write!(
                f,
                "request body not received within {}: {received} received after {elapsed}",
                format_duration(*timeout)
            ),
            BodyPolicyError::MinRate { rate, grace, .. } => write!(
                f,
                "request body fell below the minimum rate of {rate} after {} grace: \
                {received} received after {elapsed}",
                format_duration(*grace)
            ),
            BodyPolicyError::LimitExceeded { limit, .. } => write!(
                f,
                "request body exceeds the {limit} limit: {received} received after {elapsed}"
            ),
        }
    }
}

impl std::error::Error for BodyPolicyError {}

impl From<BodyPolicyError> for io::Error {
    fn from(error: BodyPolicyError) -> Self {
        io::Error::new(error.kind(), error)
    }
}

/// The time `rate` allows for `bytes` bytes: `bytes * rate.per / rate.bytes`.
fn time_for(rate: Rate, bytes: u64) -> Duration {
    const NANOS_PER_SEC: u128 = 1_000_000_000;

    let nanos = rate.per.as_nanos().saturating_mul(bytes as u128) / rate.bytes as u128;
    match u64::try_from(nanos / NANOS_PER_SEC) {
        Ok(secs) => Duration::new(secs, (nanos % NANOS_PER_SEC) as u32),
        Err(_) => Duration::MAX,
    }
}

/// A [`RawStream`] whose payload is subject to a [`BodyPolicy`].
///
/// This is the single point through which every payload byte of a request body
/// flows, whether read via [`Data::peek()`](crate::data::Data::peek()) or a
/// [`DataStream`](crate::data::DataStream), and thus the single point at which
/// the policy is enforced. It owns the state of the body: the moment it became
/// available, the number of payload bytes received so far, and whether it has
/// ended or failed. That state moves with the stream, so forwarding a
/// [`Data`](crate::data::Data) or opening it never restarts the clock.
///
/// Every poll that would wait on the client arms a timer at the current
/// deadline, so a stalled client is cut off at the deadline whether or not it
/// ever sends another byte. Only non-empty payload chunks count as progress.
pub struct PolicedStream<'r> {
    stream: RawStream<'r>,
    /// `None` for a stream, such as a multipart field, that is read from a
    /// body which enforces its own policy on every byte this stream yields.
    policy: Option<BodyPolicy>,
    origin: Instant,
    received: u64,
    complete: bool,
    failure: Option<BodyPolicyError>,
    /// Armed at the current deadline while the underlying stream is pending.
    sleep: Option<Pin<Box<Sleep>>>,
}

impl<'r> PolicedStream<'r> {
    /// Creates a policed stream for a body that has just become available.
    /// The body clock starts now.
    pub(crate) fn new(stream: RawStream<'r>, policy: BodyPolicy) -> Self {
        Self {
            stream,
            policy: Some(policy),
            origin: Instant::now(),
            received: 0,
            complete: false,
            failure: None,
            sleep: None,
        }
    }

    /// Creates a stream for a portion of a body, such as a multipart field,
    /// that is read through the policed stream of that body. Its bytes are
    /// counted and bounded there, so this stream enforces nothing itself.
    pub(crate) fn inherited(stream: RawStream<'r>) -> Self {
        Self {
            stream,
            policy: None,
            origin: Instant::now(),
            received: 0,
            complete: false,
            failure: None,
            sleep: None,
        }
    }

    /// Counts `len` payload bytes as received, recording a size failure if the
    /// total now exceeds the hard limit. The failure is returned and also
    /// surfaces from every subsequent poll.
    pub(crate) fn count(&mut self, len: usize) -> Result<(), io::Error> {
        self.received = self.received.saturating_add(len as u64);
        match self.policy {
            Some(policy) if self.received > policy.limit => {
                let (limit, received) = (policy.limit, self.received);
                let elapsed = self.elapsed(Instant::now());
                Err(self.fail(BodyPolicyError::LimitExceeded { limit, elapsed, received }))
            }
            _ => Ok(()),
        }
    }

    /// The time elapsed between the body becoming available and `now`.
    fn elapsed(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.origin)
    }

    /// The instant at or after which the body has failed its progress policy,
    /// given the bytes received so far. `None` if progress is not bounded.
    fn deadline(&self) -> Option<Instant> {
        let allowance = match self.policy?.progress {
            BodyProgress::Timeout(timeout) => timeout,
            BodyProgress::MinRate { rate, grace } => {
                grace.saturating_add(time_for(rate, self.received))
            }
            BodyProgress::Unlimited => return None,
        };

        // A deadline too far in the future to represent is not a deadline.
        self.origin.checked_add(allowance)
    }

    /// The progress failure for the policy in effect at `elapsed`. `None` if
    /// progress is not bounded.
    fn progress_error(&self, elapsed: Duration) -> Option<BodyPolicyError> {
        let received = self.received;
        match self.policy?.progress {
            BodyProgress::Timeout(timeout) => {
                Some(BodyPolicyError::Timeout { timeout, elapsed, received })
            }
            BodyProgress::MinRate { rate, grace } => {
                Some(BodyPolicyError::MinRate { rate, grace, elapsed, received })
            }
            BodyProgress::Unlimited => None,
        }
    }

    /// Records `error` as the terminal failure of this body and returns it as
    /// the `io::Error` to surface.
    fn fail(&mut self, error: BodyPolicyError) -> io::Error {
        self.failure = Some(error);
        self.sleep = None;
        error.into()
    }

    /// Polls the deadline timer, arming or re-arming it at `deadline` first.
    /// Under a [`BodyProgress::MinRate`] policy the deadline moves later as
    /// bytes arrive; under a [`BodyProgress::Timeout`] it never moves.
    fn poll_deadline(&mut self, cx: &mut Context<'_>, deadline: Instant) -> Poll<()> {
        let sleep = match &mut self.sleep {
            Some(sleep) => {
                if sleep.deadline() != deadline {
                    sleep.as_mut().reset(deadline);
                }

                sleep
            }
            None => self.sleep.insert(Box::pin(tokio::time::sleep_until(deadline))),
        };

        sleep.as_mut().poll(cx)
    }
}

impl Stream for PolicedStream<'_> {
    type Item = io::Result<Bytes>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if let Some(error) = this.failure {
            return Poll::Ready(Some(Err(error.into())));
        }

        // The policy bounds receiving the body; once it has ended, it no
        // longer applies. A body whose bytes are counted elsewhere is not
        // bounded here at all.
        if this.complete {
            return Poll::Ready(None);
        } else if this.policy.is_none() {
            return Pin::new(&mut this.stream).poll_next(cx);
        }

        // Under `BodyProgress::Unlimited` there is no deadline, and this costs
        // no more than a match: the clock is not even read.
        let deadline = this.deadline();
        if let Some(deadline) = deadline {
            let now = Instant::now();
            if now >= deadline {
                if let Some(error) = this.progress_error(this.elapsed(now)) {
                    return Poll::Ready(Some(Err(this.fail(error))));
                }
            }
        }

        match Pin::new(&mut this.stream).poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                if !bytes.is_empty() {
                    this.count(bytes.len())?;
                }

                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(None) => {
                this.complete = true;
                this.sleep = None;
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(error))),
            Poll::Pending => match deadline {
                Some(deadline) => match this.poll_deadline(cx, deadline) {
                    Poll::Ready(()) => match this.progress_error(this.elapsed(Instant::now())) {
                        Some(error) => Poll::Ready(Some(Err(this.fail(error)))),
                        None => Poll::Pending,
                    },
                    Poll::Pending => Poll::Pending,
                },
                None => Poll::Pending,
            },
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

impl fmt::Display for PolicedStream<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.stream.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use std::future::{poll_fn, Future};
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::Duration;

    use tokio::io::AsyncReadExt;
    use tokio::sync::mpsc;
    use tokio::time::{advance, Instant};
    use ubyte::ToByteUnit;

    use super::*;
    use crate::data::Data;

    type Sender = mpsc::Sender<io::Result<Bytes>>;

    /// A body whose chunks arrive when sent and which ends when the sender is
    /// dropped. Until then, a read that has consumed every chunk is pending.
    struct Channel(mpsc::Receiver<io::Result<Bytes>>);

    impl Stream for Channel {
        type Item = io::Result<Bytes>;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.0.poll_recv(cx)
        }
    }

    fn body(policy: BodyPolicy) -> (Sender, Data<'static>) {
        let (tx, rx) = mpsc::channel(64);
        (tx, Data::from(RawStream::Test(Box::pin(Channel(rx))), policy))
    }

    fn send(tx: &Sender, bytes: &[u8]) {
        tx.try_send(Ok(Bytes::copy_from_slice(bytes))).expect("channel has capacity");
    }

    fn timeout(secs: u64, limit: ByteUnit) -> BodyPolicy {
        let progress = BodyProgress::Timeout(Duration::from_secs(secs));
        BodyPolicy { progress, limit }
    }

    fn min_rate(bytes_per_sec: u64, grace_secs: u64, limit: ByteUnit) -> BodyPolicy {
        let rate = Rate { bytes: bytes_per_sec, per: Duration::from_secs(1) };
        let progress = BodyProgress::MinRate { rate, grace: Duration::from_secs(grace_secs) };
        BodyPolicy { progress, limit }
    }

    fn unlimited(limit: ByteUnit) -> BodyPolicy {
        BodyPolicy { progress: BodyProgress::Unlimited, limit }
    }

    /// Polls `future` exactly once, with a real waker.
    async fn poll_once<F: Future + Unpin>(future: &mut F) -> Poll<F::Output> {
        poll_fn(|cx| Poll::Ready(Pin::new(&mut *future).poll(cx))).await
    }

    /// The policy error `result` failed with, or a panic if it didn't.
    fn policy_error<T: std::fmt::Debug>(result: &io::Result<T>) -> BodyPolicyError {
        let error = result.as_ref().expect_err("expected a body policy failure");
        let policy_error = BodyPolicyError::from_io(error).expect("expected a body policy error");
        assert_eq!(error.kind(), policy_error.kind());
        *policy_error
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_body_fails_at_deadline_without_a_byte() {
        let (_tx, data) = body(timeout(60, 1.kibibytes()));
        let origin = Instant::now();

        // Await the read in a task so that only the deadline timer can wake it.
        let read = tokio::spawn(data.open(1.kibibytes()).into_bytes());
        tokio::task::yield_now().await;
        assert!(!read.is_finished());

        advance(Duration::from_secs(59)).await;
        tokio::task::yield_now().await;
        assert!(!read.is_finished(), "failed before the deadline");

        advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert!(read.is_finished(), "not cut off at the deadline");
        assert_eq!(Instant::now() - origin, Duration::from_secs(60));

        let result = read.await.unwrap();
        assert_eq!(policy_error(&result), BodyPolicyError::Timeout {
            timeout: Duration::from_secs(60),
            elapsed: Duration::from_secs(60),
            received: 0,
        });
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_is_not_extended_by_trickled_bytes() {
        let (tx, data) = body(timeout(60, 1.kibibytes()));
        let mut read = Box::pin(data.open(1.kibibytes()).into_bytes());

        send(&tx, b"a");
        assert!(poll_once(&mut read).await.is_pending());

        advance(Duration::from_secs(30)).await;
        send(&tx, b"b");
        assert!(poll_once(&mut read).await.is_pending());

        advance(Duration::from_secs(29)).await;
        send(&tx, b"c");
        assert!(poll_once(&mut read).await.is_pending());

        advance(Duration::from_secs(1)).await;
        let result = poll_once(&mut read).await;
        let Poll::Ready(result) = result else { panic!("deadline was extended") };
        assert_eq!(policy_error(&result), BodyPolicyError::Timeout {
            timeout: Duration::from_secs(60),
            elapsed: Duration::from_secs(60),
            received: 3,
        });
    }

    #[tokio::test(start_paused = true)]
    async fn min_rate_extends_deadline_in_proportion_to_bytes() {
        // 10 bytes per second after 5 seconds of grace.
        let (tx, data) = body(min_rate(10, 5, 1.kibibytes()));
        let mut read = Box::pin(data.open(1.kibibytes()).into_bytes());
        assert!(poll_once(&mut read).await.is_pending());

        // 100 bytes buy 10 seconds: the deadline moves from 5s to 15s.
        advance(Duration::from_secs(4)).await;
        send(&tx, &[0; 100]);
        assert!(poll_once(&mut read).await.is_pending());

        advance(Duration::from_secs(10)).await;
        assert!(poll_once(&mut read).await.is_pending(), "deadline not extended");

        // Another 50 bytes buy 5 more seconds: 20s.
        send(&tx, &[0; 50]);
        assert!(poll_once(&mut read).await.is_pending());
        advance(Duration::from_secs(5)).await;
        assert!(poll_once(&mut read).await.is_pending(), "deadline not extended");

        advance(Duration::from_secs(1)).await;
        let Poll::Ready(result) = poll_once(&mut read).await else { panic!("not cut off") };
        assert_eq!(policy_error(&result), BodyPolicyError::MinRate {
            rate: Rate { bytes: 10, per: Duration::from_secs(1) },
            grace: Duration::from_secs(5),
            elapsed: Duration::from_secs(20),
            received: 150,
        });
    }

    #[tokio::test(start_paused = true)]
    async fn min_rate_grace_period_is_honoured() {
        let (tx, data) = body(min_rate(1024, 30, 1.kibibytes()));
        let mut read = Box::pin(data.open(1.kibibytes()).into_bytes());

        // Nothing at all is required during the grace period.
        advance(Duration::from_secs(29)).await;
        assert!(poll_once(&mut read).await.is_pending(), "failed during grace");

        // A single byte at 1KiB/s extends the deadline past the grace period
        // by under a millisecond: grace is where the rate starts to count.
        send(&tx, b"a");
        assert!(poll_once(&mut read).await.is_pending());
        advance(Duration::from_secs(1)).await;
        assert!(poll_once(&mut read).await.is_pending(), "byte did not extend deadline");
        advance(Duration::from_millis(1)).await;
        let Poll::Ready(result) = poll_once(&mut read).await else { panic!("not cut off") };
        assert!(matches!(policy_error(&result), BodyPolicyError::MinRate { received: 1, .. }));
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_min_rate_body_is_cut_off_by_timer() {
        let (tx, data) = body(min_rate(10, 5, 1.kibibytes()));
        send(&tx, &[0; 100]);

        let read = tokio::spawn(data.open(1.kibibytes()).into_bytes());
        tokio::task::yield_now().await;

        // The timer must have been re-armed from 5s to 15s by the 100 bytes.
        advance(Duration::from_secs(14)).await;
        tokio::task::yield_now().await;
        assert!(!read.is_finished(), "timer not re-armed for received bytes");

        advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert!(read.is_finished(), "not cut off at the extended deadline");
        assert!(matches!(
            policy_error(&read.await.unwrap()),
            BodyPolicyError::MinRate { received: 100, .. }
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn hard_limit_applies_beneath_a_larger_guard_limit() {
        let (tx, data) = body(timeout(60, 10.bytes()));
        send(&tx, &[0; 8]);
        send(&tx, &[0; 8]);

        let result = data.open(100.bytes()).into_bytes().await;
        assert_eq!(policy_error(&result), BodyPolicyError::LimitExceeded {
            limit: 10.bytes(),
            elapsed: Duration::ZERO,
            received: 16,
        });
    }

    #[tokio::test(start_paused = true)]
    async fn guard_limit_still_caps_when_stricter() {
        let (tx, data) = body(timeout(60, 100.bytes()));
        send(&tx, &[0; 10]);
        drop(tx);

        let bytes = data.open(4.bytes()).into_bytes().await.unwrap();
        assert_eq!(bytes.len(), 4);
        assert!(!bytes.is_complete());

        let (tx, data) = body(timeout(60, 100.bytes()));
        send(&tx, &[0; 10]);
        drop(tx);

        let bytes = data.open(10.bytes()).into_bytes().await.unwrap();
        assert_eq!(bytes.len(), 10);
        assert!(bytes.is_complete());
    }

    #[tokio::test(start_paused = true)]
    async fn unlimited_progress_never_times_out() {
        let (tx, data) = body(unlimited(1.kibibytes()));
        let mut read = Box::pin(data.open(1.kibibytes()).into_bytes());
        assert!(poll_once(&mut read).await.is_pending());

        advance(Duration::from_secs(365 * 24 * 60 * 60)).await;
        assert!(poll_once(&mut read).await.is_pending(), "unlimited body timed out");

        send(&tx, b"hello");
        drop(tx);
        let Poll::Ready(result) = poll_once(&mut read).await else { panic!("not read") };
        let bytes = result.unwrap();
        assert_eq!(bytes.value, b"hello");
        assert!(bytes.is_complete());
    }

    #[tokio::test(start_paused = true)]
    async fn unlimited_progress_still_applies_hard_limit() {
        let (tx, data) = body(unlimited(4.bytes()));
        send(&tx, b"hello");

        let result = data.open(100.bytes()).into_bytes().await;
        assert!(matches!(
            policy_error(&result),
            BodyPolicyError::LimitExceeded { received: 5, .. }
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn ended_body_is_not_failed_after_its_deadline() {
        let (tx, data) = body(timeout(60, 1.kibibytes()));
        send(&tx, b"hello");
        drop(tx);

        let mut stream = data.open(1.kibibytes());
        let mut buf = Vec::new();
        assert_eq!(stream.read_to_end(&mut buf).await.unwrap(), 5);

        advance(Duration::from_secs(120)).await;
        assert_eq!(stream.read(&mut [0; 8]).await.unwrap(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn failure_is_terminal_across_reads() {
        let (tx, data) = body(timeout(60, 1.kibibytes()));
        send(&tx, b"hello");

        let mut stream = data.open(1.kibibytes());
        let mut buf = [0; 8];
        assert_eq!(stream.read(&mut buf).await.unwrap(), 5);

        advance(Duration::from_secs(60)).await;
        let first = policy_error(&stream.read(&mut buf).await);
        assert!(matches!(first, BodyPolicyError::Timeout { received: 5, .. }));

        // Time passing and bytes arriving change nothing once failed.
        advance(Duration::from_secs(60)).await;
        send(&tx, b"world");
        assert_eq!(policy_error(&stream.read(&mut buf).await), first);
        assert_eq!(policy_error(&stream.read(&mut buf).await), first);
    }

    #[tokio::test(start_paused = true)]
    async fn failure_during_peek_is_terminal_for_open() {
        let (tx, mut data) = body(timeout(60, 1.kibibytes()));
        send(&tx, b"hi");

        // `peek` swallows the error, returning what it has.
        let mut peek = Box::pin(data.peek(4));
        assert!(poll_once(&mut peek).await.is_pending());
        advance(Duration::from_secs(60)).await;
        let Poll::Ready(peeked) = poll_once(&mut peek).await else { panic!("not cut off") };
        assert_eq!(peeked, b"hi");
        drop(peek);

        assert!(!data.peek_complete());
        let result = data.open(1.kibibytes()).into_bytes().await;
        assert!(matches!(policy_error(&result), BodyPolicyError::Timeout { received: 2, .. }));
    }

    #[tokio::test(start_paused = true)]
    async fn multipart_field_inherits_body_failure() {
        let (tx, data) = body(timeout(60, 1.kibibytes()));
        send(&tx, b"--X\r\nContent-Disposition: form-data; name=\"f\"; filename=\"a\"\r\n\r\nhello");

        let mut multipart = multer::Multipart::with_reader(data.open(1.kibibytes()), "X");
        let field = multipart.next_field().await.unwrap().expect("a field");

        // The field's data ends only at the next boundary, which never comes.
        let mut read = Box::pin(Data::from_field(field).open(1.kibibytes()).into_bytes());
        assert!(poll_once(&mut read).await.is_pending());
        advance(Duration::from_secs(60)).await;
        let Poll::Ready(result) = poll_once(&mut read).await else { panic!("not cut off") };
        assert!(matches!(policy_error(&result), BodyPolicyError::Timeout { .. }));
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    }

    #[tokio::test(start_paused = true)]
    async fn local_data_counts_toward_hard_limit() {
        let data = Data::local(vec![0; 20], timeout(60, 10.bytes()));
        let result = data.open(100.bytes()).into_bytes().await;
        assert!(matches!(
            policy_error(&result),
            BodyPolicyError::LimitExceeded { received: 20, .. }
        ));

        let data = Data::local(vec![0; 10], timeout(60, 10.bytes()));
        let bytes = data.open(100.bytes()).into_bytes().await.unwrap();
        assert!(bytes.is_complete());
        assert_eq!(bytes.len(), 10);
    }

    #[test]
    fn errors_are_distinct_and_downcastable() {
        let policy_error = BodyPolicyError::Timeout {
            timeout: Duration::from_secs(60),
            elapsed: Duration::from_secs(61),
            received: 42,
        };

        let error = io::Error::from(policy_error);
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(BodyPolicyError::from_io(&error), Some(&policy_error));
        assert_eq!(policy_error.elapsed(), Duration::from_secs(61));
        assert_eq!(policy_error.received(), 42);

        // Through arbitrary wrapping, including multer's.
        let wrapped = io::Error::other(io::Error::other(error));
        assert_eq!(BodyPolicyError::from_io(&wrapped), Some(&policy_error));
        let multer_error = multer::Error::StreamReadFailed(Box::new(wrapped));
        let wrapped = io::Error::other(multer_error);
        assert_eq!(BodyPolicyError::from_io(&wrapped), Some(&policy_error));

        let size_error = BodyPolicyError::LimitExceeded {
            limit: 8.mebibytes(),
            elapsed: Duration::from_secs(1),
            received: 9.mebibytes().as_u64(),
        };
        assert_eq!(io::Error::from(size_error).kind(), io::ErrorKind::FileTooLarge);

        let rate_error = BodyPolicyError::MinRate {
            rate: Rate { bytes: 1024, per: Duration::from_secs(1) },
            grace: Duration::from_secs(15),
            elapsed: Duration::from_secs(20),
            received: 100,
        };
        assert_eq!(io::Error::from(rate_error).kind(), io::ErrorKind::TimedOut);

        assert!(BodyPolicyError::from_io(&io::Error::other("unrelated")).is_none());
        assert!(BodyPolicyError::from_io(&io::Error::from(io::ErrorKind::TimedOut)).is_none());
    }

    #[test]
    fn time_for_rate() {
        let rate = Rate { bytes: 1024, per: Duration::from_secs(1) };
        assert_eq!(time_for(rate, 0), Duration::ZERO);
        assert_eq!(time_for(rate, 1024), Duration::from_secs(1));
        assert_eq!(time_for(rate, 512), Duration::from_millis(500));
        assert_eq!(time_for(rate, 10 * 1024), Duration::from_secs(10));

        let rate = Rate { bytes: 1, per: Duration::from_secs(3600) };
        assert_eq!(time_for(rate, u64::MAX), Duration::MAX);
    }
}
