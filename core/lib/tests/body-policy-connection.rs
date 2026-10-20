// What happens to the transport after a body policy failure.
//
// Expected to fail until step 1c (connection handling after a policy failure).
// 1b is not needed: these use the global policy.
//
//   http1_connection_closes_after_timeout          1c  (no `Connection: close`)
//   http1_connection_closes_after_size_failure     1c  (no `Connection: close`)
//
// Passing today, as regression guards that 1c must keep passing. hyper happens
// to drop an HTTP/1 connection whose request body was left unread, and the h2
// stack resets a stream answered before its request ended, but neither is
// announced or guaranteed by rkt:
//
//   http1_second_request_on_the_socket_is_not_served
//   http1_response_is_flushed_before_close
//   http2_only_failing_stream_is_reset
//   http2_connection_survives_a_failed_stream
//
// Written against a real socket, since the local client has no connection to
// observe. HTTP/2 is spoken in cleartext with prior knowledge (h2c) using the
// `h2` crate as a client, which the server's protocol detection accepts. TLS
// and HTTP/3 are not exercised here: neither is needed to reach the same code
// path, and 1c documents any transport that cannot meet the requirement.
//
// The HTTP/2 tests are therefore not expected to drive 1c; the HTTP/1 tests do.

#[macro_use]
extern crate rkt;

use std::time::Duration;

use rkt::data::{Data, ToByteUnit};
use rkt::http::Status;
use rkt::tokio::net::TcpStream;
use rkt::tokio::time::timeout;
use rkt::Config;

#[path = "body-policy-support/mod.rs"]
mod support;

use support::*;

#[post("/echo", data = "<data>")]
async fn echo(data: Data<'_>) -> Result<String, Status> {
    let bytes = data.open(1.mebibytes()).into_bytes().await.map_err(|_| Status::BadRequest)?;
    Ok(bytes.len().to_string())
}

#[get("/ping")]
fn ping() -> &'static str { "pong" }

fn rocket() -> rkt::Rocket<rkt::Build> {
    let config = Config { body_timeout: duration(300), body_limit: 16.bytes(), ..config() };
    rkt::custom(config).mount("/", routes![echo, ping])
}

const PING: &str = "GET /ping HTTP/1.1\r\nHost: localhost\r\n\r\n";

/// After `first` has been answered with `expected`, the connection must
/// announce and carry out its closure.
async fn assert_closes(first: Vec<Step>, expected: u16) {
    let port = launch(rocket()).await;
    let mut client = Client::connect(port).await;
    let _script = client.play(first);

    let response = client.response().await.expect("the failure is answered");
    assert_eq!(response.status, expected);
    assert_eq!(response.header("connection"), Some("close"), "{response:?}");
    assert!(client.closed_within(Duration::from_secs(2)).await, "connection left open");
}

#[rkt::async_test]
async fn http1_connection_closes_after_timeout() {
    let stalled = vec![head("POST", "/echo", 10), send("hello"), wait(1500)];
    assert_closes(stalled, 408).await;
}

#[rkt::async_test]
async fn http1_connection_closes_after_size_failure() {
    let over = vec![head_chunked("POST", "/echo"), chunk([b'x'; 10]), chunk([b'x'; 10]), wait(1500)];
    assert_closes(over, 413).await;
}

#[rkt::async_test]
async fn http1_second_request_on_the_socket_is_not_served() {
    let port = launch(rocket()).await;
    let mut client = Client::connect(port).await;

    // Pipelined on one socket: a request that fails its body, then a ping.
    let _script = client.play(vec![
        head("POST", "/echo", 10), send("hello"), wait(600), send(PING),
    ]);

    let first = client.response().await.expect("the failure is answered");
    assert_eq!(first.status, 408);
    assert_eq!(client.response().await.map(|r| r.status), None, "second request was served");
}

#[rkt::async_test]
async fn http1_response_is_flushed_before_close() {
    // The client has not finished sending when the server answers: the response
    // must still arrive whole, not as a connection reset.
    let port = launch(rocket()).await;
    let mut client = Client::connect(port).await;
    let _script = client.play(vec![head("POST", "/echo", 10), send([b'x'; 8]), wait(2000)]);

    let response = client.response().await.expect("response arrives before the close");
    assert_eq!(response.status, 408);
    assert!(response.body.contains("Request Timeout"), "{response:?}");
}

/// Opens an h2c connection with prior knowledge.
async fn h2_connect(port: u16) -> h2::client::SendRequest<bytes::Bytes> {
    let tcp = TcpStream::connect(("127.0.0.1", port)).await.expect("connect");
    let (send, connection) = h2::client::handshake(tcp).await.expect("h2 handshake");
    rkt::tokio::spawn(connection);
    send
}

fn h2_request(method: &str, path: &str) -> http::Request<()> {
    http::Request::builder()
        .method(method)
        .uri(format!("http://localhost{path}"))
        .body(())
        .unwrap()
}

/// Sends a complete small request on `send` and returns its status.
async fn h2_complete(send: &h2::client::SendRequest<bytes::Bytes>) -> Option<u16> {
    let mut ready = send.clone().ready().await.ok()?;
    let (response, mut body) = ready.send_request(h2_request("POST", "/echo"), false).ok()?;
    body.send_data(bytes::Bytes::from_static(b"hello"), true).ok()?;
    let response = timeout(RESPONSE_WAIT, response).await.ok()?.ok()?;
    Some(response.status().as_u16())
}

#[rkt::async_test]
async fn http2_only_failing_stream_is_reset() {
    let port = launch(rocket()).await;
    let send = h2_connect(port).await;

    // The failing stream sends some of a body and then stalls.
    let mut ready = send.clone().ready().await.expect("ready");
    let (response, mut body) = ready.send_request(h2_request("POST", "/echo"), false).unwrap();
    body.send_data(bytes::Bytes::from_static(b"hello"), false).unwrap();

    // A concurrent stream on the same connection completes while it stalls.
    assert_eq!(h2_complete(&send).await, Some(200), "concurrent stream");

    // The failing stream is answered, then reset: it cannot be written to.
    let response = timeout(RESPONSE_WAIT, response).await.expect("answered").expect("response");
    assert_eq!(response.status().as_u16(), 408);

    let reset = timeout(RESPONSE_WAIT, futures_poll_reset(&mut body)).await;
    assert!(matches!(reset, Ok(Some(_))), "failing stream was not reset: {reset:?}");

    // The connection and its other streams are unaffected.
    assert_eq!(h2_complete(&send).await, Some(200), "connection survives");
}

#[rkt::async_test]
async fn http2_connection_survives_a_failed_stream() {
    let port = launch(rocket()).await;
    let send = h2_connect(port).await;

    // An oversized body fails its stream; later streams on the connection work.
    let mut ready = send.clone().ready().await.expect("ready");
    let (response, mut body) = ready.send_request(h2_request("POST", "/echo"), false).unwrap();
    body.send_data(bytes::Bytes::from(vec![b'x'; 32]), true).unwrap();
    let response = timeout(RESPONSE_WAIT, response).await.expect("answered").expect("response");
    assert_eq!(response.status().as_u16(), 413);

    assert_eq!(h2_complete(&send).await, Some(200), "later stream");
}

/// Waits for the peer to reset the stream, returning the reason.
async fn futures_poll_reset(body: &mut h2::SendStream<bytes::Bytes>) -> Option<h2::Reason> {
    std::future::poll_fn(|cx| body.poll_reset(cx)).await.ok()
}
