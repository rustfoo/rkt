#[macro_use]
extern crate rkt;

#[get("/")]
fn index() -> String {
    "Hello, world!".into()
}

#[test]
fn content_length_header() {
    let rocket = rkt::build().mount("/", routes![index]);
    let client = rkt::local::blocking::Client::debug(rocket).unwrap();
    let response = client.get("/").dispatch();
    assert!(response.headers().get_one("Content-Length").is_some());
}
