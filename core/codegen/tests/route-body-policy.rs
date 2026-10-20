// Every valid combination of the body policy route options compiles, mounts,
// and leaves the route usable. The options are parsed and validated here;
// their runtime effect is tested in `rkt`'s `body-policy-*` tests.

#[macro_use] extern crate rkt;

use rkt::local::blocking::Client;
use rkt::http::Status;

#[post("/none", data = "<_d>")]
fn none(_d: Vec<u8>) -> &'static str { "none" }

#[post("/timeout", data = "<_d>", body_timeout = "30s")]
fn timeout(_d: Vec<u8>) -> &'static str { "timeout" }

#[post("/compound", data = "<_d>", body_timeout = "1m30s")]
fn compound(_d: Vec<u8>) -> &'static str { "compound" }

#[post("/rate", data = "<_d>", body_min_rate = "8KiB/s")]
fn rate(_d: Vec<u8>) -> &'static str { "rate" }

#[post("/rate_period", data = "<_d>", body_min_rate = "1MiB/10s")]
fn rate_period(_d: Vec<u8>) -> &'static str { "rate_period" }

#[post("/rate_grace", data = "<_d>", body_min_rate = "8KiB/s", body_grace = "30s")]
fn rate_grace(_d: Vec<u8>) -> &'static str { "rate_grace" }

#[post("/unlimited", data = "<_d>", body_policy = "unlimited")]
fn unlimited(_d: Vec<u8>) -> &'static str { "unlimited" }

#[post("/limit", data = "<_d>", body_limit = "2MiB")]
fn limit(_d: Vec<u8>) -> &'static str { "limit" }

#[post("/timeout_limit", data = "<_d>", body_timeout = "30s", body_limit = "2MiB")]
fn timeout_limit(_d: Vec<u8>) -> &'static str { "timeout_limit" }

#[post(
    "/rate_grace_limit",
    data = "<_d>",
    body_min_rate = "8KiB/s",
    body_grace = "30s",
    body_limit = "5GiB"
)]
fn rate_grace_limit(_d: Vec<u8>) -> &'static str { "rate_grace_limit" }

#[post("/unlimited_limit", data = "<_d>", body_policy = "unlimited", body_limit = "64MiB")]
fn unlimited_limit(_d: Vec<u8>) -> &'static str { "unlimited_limit" }

// Option order is irrelevant, and the options are accepted without `data`.
#[post("/reordered", body_limit = "1KiB", body_grace = "5s", body_min_rate = "1KiB/s", data = "<_d>")]
fn reordered(_d: Vec<u8>) -> &'static str { "reordered" }

#[put("/no_data", body_timeout = "5s")]
fn no_data() -> &'static str { "no_data" }

// The generic `route` attribute accepts the same options.
#[route("/generic", method = POST, data = "<_d>", body_timeout = "30s", body_limit = "1MiB")]
fn generic(_d: Vec<u8>) -> &'static str { "generic" }

#[test]
fn valid_body_policy_options_mount_and_route() {
    let rocket = rkt::build().mount("/", routes![
        none, timeout, compound, rate, rate_period, rate_grace, unlimited, limit,
        timeout_limit, rate_grace_limit, unlimited_limit, reordered, no_data, generic,
    ]);

    let client = Client::debug(rocket).unwrap();
    let cases = [
        ("none", "none"), ("timeout", "timeout"), ("compound", "compound"),
        ("rate", "rate"), ("rate_period", "rate_period"), ("rate_grace", "rate_grace"),
        ("unlimited", "unlimited"), ("limit", "limit"), ("timeout_limit", "timeout_limit"),
        ("rate_grace_limit", "rate_grace_limit"), ("unlimited_limit", "unlimited_limit"),
        ("reordered", "reordered"), ("generic", "generic"),
    ];

    for (path, expected) in cases {
        let response = client.post(format!("/{path}")).body("hello").dispatch();
        assert_eq!(response.status(), Status::Ok, "POST /{path}");
        assert_eq!(response.into_string().unwrap(), expected, "POST /{path}");
    }

    let response = client.put("/no_data").dispatch();
    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.into_string().unwrap(), "no_data");
}
