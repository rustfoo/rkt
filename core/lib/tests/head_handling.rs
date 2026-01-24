#[macro_use]
extern crate rkt;

use rkt::http::Status;
use rkt::response::content::RawJson;

#[get("/empty")]
fn empty() -> Status {
    Status::NoContent
}

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[head("/other")]
fn other() -> RawJson<&'static str> {
    RawJson("{ 'hi': 'hello' }")
}

mod head_handling_tests {
    use super::*;

    use rkt::http::ContentType;
    use rkt::local::blocking::Client;
    use rkt::Route;

    fn routes() -> Vec<Route> {
        routes![index, empty, other]
    }

    #[test]
    fn auto_head() {
        let client = Client::debug_with(routes()).unwrap();
        let response = client.head("/").dispatch();

        let content_type: Vec<_> = response.headers().get("Content-Type").collect();
        assert_eq!(content_type, vec![ContentType::Plain.to_string()]);
        assert_eq!(response.status(), Status::Ok);
        assert_eq!(response.body().preset_size(), Some(13));
        assert!(response.into_bytes().unwrap().is_empty());

        let response = client.head("/empty").dispatch();
        assert_eq!(response.status(), Status::NoContent);
        assert!(response.into_bytes().is_none());
    }

    #[test]
    fn user_head() {
        let client = Client::debug_with(routes()).unwrap();
        let response = client.head("/other").dispatch();

        let content_type: Vec<_> = response.headers().get("Content-Type").collect();
        assert_eq!(content_type, vec![ContentType::JSON.to_string()]);
        assert_eq!(response.status(), Status::Ok);
        assert_eq!(response.body().preset_size(), Some(17));
        assert!(response.into_bytes().unwrap().is_empty());
    }
}
