// Body policy through the request lifecycle: fairings, data guards, handlers,
// forwarding, and `HEAD` autohandling.
//
// Expected to fail until step 1b (route storage and policy resolution), because
// they need a route's body options to take effect once it is selected:
//
//   route_override_applies_to_data_guard          1b
//   route_override_applies_to_handler             1b
//   forwarding_does_not_restart_the_clock         1b
//   forwarding_keeps_the_received_byte_count      1b
//   head_autohandling_applies_get_route_override  1b
//   head_forwarding_does_not_restart_the_clock    1b
//
// Passing now, as regression guards that 1b must keep passing. Today every
// route runs under the global policy, which is exactly right for these:
//
//   request_fairings_read_under_global_policy
//   forwarding_does_not_revive_a_timed_out_body
//   forwarding_does_not_revive_an_oversized_body
//
// Elapsed time is real: a body that "spent 20s" on its first route is scaled
// down to 600ms here, against a 1s deadline on the next route.

#[macro_use]
extern crate rkt;

use std::sync::atomic::{AtomicUsize, Ordering};

use rkt::data::{Data, FromData, Outcome, ToByteUnit};
use rkt::fairing::AdHoc;
use rkt::http::Status;
use rkt::tokio::time::sleep;
use rkt::{Build, Config, Request, Rocket, State};

#[path = "body-policy-support/mod.rs"]
mod support;

use support::*;

/// Counts requests whose body was read to the end without a policy failure.
#[derive(Default)]
struct Completed(AtomicUsize);

async fn slurp(data: Data<'_>, completed: &State<Completed>) -> Result<String, Status> {
    match data.open(1.mebibytes()).into_bytes().await {
        Ok(bytes) => {
            completed.0.fetch_add(1, Ordering::SeqCst);
            Ok(bytes.len().to_string())
        }
        Err(_) => Err(Status::BadRequest),
    }
}

/// Defines a data guard that peeks at `$peek` bytes of the body, spends
/// `$pause` ms on the route, and forwards the data on, as a guard that cannot
/// handle the request would.
macro_rules! forwarder {
    ($name:ident, $peek:expr, $pause:expr) => {
        struct $name;

        #[rkt::async_trait]
        impl<'r> FromData<'r> for $name {
            type Error = ();

            async fn from_data(_: &'r Request<'_>, mut data: Data<'r>) -> Outcome<'r, Self> {
                data.peek($peek).await;
                sleep(ms($pause)).await;
                Outcome::Forward((data, Status::NotFound))
            }
        }
    };
}

forwarder!(SlowForwarder, 5, 600);
forwarder!(CountingForwarder, 400, 0);
forwarder!(FailedForwarder, 100, 0);

// Request fairings run before routing, so under the global policy.

#[post("/fairing", data = "<data>", body_policy = "unlimited")]
async fn fairing_route(data: Data<'_>, c: &State<Completed>) -> Result<String, Status> {
    slurp(data, c).await
}

// A selected route's override applies to its guards and handler.

#[post("/guard", data = "<_d>", body_timeout = "3s")]
fn guard_route(_d: Vec<u8>, c: &State<Completed>) -> &'static str {
    c.0.fetch_add(1, Ordering::SeqCst);
    "ok"
}

#[post("/handler", data = "<data>", body_timeout = "3s")]
async fn handler_route(data: Data<'_>, c: &State<Completed>) -> Result<String, Status> {
    slurp(data, c).await
}

// Forwarding: the first route spends time (or bytes) and forwards.

#[post("/clock", data = "<_d>", rank = 1, body_timeout = "5s")]
fn clock_first(_d: SlowForwarder) {}

#[post("/clock", data = "<data>", rank = 2, body_timeout = "1s")]
async fn clock_second(data: Data<'_>, c: &State<Completed>) -> Result<String, Status> {
    slurp(data, c).await
}

#[post("/count", data = "<_d>", rank = 1)]
fn count_first(_d: CountingForwarder) {}

#[post("/count", data = "<data>", rank = 2, body_limit = "500B")]
async fn count_second(data: Data<'_>, c: &State<Completed>) -> Result<String, Status> {
    slurp(data, c).await
}

#[post("/revive", data = "<_d>", rank = 1)]
fn revive_first(_d: FailedForwarder) {}

#[post("/revive", data = "<data>", rank = 2, body_timeout = "10s", body_limit = "1MiB")]
async fn revive_second(data: Data<'_>, c: &State<Completed>) -> Result<String, Status> {
    slurp(data, c).await
}

// `HEAD` requests with no `HEAD` route are re-dispatched as `GET`.

#[get("/head", data = "<data>", body_timeout = "1s")]
#[suppress(dubious_payload)]
async fn head_get(data: Data<'_>, c: &State<Completed>) -> Result<String, Status> {
    slurp(data, c).await
}

