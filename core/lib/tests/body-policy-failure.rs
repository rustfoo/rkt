// Body policy failures as the client and the application see them.
//
// Expected to fail until step 1b (route storage and policy resolution), because
// they rely on a route's own body options taking effect:
//
//   stalled_body_under_route_timeout_returns_408                1b
//   trickle_under_route_rate_returns_408                        1b
//   guards_time_out_under_route_policy                          1b
//   guards_reject_oversized_under_route_policy                  1b
//
// Everything else passes today and guards the global policy's behaviour:
//
//   stalled_body_returns_408
//   trickle_under_global_rate_returns_408
//   payload_bytes_earn_rate_time_but_chunk_metadata_does_not
//   trailers_do_not_earn_rate_time
//   chunked_body_over_limit_returns_413
//   raw_consumers_see_timeout / _rate / _limit
//   raw_consumers_do_not_confuse_malformed_input_with_policy_failure
//   raw_consumers_do_not_confuse_peer_disconnect_with_policy_failure
//   guards_time_out_under_global_policy
//   guards_reject_oversized_under_global_policy
//   failed_body_stays_failed
//
// Empty data frames exist only in HTTP/2, so the "empty frames earn nothing"
// case is covered at the stream level by the unit tests in `data/policy.rs`.
//
// `Data::peek()` swallows non-policy read errors (malformed input, disconnects),
// so those two raw-consumer tests cover `open()` only. This is existing
// behaviour, not part of 1b or 1c.
//
// Real sockets and real time are used; see `body-policy-support`.

#[macro_use]
extern crate rkt;

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rkt::data::{BodyPolicyError, Data, ToByteUnit};
use rkt::form::Form;
use rkt::fs::TempFile;
use rkt::http::Status;
use rkt::tokio::io::AsyncReadExt;
use rkt::tokio::time::sleep;
use rkt::{Build, Config, Rocket};

#[path = "body-policy-support/mod.rs"]
mod support;

use support::*;

/// What the application observed, in order.
type Seen = Arc<Mutex<Vec<String>>>;

/// How a read failure presents to an application: which kind of policy failure
/// it is, or that it is not one.
fn classify(error: &io::Error) -> String {
    match BodyPolicyError::from_io(error) {
        Some(BodyPolicyError::Timeout { .. }) => "timeout".into(),
        Some(BodyPolicyError::MinRate { .. }) => "rate".into(),
        Some(BodyPolicyError::LimitExceeded { .. }) => "limit".into(),
        Some(BodyPolicyError::DeclaredTooLarge { .. }) => "declared".into(),
        Some(other) => format!("policy:{other}"),
        None => format!("io:{:?}", error.kind()),
    }
}

fn record(seen: &Seen, what: String) {
    seen.lock().unwrap().push(what);
}

/// Records `cancelled` if dropped before the handler reports: hyper cancels the
/// handler of a request whose client has disconnected.
struct Report<'a>(&'a Seen, bool);

impl<'a> Report<'a> {
    fn new(seen: &'a Seen) -> Self {
        Report(seen, false)
    }

    fn read(mut self, result: io::Result<()>) {
        self.1 = true;
        record(self.0, result.map_or_else(|e| classify(&e), |_| "ok".into()));
    }
}

impl Drop for Report<'_> {
    fn drop(&mut self) {
        if !self.1 {
            record(self.0, "cancelled".into());
        }
    }
}

async fn slurp(data: Data<'_>) -> Result<String, Status> {
    let bytes = data.open(1.mebibytes()).into_bytes().await.map_err(|_: io::Error| Status::BadRequest)?;
    Ok(bytes.len().to_string())
}

async fn peek_then_read(mut data: Data<'_>) -> Result<String, Status> {
    data.peek(100).await;
    slurp(data).await
}

