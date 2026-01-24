extern crate rkt;

use rkt::http::uri::Origin;
use rkt::local::blocking::Client;

#[test]
fn can_correct_bad_local_uri() {
    #[rkt::get("/")]
    fn f() {}

    let client = Client::debug_with(rkt::routes![f]).unwrap();
    let mut req = client.get("this is a bad URI");
    req.set_uri(Origin::parse("/").unwrap());

    assert_eq!(req.uri(), "/");
    assert!(req.dispatch().status().class().is_success());

    let req = client.get("this is a bad URI");
    assert!(req.dispatch().status().class().is_client_error());
}
