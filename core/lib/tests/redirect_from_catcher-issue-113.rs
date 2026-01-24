#[macro_use]
extern crate rkt;

use rkt::response::Redirect;

#[catch(404)]
fn not_found() -> Redirect {
    Redirect::to("/")
}

mod tests {
    use super::*;
    use rkt::http::Status;
    use rkt::local::blocking::Client;

    #[test]
    fn error_catcher_redirect() {
        let client = Client::debug(rkt::build().register("/", catchers![not_found])).unwrap();
        let response = client.get("/unknown").dispatch();

        let location: Vec<_> = response.headers().get("location").collect();
        assert_eq!(response.status(), Status::SeeOther);
        assert_eq!(location, vec!["/"]);
    }
}
