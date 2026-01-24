#[macro_use] extern crate rkt;

mod async_required;

#[get("/")]
fn hello() -> &'static str {
    "Hello, world!"
}

#[launch]
fn rocket() -> _ {
    async_required::rocket().mount("/", routes![hello])
}

#[cfg(test)]
mod test {
    use super::rocket;
    use rkt::http::Status;

    #[test]
    fn test_hello() {
        use rkt::local::blocking::Client;

        let client = Client::tracked(rocket()).unwrap();
        let response = client.get("/").dispatch();
        assert_eq!(response.status(), Status::Ok);
        assert_eq!(response.into_string(), Some("Hello, world!".into()));
    }
}
