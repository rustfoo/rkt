// Policy resolution: how a route's body options combine with the global policy.
//
// Expected to fail until step 1b (route storage and policy resolution), which
// makes the route attribute's body options take effect. Until then every route
// runs under the global policy alone:
//
//   route_timeout_replaces_global_rate           1b
//   route_rate_replaces_global_deadline          1b
//   route_rate_inherits_global_grace             1b
//   route_grace_overrides_global_grace           1b
//   route_unlimited_disables_progress            1b
//   route_unlimited_keeps_its_limit              1b
//   route_limit_overrides_only_size_chunked      1b
//   route_limit_overrides_only_size_declared     1b  (see note below)
//   hard_limit_below_named_limit_wins            1b
//
// Passing now, as regression guards for 1b:
//
//   route_without_options_inherits_global_policy
//   route_limit_leaves_global_progress_policy
//   named_limit_below_hard_limit_wins
//   bodies_within_both_limits_are_accepted
//
// Note on `route_limit_overrides_only_size_declared`: a `Content-Length` over
// the *global* limit is rejected before routing today, which would also reject
// a route that raises its own limit above the global one. 1b has to defer that
// early rejection until the route, and so its limit, is known.
//
// Option spellings in route attributes are compile-time literals, so the
// durations below are fixed; the global configuration varies per test.

#[macro_use]
extern crate rkt;

use std::io;

use rkt::data::{Data, ToByteUnit};
use rkt::form::Form;
use rkt::fs::TempFile;
use rkt::http::{ContentType, Status};
use rkt::local::blocking::Client as LocalClient;
use rkt::{Build, Config, Rocket};

#[path = "body-policy-support/mod.rs"]
mod support;

use support::*;

/// Reads the whole body, answering with its length. A body that fails is a
/// guard failure; the lifecycle answers it with the policy's status.
async fn slurp(data: Data<'_>) -> Result<String, Status> {
    let bytes = data.open(1.mebibytes()).into_bytes().await.map_err(|_: io::Error| Status::BadRequest)?;
    Ok(bytes.len().to_string())
}

