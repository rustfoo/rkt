extern crate rkt;

use rkt::local::blocking::Client;
use rkt::{Build, Rocket};

mod inner {
    use rkt::uri;

    #[rkt::get("/")]
    pub fn hello() -> String {
        format!("Hello! Try {}.", uri!(super::hello_name("Rust 2018")))
    }
}

#[rkt::get("/<name>")]
fn hello_name(name: String) -> String {
    format!(
        "Hello, {}! This is {}.",
        name,
        rkt::uri!(hello_name(&name))
    )
}

fn rocket() -> Rocket<Build> {
    rkt::build()
        .mount("/", rkt::routes![hello_name])
        .mount("/", rkt::routes![inner::hello])
}

#[test]
fn test_inner_hello() {
    let client = Client::debug(rocket()).unwrap();
    let response = client.get("/").dispatch();
    assert_eq!(
        response.into_string(),
        Some("Hello! Try /Rust%202018.".into())
    );
}

#[test]
fn test_hello_name() {
    let client = Client::debug(rocket()).unwrap();
    let response = client.get("/Rust%202018").dispatch();
    assert_eq!(
        response.into_string().unwrap(),
        "Hello, Rust 2018! This is /Rust%202018."
    );
}
