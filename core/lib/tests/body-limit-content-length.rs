#[macro_use]
extern crate rkt;

use std::sync::atomic::{AtomicUsize, Ordering};

use rkt::data::{Data, ToByteUnit};
use rkt::http::Status;
use rkt::local::blocking::Client;
use rkt::{Build, Config, Rocket, State};
use ubyte::ByteUnit;

/// The number of times the route has run.
struct Handled(AtomicUsize);

#[post("/", data = "<data>")]
async fn index(data: Data<'_>, handled: &State<Handled>) -> std::io::Result<String> {
    handled.0.fetch_add(1, Ordering::SeqCst);
    let string = data.open(1.kibibytes()).into_string().await?;
    Ok(string.into_inner())
}

fn rocket_with_body_limit(limit: ByteUnit) -> Rocket<Build> {
    rkt::custom(Config {
        body_limit: limit,
        ..Config::debug_default()
    })
    .manage(Handled(AtomicUsize::new(0)))
    .mount("/", routes![index])
}

fn handled(client: &Client) -> usize {
    client.rocket().state::<Handled>().unwrap().0.load(Ordering::SeqCst)
}

#[test]
fn body_within_limit_is_handled() {
    let client = Client::debug(rocket_with_body_limit(10.bytes())).unwrap();

    let response = client.post("/").body("hello").dispatch();
    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.into_string().as_deref(), Some("hello"));
    assert_eq!(handled(&client), 1);

    // Exactly the limit is within it.
    let response = client.post("/").body("0123456789").dispatch();
    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.into_string().as_deref(), Some("0123456789"));
    assert_eq!(handled(&client), 2);
}

#[test]
fn body_declared_over_limit_is_rejected_before_routing() {
    let client = Client::debug(rocket_with_body_limit(10.bytes())).unwrap();

    let response = client.post("/").body("0123456789!").dispatch();
    assert_eq!(response.status(), Status::PayloadTooLarge);
    assert_eq!(handled(&client), 0, "route ran for a rejected body");

    // The client remains usable afterwards.
    let response = client.post("/").body("ok").dispatch();
    assert_eq!(response.status(), Status::Ok);
    assert_eq!(handled(&client), 1);
}