#[post("/inherit", data = "<data>")]
async fn inherit(data: Data<'_>) -> Result<String, Status> { slurp(data).await }

#[post("/timeout", data = "<data>", body_timeout = "2s")]
async fn timeout(data: Data<'_>) -> Result<String, Status> { slurp(data).await }

#[post("/rate", data = "<data>", body_min_rate = "1KiB/s")]
async fn rate(data: Data<'_>) -> Result<String, Status> { slurp(data).await }

#[post("/rate_grace", data = "<data>", body_min_rate = "1KiB/s", body_grace = "2s")]
async fn rate_grace(data: Data<'_>) -> Result<String, Status> { slurp(data).await }

#[post("/unlimited", data = "<data>", body_policy = "unlimited", body_limit = "16B")]
async fn unlimited(data: Data<'_>) -> Result<String, Status> { slurp(data).await }

#[post("/limit", data = "<data>", body_limit = "1KiB")]
async fn limit(data: Data<'_>) -> Result<String, Status> { slurp(data).await }

fn mounted(config: Config) -> Rocket<Build> {
    rkt::custom(config)
        .mount("/", routes![inherit, timeout, rate, rate_grace, unlimited, limit])
}

fn global(timeout_ms: Option<u64>, limit: rkt::data::ByteUnit) -> Config {
    Config {
        body_timeout: timeout_ms.and_then(duration),
        body_limit: limit,
        ..config()
    }
}

fn global_rate(rate: &str, grace_ms: u64) -> Config {
    Config {
        body_min_rate: Some(rate.parse().unwrap()),
        body_grace: duration(grace_ms),
        ..config()
    }
}

#[rkt::async_test]
async fn route_without_options_inherits_global_policy() {
    let port = launch(mounted(global(Some(300), 16.bytes()))).await;

    // The global deadline applies: the body stalls past it.
    let stalled = vec![head("POST", "/inherit", 10), send("hello"), wait(1000), send("world")];
    assert_eq!(status(port, stalled).await, Some(408));

    // The global limit applies.
    let over = vec![head_chunked("POST", "/inherit"), chunk("0123456789abcdef!"), end()];
    assert_eq!(status(port, over).await, Some(413));

    // A body within both is handled.
    let ok = vec![head("POST", "/inherit", 10), send("0123456789")];
    assert_eq!(status(port, ok).await, Some(200));
}

#[rkt::async_test]
async fn route_timeout_replaces_global_rate() {
    // Globally, 5 bytes buy almost no time beyond the 100ms grace.
    let port = launch(mounted(global_rate("100KiB/s", 100))).await;
    let slow = vec![head("POST", "/timeout", 10), send("hello"), wait(600), send("world")];
    assert_eq!(status(port, slow).await, Some(200), "the route's 2s deadline applies");
}

#[rkt::async_test]
async fn route_rate_replaces_global_deadline() {
    let port = launch(mounted(global(Some(300), 1.mebibytes()))).await;
    let slow = vec![head("POST", "/rate_grace", 10), send("hello"), wait(800), send("world")];
    assert_eq!(status(port, slow).await, Some(200), "the route's rate and grace apply");
}

#[rkt::async_test]
async fn route_rate_inherits_global_grace() {
    // The global rate is negligible, so only the global 200ms grace matters
    // once the route supplies a real rate. 100 bytes at 1KiB/s earn ~100ms.
    let port = launch(mounted(global_rate("1B/s", 200))).await;
    let stalled = vec![head("POST", "/rate", 200), send([b'x'; 100]), wait(1000), send([b'x'; 100])];
    assert_eq!(status(port, stalled).await, Some(408), "grace is the global 200ms, not the default");
}

#[rkt::async_test]
async fn route_grace_overrides_global_grace() {
    // Globally 1KiB/s after 200ms: 100 bytes stalled for 1s would fail.
    let port = launch(mounted(global_rate("1KiB/s", 200))).await;
    let stalled = vec![head("POST", "/rate_grace", 200), send([b'x'; 100]), wait(1000), send([b'x'; 100])];
    assert_eq!(status(port, stalled).await, Some(200), "the route's 2s grace applies");
}

#[rkt::async_test]
async fn route_unlimited_disables_progress() {
    let port = launch(mounted(global(Some(300), 1.mebibytes()))).await;
    let stalled = vec![head("POST", "/unlimited", 10), send("hello"), wait(800), send("world")];
    assert_eq!(status(port, stalled).await, Some(200), "no deadline applies");
}

#[rkt::async_test]
async fn route_unlimited_keeps_its_limit() {
    let port = launch(mounted(global(Some(300), 1.mebibytes()))).await;
    let over = vec![head_chunked("POST", "/unlimited"), chunk("0123456789abcdef!"), end()];
    assert_eq!(status(port, over).await, Some(413), "the route's 16B limit applies");

    let ok = vec![head_chunked("POST", "/unlimited"), chunk("0123456789abcdef"), end()];
    assert_eq!(status(port, ok).await, Some(200));
}

#[rkt::async_test]
async fn route_limit_overrides_only_size_chunked() {
    let port = launch(mounted(global(Some(300), 16.bytes()))).await;

    let within = vec![head_chunked("POST", "/limit"), chunk([b'x'; 100]), end()];
    assert_eq!(status(port, within).await, Some(200), "the route's 1KiB limit applies");

    let over = vec![head_chunked("POST", "/limit"), chunk([b'x'; 2048]), end()];
    assert_eq!(status(port, over).await, Some(413));
}

#[rkt::async_test]
async fn route_limit_overrides_only_size_declared() {
    let port = launch(mounted(global(Some(300), 16.bytes()))).await;

    let within = vec![head("POST", "/limit", 100), send([b'x'; 100])];
    assert_eq!(status(port, within).await, Some(200), "declared length is within the route's limit");

    let over = vec![head("POST", "/limit", 2048), send([b'x'; 2048])];
    assert_eq!(status(port, over).await, Some(413));
}

#[rkt::async_test]
async fn route_limit_leaves_global_progress_policy() {
    let port = launch(mounted(global(Some(300), 16.bytes()))).await;
    let stalled = vec![head("POST", "/limit", 10), send("hello"), wait(1000), send("world")];
    assert_eq!(status(port, stalled).await, Some(408), "the global deadline still applies");
}

// The stricter-of rule, through the local client. Each guard has a named limit
// (`bytes`, `form`, `file`, `json`); the route's `body_limit` is the other.

#[derive(FromForm)]
struct Input { #[allow(dead_code)] value: String }


#[post("/hard/bytes", data = "<_d>", body_limit = "32B")]
fn hard_bytes(_d: Vec<u8>) -> &'static str { "ok" }

#[post("/hard/form", data = "<_d>", body_limit = "32B")]
fn hard_form(_d: Form<Input>) -> &'static str { "ok" }

#[post("/hard/file", data = "<_d>", body_limit = "32B")]
fn hard_file(_d: TempFile<'_>) -> &'static str { "ok" }

#[post("/loose/bytes", data = "<_d>", body_limit = "1KiB")]
fn loose_bytes(_d: Vec<u8>) -> &'static str { "ok" }

#[post("/loose/form", data = "<_d>", body_limit = "1KiB")]
fn loose_form(_d: Form<Input>) -> &'static str { "ok" }

#[post("/loose/file", data = "<_d>", body_limit = "1KiB")]
fn loose_file(_d: TempFile<'_>) -> &'static str { "ok" }

#[cfg(feature = "json")]
#[post("/hard/json", data = "<_d>", body_limit = "32B")]
fn hard_json(_d: rkt::serde::json::Json<String>) -> &'static str { "ok" }

#[cfg(feature = "json")]
#[post("/loose/json", data = "<_d>", body_limit = "1KiB")]
fn loose_json(_d: rkt::serde::json::Json<String>) -> &'static str { "ok" }

fn guard_client(config: Config) -> LocalClient {
    let rocket = rkt::custom(config).mount("/", routes![
        hard_bytes, hard_form, hard_file, loose_bytes, loose_form, loose_file,
    ]);

    #[cfg(feature = "json")]
    let rocket = rocket.mount("/", routes![hard_json, loose_json]);

    LocalClient::debug(rocket).unwrap()
}

/// A body of at least `len` bytes that every guard below accepts as input.
fn body_for(guard: &str, len: usize) -> (ContentType, String) {
    match guard {
        "form" => (ContentType::Form, format!("value={}", "a".repeat(len))),
        "json" => (ContentType::JSON, format!("\"{}\"", "a".repeat(len))),
        _ => (ContentType::Binary, "a".repeat(len)),
    }
}

fn guards() -> Vec<&'static str> {
    let mut guards = vec!["bytes", "form", "file"];
    if cfg!(feature = "json") {
        guards.push("json");
    }

    guards
}

#[test]
fn named_limit_below_hard_limit_wins() {
    // Every named limit is 16 bytes; the route allows 1KiB; the body is 100.
    let limits = rkt::data::Limits::default()
        .limit("bytes", 16.bytes())
        .limit("form", 16.bytes())
        .limit("file", 16.bytes())
        .limit("json", 16.bytes());

    let client = guard_client(Config { limits, ..config() });
    for guard in guards() {
        let (content_type, body) = body_for(guard, 100);
        let response = client.post(format!("/loose/{guard}")).header(content_type).body(body).dispatch();
        // The named limit truncates the body, and strict guards answer a
        // truncated body with 400 or 413 depending on the guard.
        let status = response.status();
        assert!(matches!(status.code, 400 | 413), "{guard}: named limit applies, got {status}");
    }
}

#[test]
fn hard_limit_below_named_limit_wins() {
    // The default named limits are all far above 100 bytes; the route allows 32.
    let client = guard_client(config());
    for guard in guards() {
        let (content_type, body) = body_for(guard, 100);
        let response = client.post(format!("/hard/{guard}")).header(content_type).body(body).dispatch();
        assert_eq!(response.status(), Status::PayloadTooLarge, "{guard}: route's body_limit applies");
    }
}

#[test]
fn bodies_within_both_limits_are_accepted() {
    let client = guard_client(config());
    for guard in guards() {
        let (content_type, body) = body_for(guard, 10);
        let response = client.post(format!("/hard/{guard}")).header(content_type).body(body).dispatch();
        assert_eq!(response.status(), Status::Ok, "{guard}");
    }
}
