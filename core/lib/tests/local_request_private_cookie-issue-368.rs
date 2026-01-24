#![cfg(feature = "secrets")]

extern crate rkt;

use rkt::http::CookieJar;

#[rkt::get("/")]
fn return_private_cookie(cookies: &CookieJar<'_>) -> Option<String> {
    cookies.get_private("cookie_name").map(|cookie| cookie.value().into())
}

mod tests {
    use super::*;
    use rkt::http::Status;
    use rkt::local::blocking::Client;
    use rkt::routes;

    #[test]
    fn private_cookie_is_returned() {
        let rocket = rkt::build().mount("/", routes![return_private_cookie]);

        let client = Client::debug(rocket).unwrap();
        let req = client
            .get("/")
            .private_cookie(("cookie_name", "cookie_value"));
        let response = req.dispatch();

        assert_eq!(response.headers().get_one("Set-Cookie"), None);
        assert_eq!(response.into_string(), Some("cookie_value".into()));
    }

    #[test]
    fn regular_cookie_is_not_returned() {
        let rocket = rkt::build().mount("/", routes![return_private_cookie]);

        let client = Client::debug(rocket).unwrap();
        let req = client.get("/").cookie(("cookie_name", "cookie_value"));
        let response = req.dispatch();

        assert_eq!(response.status(), Status::NotFound);
    }
}
