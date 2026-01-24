#![cfg(feature = "secrets")]

extern crate rkt;

use rkt::http::{Cookie, CookieJar};

#[rkt::get("/")]
fn index(jar: &CookieJar<'_>) {
    jar.add_private(Cookie::build(("key", "value")).expires(None));
}

mod test_session_cookies {
    use super::*;
    use rkt::local::blocking::Client;

    #[test]
    fn session_cookie_is_session() {
        let rocket = rkt::build().mount("/", rkt::routes![index]);
        let client = Client::debug(rocket).unwrap();

        let response = client.get("/").dispatch();
        let cookie = response.cookies().get_private("key").unwrap();
        assert_eq!(cookie.expires_datetime(), None);
    }
}
