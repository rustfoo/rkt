#[macro_use]
extern crate rkt;
extern crate self as tracing;

#[cfg(test)]
mod tests;
mod redirector;

use rkt::tracing::*;
use rkt::mtls::Certificate;
use rkt::listener::Endpoint;

#[get("/")]
fn mutual(cert: Certificate<'_>) -> String {
    format!("Hello! Here's what we know: [{}] {}", cert.serial(), cert.subject())
}

#[get("/", rank = 2)]
fn hello(endpoint: Option<&Endpoint>) -> String {
    match endpoint {
        Some(endpoint) => format!("Hello, {endpoint}!"),
        None => "Hello, world!".into(),
    }
}

#[launch]
fn rocket() -> _ {
    // See `Rocket.toml` and `Cargo.toml` for TLS configuration.
    // Run `./private/gen_certs.sh` to generate a CA and key pairs.
    rkt::build()
        .mount("/", routes![hello, mutual])
        .attach(redirector::Redirector::on(3000))
}