#[post("/slurp", data = "<data>")]
async fn plain(data: Data<'_>) -> Result<String, Status> { slurp(data).await }

#[post("/r_timeout", data = "<data>", body_timeout = "300ms")]
async fn route_timeout(data: Data<'_>) -> Result<String, Status> { slurp(data).await }

#[post("/r_rate", data = "<data>", body_min_rate = "1KiB/s", body_grace = "200ms")]
async fn route_rate(data: Data<'_>) -> Result<String, Status> { slurp(data).await }

// Raw consumers report what they saw instead of failing the request.

#[post("/raw/data", data = "<data>")]
async fn raw_data(data: Data<'_>, seen: &rkt::State<Seen>) -> &'static str {
    let report = Report::new(seen);
    report.read(data.open(1.mebibytes()).into_bytes().await.map(|_| ()));
    "done"
}

#[post("/raw/stream", data = "<data>")]
async fn raw_stream(data: Data<'_>, seen: &rkt::State<Seen>) -> &'static str {
    let report = Report::new(seen);
    let mut stream = data.open(1.mebibytes());
    let mut buffer = [0u8; 64];
    let result = loop {
        match stream.read(&mut buffer).await {
            Ok(0) => break Ok(()),
            Ok(_) => continue,
            Err(e) => break Err(e),
        }
    };

    report.read(result);
    "done"
}

#[post("/raw/peek", data = "<data>")]
async fn raw_peek(mut data: Data<'_>, seen: &rkt::State<Seen>) -> &'static str {
    let report = Report::new(seen);
    data.peek(100).await;
    report.read(data.open(1.mebibytes()).into_bytes().await.map(|_| ()));
    "done"
}

#[post("/sticky", data = "<data>")]
async fn sticky(data: Data<'_>, seen: &rkt::State<Seen>) -> &'static str {
    let mut stream = data.open(1.mebibytes());
    let mut buffer = [0u8; 64];
    let mut errors = vec![];
    loop {
        match stream.read(&mut buffer).await {
            Ok(0) => break errors.push(None),
            Ok(_) => continue,
            Err(e) => break errors.push(BodyPolicyError::from_io(&e).copied()),
        }
    }

    // Every read after the failure must fail with the same error.
    for _ in 0..3 {
        match stream.read(&mut buffer).await {
            Err(e) => errors.push(BodyPolicyError::from_io(&e).copied()),
            Ok(_) => errors.push(None),
        }
    }

    let first = errors[0];
    match first.is_some() && errors.iter().all(|e| *e == first) {
        true => record(seen, "sticky".into()),
        false => record(seen, format!("not sticky: {errors:?}")),
    }

    "done"
}

// One route per guard, for each of the global, route-timeout, and route-limit
// policies: `/g/*`, `/rt/*`, and `/rl/*`.

#[derive(FromForm)]
struct Input { #[allow(dead_code)] value: String }

#[derive(FromForm)]
struct Upload<'r> { #[allow(dead_code)] file: TempFile<'r> }

#[post("/g/form", data = "<_d>")]
fn g_form(_d: Form<Input>) {}
#[post("/g/file", data = "<_d>")]
fn g_file(_d: TempFile<'_>) {}
#[post("/g/multipart", data = "<_d>")]
fn g_multipart(_d: Form<Upload<'_>>) {}
#[post("/g/peek", data = "<d>")]
async fn g_peek(d: Data<'_>) -> Result<String, Status> { peek_then_read(d).await }

#[post("/rt/form", data = "<_d>", body_timeout = "300ms")]
fn rt_form(_d: Form<Input>) {}
#[post("/rt/file", data = "<_d>", body_timeout = "300ms")]
fn rt_file(_d: TempFile<'_>) {}
#[post("/rt/multipart", data = "<_d>", body_timeout = "300ms")]
fn rt_multipart(_d: Form<Upload<'_>>) {}
#[post("/rt/peek", data = "<d>", body_timeout = "300ms")]
async fn rt_peek(d: Data<'_>) -> Result<String, Status> { peek_then_read(d).await }

#[post("/rl/form", data = "<_d>", body_limit = "16B")]
fn rl_form(_d: Form<Input>) {}
#[post("/rl/file", data = "<_d>", body_limit = "16B")]
fn rl_file(_d: TempFile<'_>) {}
#[post("/rl/multipart", data = "<_d>", body_limit = "16B")]
fn rl_multipart(_d: Form<Upload<'_>>) {}
#[post("/rl/peek", data = "<d>", body_limit = "16B")]
async fn rl_peek(d: Data<'_>) -> Result<String, Status> { peek_then_read(d).await }

#[cfg(feature = "json")]
mod json_routes {
    use super::*;
    use rkt::serde::json::Json;

    #[post("/g/json", data = "<_d>")]
    pub fn g_json(_d: Json<String>) {}
    #[post("/rt/json", data = "<_d>", body_timeout = "300ms")]
    pub fn rt_json(_d: Json<String>) {}
    #[post("/rl/json", data = "<_d>", body_limit = "16B")]
    pub fn rl_json(_d: Json<String>) {}
}

fn rocket(config: Config, seen: &Seen) -> Rocket<Build> {
    let rocket = rkt::custom(config).manage(seen.clone()).mount("/", routes![
        plain, route_timeout, route_rate, raw_data, raw_stream, raw_peek, sticky,
        g_form, g_file, g_multipart, g_peek,
        rt_form, rt_file, rt_multipart, rt_peek,
        rl_form, rl_file, rl_multipart, rl_peek,
    ]);

    #[cfg(feature = "json")]
    let rocket = rocket.mount("/", routes![
        json_routes::g_json, json_routes::rt_json, json_routes::rl_json,
    ]);

    rocket
}

fn strict() -> Config {
    Config { body_timeout: duration(300), body_limit: 16.bytes(), ..config() }
}

fn timeout_only() -> Config {
    Config { body_timeout: duration(300), ..config() }
}

fn limit_only() -> Config {
    Config { body_limit: 16.bytes(), ..config() }
}

fn new_rocket(config: Config) -> (Rocket<Build>, Seen) {
    let seen = Seen::default();
    (rocket(config, &seen), seen)
}

/// Waits for the application to report, and returns what it saw.
async fn observed(seen: &Seen) -> Vec<String> {
    for _ in 0..60 {
        if !seen.lock().unwrap().is_empty() {
            break;
        }

        sleep(Duration::from_millis(50)).await;
    }

    seen.lock().unwrap().clone()
}

#[rkt::async_test]
async fn stalled_body_returns_408() {
    let port = launch(new_rocket(strict()).0).await;
    let stalled = vec![head("POST", "/slurp", 10), send("hello"), wait(1000), send("world")];
    assert_eq!(status(port, stalled).await, Some(408));

    // A body that never starts is stalled too.
    let silent = vec![head("POST", "/slurp", 10), wait(1000)];
    assert_eq!(status(port, silent).await, Some(408));
}

#[rkt::async_test]
async fn stalled_body_under_route_timeout_returns_408() {
    let port = launch(new_rocket(config()).0).await;
    let stalled = vec![head("POST", "/r_timeout", 10), send("hello"), wait(1000), send("world")];
    assert_eq!(status(port, stalled).await, Some(408));
}

/// One byte every 100ms for three seconds, of a body declared as 100 bytes.
fn trickle(path: &str) -> Vec<Step> {
    let mut steps = vec![head("POST", path, 100)];
    for _ in 0..30 {
        steps.push(send("x"));
        steps.push(wait(100));
    }

    steps
}

fn global_rate() -> Config {
    Config { body_min_rate: Some("1KiB/s".parse().unwrap()), body_grace: duration(200), ..config() }
}

#[rkt::async_test]
async fn trickle_under_global_rate_returns_408() {
    let port = launch(new_rocket(global_rate()).0).await;
    assert_eq!(status(port, trickle("/slurp")).await, Some(408));
}

#[rkt::async_test]
async fn trickle_under_route_rate_returns_408() {
    let port = launch(new_rocket(config()).0).await;
    assert_eq!(status(port, trickle("/r_rate")).await, Some(408));
}

#[rkt::async_test]
async fn payload_bytes_earn_rate_time_but_chunk_metadata_does_not() {
    let config = Config {
        body_min_rate: Some("10B/s".parse().unwrap()),
        body_grace: duration(300),
        ..config()
    };

    let port = launch(new_rocket(config).0).await;

    // Control: 20 payload bytes per second against a 10B/s minimum is enough.
    let mut payload = vec![head_chunked("POST", "/slurp")];
    for _ in 0..20 {
        payload.push(chunk("x"));
        payload.push(wait(50));
    }
    payload.push(end());
    assert_eq!(status(port, payload).await, Some(200), "payload bytes should earn time");

    // The same cadence of bytes spent on one never-finished chunk header is
    // not payload, and earns nothing.
    let mut metadata = vec![head_chunked("POST", "/slurp"), send("5;")];
    for _ in 0..40 {
        metadata.push(send("a"));
        metadata.push(wait(50));
    }
    assert_eq!(status(port, metadata).await, Some(408), "chunk metadata should not earn time");
}

#[rkt::async_test]
async fn trailers_do_not_earn_rate_time() {
    let port = launch(new_rocket(global_rate()).0).await;

    // The payload is complete; trailer bytes then arrive without ever finishing.
    let mut trailers = vec![
        head_with("POST", "/slurp", "Transfer-Encoding: chunked\r\nTrailer: X-Trailer"),
        chunk("hello"),
        send("0\r\nX-Trailer: "),
    ];
    for _ in 0..40 {
        trailers.push(send("a"));
        trailers.push(wait(50));
    }

    assert_eq!(status(port, trailers).await, Some(408));
}

#[rkt::async_test]
async fn chunked_body_over_limit_returns_413() {
    let port = launch(new_rocket(strict()).0).await;

    // No `Content-Length`: only counting the bytes can enforce the limit.
    let over = vec![head_chunked("POST", "/slurp"), chunk([b'x'; 10]), chunk([b'x'; 10]), end()];
    assert_eq!(status(port, over).await, Some(413));
}

async fn raw_routes(
    config: Config,
    paths: &[&str],
    steps: impl Fn(&str) -> Vec<Step>,
) -> Vec<(String, Vec<String>)> {
    let mut results = vec![];
    for path in paths {
        let (rocket, seen) = new_rocket(config.clone());
        let port = launch(rocket).await;
        let _client = {
            let mut client = Client::connect(port).await;
            let _script = client.play(steps(path));
            client
        };

        results.push((path.to_string(), observed(&seen).await));
    }

    results
}

const RAW: [&str; 3] = ["/raw/data", "/raw/stream", "/raw/peek"];

fn assert_all_saw(results: Vec<(String, Vec<String>)>, expected: &str) {
    for (path, seen) in results {
        assert_eq!(seen, vec![expected.to_string()], "{path}");
    }
}

#[rkt::async_test]
async fn raw_consumers_see_timeout() {
    let stalled = |path: &str| vec![head("POST", path, 10), send("hello"), wait(1000)];
    assert_all_saw(raw_routes(strict(), &RAW, stalled).await, "timeout");
}

#[rkt::async_test]
async fn raw_consumers_see_rate() {
    assert_all_saw(raw_routes(global_rate(), &RAW, |path| trickle(path)).await, "rate");
}

#[rkt::async_test]
async fn raw_consumers_see_limit() {
    let over = |path: &str| vec![head_chunked("POST", path), chunk([b'x'; 10]), chunk([b'x'; 10]), end()];
    assert_all_saw(raw_routes(strict(), &RAW, over).await, "limit");
}

#[rkt::async_test]
async fn raw_consumers_do_not_confuse_malformed_input_with_policy_failure() {
    let malformed = |path: &str| vec![head_chunked("POST", path), chunk("hello"), send("zz\r\n")];
    // `Data::peek()` swallows a read error, so only `open()` can be checked.
    for (path, seen) in raw_routes(strict(), &RAW[..2], malformed).await {
        assert!(matches!(&seen[..], [s] if s.starts_with("io:")), "{path}: saw {seen:?}");
    }
}

#[rkt::async_test]
async fn raw_consumers_do_not_confuse_peer_disconnect_with_policy_failure() {
    let disconnect = |path: &str| vec![head("POST", path, 10), send("hello"), wait(50), close()];
    // `Data::peek()` swallows a read error, so only `open()` can be checked.
    for (path, seen) in raw_routes(strict(), &RAW[..2], disconnect).await {
        let expected = |s: &String| s.starts_with("io:") || s == "cancelled";
        assert!(matches!(&seen[..], [s] if expected(s)), "{path}: saw {seen:?}");
    }
}

#[rkt::async_test]
async fn failed_body_stays_failed() {
    let (rocket, seen) = new_rocket(strict());
    let port = launch(rocket).await;

    let stalled = vec![head("POST", "/sticky", 10), send("hello"), wait(1000)];
    let _ = request(port, stalled).await;
    assert_eq!(observed(&seen).await, vec!["sticky"], "after a timeout");

    seen.lock().unwrap().clear();
    let over = vec![head_chunked("POST", "/sticky"), chunk([b'x'; 10]), chunk([b'x'; 10]), end()];
    let _ = request(port, over).await;
    assert_eq!(observed(&seen).await, vec!["sticky"], "after exceeding the limit");
}

/// The guards under test, with a request whose body stalls and one that is
/// over a 16 byte limit for each.
struct Kind {
    name: &'static str,
    content_type: &'static str,
    /// A valid beginning of a body, and the whole body padded to `n` bytes.
    start: &'static str,
    whole: fn(usize) -> Vec<u8>,
}

fn multipart(payload: usize) -> Vec<u8> {
    format!(
        "--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.txt\"\r\n\
        Content-Type: text/plain\r\n\r\n{}\r\n--B--\r\n",
        "a".repeat(payload)
    ).into_bytes()
}

fn kinds() -> Vec<Kind> {
    let mut kinds = vec![
        Kind {
            name: "form",
            content_type: "application/x-www-form-urlencoded",
            start: "value=aaaa",
            whole: |n| format!("value={}", "a".repeat(n)).into_bytes(),
        },
        Kind { name: "file", content_type: "application/octet-stream", start: "aaaa",
            whole: |n| vec![b'a'; n] },
        Kind {
            name: "multipart",
            content_type: "multipart/form-data; boundary=B",
            start: "--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.txt\"\r\n\
                Content-Type: text/plain\r\n\r\naaaa",
            whole: multipart,
        },
        Kind { name: "peek", content_type: "application/octet-stream", start: "aaaa",
            whole: |n| vec![b'a'; n] },
    ];

    if cfg!(feature = "json") {
        kinds.push(Kind {
            name: "json",
            content_type: "application/json",
            start: "\"aaaa",
            whole: |n| format!("\"{}\"", "a".repeat(n)).into_bytes(),
        });
    }

    kinds
}

/// Sends each kind's stalled (or oversized) body to `{prefix}/{kind}` and
/// reports every kind that did not get `expected`.
async fn check_guards(port: u16, prefix: &str, stall: bool, expected: u16) -> Vec<String> {
    let attempts = kinds().into_iter().map(|kind| async move {
        let path = format!("{prefix}/{}", kind.name);
        let headers = format!("Content-Type: {}", kind.content_type);
        let steps = match stall {
            true => vec![
                head_with("POST", &path, &format!("{headers}\r\nContent-Length: 1000")),
                send(kind.start),
                wait(1200),
            ],
            false => vec![
                head_with("POST", &path, &format!("{headers}\r\nTransfer-Encoding: chunked")),
                chunk((kind.whole)(100)),
                end(),
            ],
        };

        match status(port, steps).await {
            Some(status) if status == expected => None,
            other => Some(format!("{path}: expected {expected}, got {other:?}")),
        }
    });

    rkt::futures::future::join_all(attempts).await.into_iter().flatten().collect()
}

fn assert_no_failures(failures: Vec<String>) {
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[rkt::async_test]
async fn guards_time_out_under_global_policy() {
    let port = launch(new_rocket(timeout_only()).0).await;
    assert_no_failures(check_guards(port, "/g", true, 408).await);
}

#[rkt::async_test]
async fn guards_time_out_under_route_policy() {
    let port = launch(new_rocket(config()).0).await;
    assert_no_failures(check_guards(port, "/rt", true, 408).await);
}

#[rkt::async_test]
async fn guards_reject_oversized_under_global_policy() {
    let port = launch(new_rocket(limit_only()).0).await;
    assert_no_failures(check_guards(port, "/g", false, 413).await);
}

#[rkt::async_test]
async fn guards_reject_oversized_under_route_policy() {
    let port = launch(new_rocket(config()).0).await;
    assert_no_failures(check_guards(port, "/rl", false, 413).await);
}