#[head("/head_fwd", data = "<_d>")]
#[suppress(dubious_payload)]
fn head_fwd_first(_d: SlowForwarder) {}

#[get("/head_fwd", data = "<data>", body_timeout = "1s")]
#[suppress(dubious_payload)]
async fn head_fwd_get(data: Data<'_>, c: &State<Completed>) -> Result<String, Status> {
    slurp(data, c).await
}

fn rocket(config: Config) -> Rocket<Build> {
    rkt::custom(config)
        .manage(Completed::default())
        .attach(AdHoc::on_request("Peek Body", |req, data| Box::pin(async move {
            if req.uri().path() == "/fairing" {
                data.peek(100).await;
            }
        })))
        .mount("/", routes![
            fairing_route, guard_route, handler_route, clock_first, clock_second,
            count_first, count_second, revive_first, revive_second,
            head_get, head_fwd_first, head_fwd_get,
        ])
}

fn global_timeout(timeout_ms: u64) -> Config {
    Config { body_timeout: duration(timeout_ms), ..config() }
}

#[rkt::async_test]
async fn request_fairings_read_under_global_policy() {
    // The route is unlimited, but the fairing reads before the route is known:
    // it waits for 100 bytes and the global 300ms deadline cuts it off.
    let port = launch(rocket(global_timeout(300))).await;
    let stalled = vec![head("POST", "/fairing", 10), send("hello"), wait(1000), send("world")];
    assert_eq!(status(port, stalled).await, Some(408));
}

#[rkt::async_test]
async fn route_override_applies_to_data_guard() {
    let port = launch(rocket(global_timeout(300))).await;
    let slow = vec![head("POST", "/guard", 10), send("hello"), wait(800), send("world")];
    assert_eq!(status(port, slow).await, Some(200), "the guard reads under the route's 3s deadline");
}

#[rkt::async_test]
async fn route_override_applies_to_handler() {
    let port = launch(rocket(global_timeout(300))).await;
    let slow = vec![head("POST", "/handler", 10), send("hello"), wait(800), send("world")];
    assert_eq!(status(port, slow).await, Some(200), "the handler reads under the route's 3s deadline");
}

#[rkt::async_test]
async fn forwarding_does_not_restart_the_clock() {
    // The first route holds the body for 600ms and forwards. The second route's
    // 1s deadline runs from when the body began, so it expires at 1s, not 1.6s.
    let port = launch(rocket(global_timeout(60_000))).await;
    let slow = vec![head("POST", "/clock", 10), send("hello"), wait(1300), send("world")];
    assert_eq!(status(port, slow).await, Some(408));
}

#[rkt::async_test]
async fn forwarding_keeps_the_received_byte_count() {
    // 400 bytes are read by the first route; 300 more arrive for the second,
    // whose 500B limit the 700 total exceed.
    let port = launch(rocket(global_timeout(60_000))).await;
    let over = vec![
        head_chunked("POST", "/count"), chunk([b'x'; 400]), wait(300), chunk([b'x'; 300]), end(),
    ];

    assert_eq!(status(port, over).await, Some(413));
}

#[rkt::async_test]
async fn forwarding_does_not_revive_a_timed_out_body() {
    let port = launch(rocket(global_timeout(300))).await;

    // The first route's guard times out reading and forwards to a route with
    // a 10s deadline, which must not revive the body.
    let stalled = vec![head("POST", "/revive", 100), send([b'x'; 5]), wait(1000), send([b'x'; 95])];
    assert_eq!(status(port, stalled).await, Some(408));
}

#[rkt::async_test]
async fn forwarding_does_not_revive_an_oversized_body() {
    let port = launch(rocket(Config { body_limit: 16.bytes(), ..config() })).await;

    // 100 chunked bytes exceed the global 16B limit in the first route's guard,
    // which forwards to a route with a 1MiB limit.
    let over = vec![head_chunked("POST", "/revive"), chunk([b'x'; 100]), end()];
    assert_eq!(status(port, over).await, Some(413));
}

#[rkt::async_test]
async fn head_autohandling_applies_get_route_override() {
    // The `GET` route's 1s deadline applies to the re-dispatched `HEAD`; the
    // global deadline is 60s.
    let port = launch(rocket(global_timeout(60_000))).await;
    let slow = vec![head("HEAD", "/head", 10), send("hello"), wait(1500), send("world")];
    assert_eq!(status(port, slow).await, Some(408));
}

#[rkt::async_test]
async fn head_forwarding_does_not_restart_the_clock() {
    // A `HEAD` route holds the body for 600ms and forwards to the `GET` route,
    // as in `forwarding_does_not_restart_the_clock`.
    let port = launch(rocket(global_timeout(60_000))).await;
    let slow = vec![head("HEAD", "/head_fwd", 10), send("hello"), wait(1300), send("world")];
    assert_eq!(status(port, slow).await, Some(408));
}
