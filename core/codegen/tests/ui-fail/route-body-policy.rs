#[macro_use] extern crate rkt;

// Only one progress policy per route.
#[post("/", data = "<_d>", body_timeout = "30s", body_min_rate = "8KiB/s")]
fn a0(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_timeout = "30s", body_policy = "unlimited")]
fn a1(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_min_rate = "8KiB/s", body_policy = "unlimited")]
fn a2(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_policy = "unlimited", body_timeout = "30s")]
fn a3(_d: Vec<u8>) {}

// A grace period needs a rate.
#[post("/", data = "<_d>", body_grace = "30s")]
fn b0(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_timeout = "30s", body_grace = "30s")]
fn b1(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_policy = "unlimited", body_grace = "30s")]
fn b2(_d: Vec<u8>) {}

// Unparsable durations, rates, and sizes.
#[post("/", data = "<_d>", body_timeout = "soon")]
fn c0(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_timeout = "30 seconds")]
fn c1(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_min_rate = "8KiB")]
fn c2(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_min_rate = "fast/s")]
fn c3(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_min_rate = "8KiB/s", body_grace = "later")]
fn c4(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_limit = "lots")]
fn c5(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_timeout = 30)]
fn c6(_d: Vec<u8>) {}

// Only `"unlimited"` is a valid policy.
#[post("/", data = "<_d>", body_policy = "limited")]
fn d0(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_policy = "")]
fn d1(_d: Vec<u8>) {}

// Zero values are rejected.
#[post("/", data = "<_d>", body_timeout = "0s")]
fn e0(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_min_rate = "0/s")]
fn e1(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_min_rate = "8KiB/0s")]
fn e2(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_min_rate = "8KiB/s", body_grace = "0s")]
fn e3(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_limit = "0B")]
fn e4(_d: Vec<u8>) {}

// Duplicate options.
#[post("/", data = "<_d>", body_timeout = "30s", body_timeout = "60s")]
fn f0(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_min_rate = "8KiB/s", body_min_rate = "1KiB/s")]
fn f1(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_min_rate = "8KiB/s", body_grace = "1s", body_grace = "2s")]
fn f2(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_policy = "unlimited", body_policy = "unlimited")]
fn f3(_d: Vec<u8>) {}

#[post("/", data = "<_d>", body_limit = "1MiB", body_limit = "2MiB")]
fn f4(_d: Vec<u8>) {}

// The same checks apply through the generic `route` attribute.
#[route("/", method = POST, data = "<_d>", body_timeout = "30s", body_policy = "unlimited")]
fn g0(_d: Vec<u8>) {}

fn main() { }
