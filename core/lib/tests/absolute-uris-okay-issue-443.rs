#[macro_use]
extern crate rkt;

use rkt::response::Redirect;

#[get("/http")]
fn http() -> Redirect {
    Redirect::to(uri!("http://example.com"))
}

#[get("/rocket")]
fn redirect() -> Redirect {
    Redirect::to("https://example.com:80")
}

mod test_absolute_uris_okay {
    use super::*;
    use rkt::local::blocking::Client;

    #[test]
    fn redirect_works() {
        let client = Client::debug_with(routes![http, redirect]).unwrap();

        let response = client.get(uri!(http)).dispatch();
        let location = response.headers().get_one("Location");
        assert_eq!(location, Some("http://example.com"));

        let response = client.get(uri!(redirect)).dispatch();
        let location = response.headers().get_one("Location");
        assert_eq!(location, Some("https://example.com:80"));
    }
}
