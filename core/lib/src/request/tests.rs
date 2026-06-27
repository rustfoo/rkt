use std::collections::HashMap;

use crate::local::blocking::Client;
use crate::request::{ConnectionMeta, Request};

macro_rules! assert_headers {
    ($($key:expr => [$($value:expr),+]),+) => ({
        // Create a new Hyper request. Add all of the passed in headers.
        let mut req = hyper::Request::get("/test").body(()).unwrap();
        $($(
            req.headers_mut()
                .append($key, hyper::header::HeaderValue::from_str($value).unwrap());
        )+)+

        // Build up what we expect the headers to actually be.
        let mut expected = HashMap::new();
        $(expected.entry($key).or_insert(vec![]).append(&mut vec![$($value),+]);)+

        // Create a valid `Rocket` and convert the hyper req to a rkt one.
        let client = Client::debug_with(vec![]).unwrap();
        let hyper = req.into_parts().0;
        let meta = ConnectionMeta::default();
        let req = Request::from_hyp(client.rocket(), &hyper, meta).unwrap();

        // Dispatch the request and check that the headers match.
        let actual_headers = req.headers();
        for (key, values) in expected.iter() {
            let actual: Vec<_> = actual_headers.get(key).collect();
            assert_eq!(*values, actual);
        }
    })
}

#[test]
fn test_multiple_headers_from_hyp() {
    assert_headers!("friends" => ["alice"]);
    assert_headers!("friends" => ["alice", "bob"]);
    assert_headers!("friends" => ["alice", "bob, carol"]);
    assert_headers!("friends" => ["alice, david", "bob, carol", "eric, frank"]);
    assert_headers!("friends" => ["alice"], "enemies" => ["victor"]);
    assert_headers!("friends" => ["alice", "bob"], "enemies" => ["david", "emily"]);
}

#[test]
fn test_multiple_headers_merge_into_one_from_hyp() {
    assert_headers!("friend" => ["alice"], "friend" => ["bob"]);
    assert_headers!("friend" => ["alice"], "friend" => ["bob"], "friend" => ["carol"]);
    assert_headers!("friend" => ["alice"], "friend" => ["bob"], "enemy" => ["carol"]);
}

// Regression: a request received over the network (i.e. built via `from_hyp`,
// not via `add_header`) must still derive its secure context from a configured
// proxy-proto header. Lazy headers mean `from_hyp` no longer routes through
// `add_header`/`bust_header_cache`, so the update must happen in `from_hyp`.
#[test]
fn test_secure_context_from_proxy_proto_from_hyp() {
    use crate::http::ProxyProto;

    fn proto_client() -> Client {
        let mut config = crate::Config::debug_default();
        config.proxy_proto_header = Some("X-Forwarded-Proto".into());
        Client::debug(crate::custom(config)).unwrap()
    }

    fn request_with(proto: Option<&'static str>) -> bool {
        let mut req = hyper::Request::get("/test").body(()).unwrap();
        if let Some(proto) = proto {
            req.headers_mut().append(
                "X-Forwarded-Proto",
                hyper::header::HeaderValue::from_static(proto),
            );
        }

        let client = proto_client();
        let hyper = req.into_parts().0;
        let request =
            Request::from_hyp(client.rocket(), &hyper, ConnectionMeta::default()).unwrap();

        if let Some(proto) = proto {
            assert_eq!(request.proxy_proto(), Some(ProxyProto::from(proto)));
        }

        request.context_is_likely_secure()
    }

    assert!(request_with(Some("https")));
    assert!(!request_with(Some("http")));
    assert!(!request_with(None));
}
