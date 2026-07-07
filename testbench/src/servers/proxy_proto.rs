//! Ensure that when `proxy_protocol` is enabled, the address forwarded in a
//! PROXY protocol preamble becomes the connection's remote endpoint, and that
//! connections without a valid preamble are rejected.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::prelude::*;

static PROXY_CONFIG: &str = r#"
    [default]
    proxy_protocol = true
"#;

#[get("/")]
fn remote(addr: SocketAddr) -> String {
    addr.to_string()
}

/// The v2 signature followed by version/command `PROXY`, family/protocol
/// `AF_INET`/`STREAM`, a 12-byte address block for src `203.0.113.6:4242`
/// and dst `127.0.0.1:80`.
static V2_PREAMBLE: &[u8] = &[
    0x0D, 0x0A, 0x0D, 0x0A, 0x00, 0x0D, 0x0A, 0x51, 0x55, 0x49, 0x54, 0x0A, 0x21, 0x11, 0x00, 0x0C,
    203, 0, 113, 6, 127, 0, 0, 1, 0x10, 0x92, 0x00, 0x50,
];

/// The v2 signature followed by version/command `LOCAL` with no address
/// block, as sent by proxy health checks.
static V2_LOCAL_PREAMBLE: &[u8] = &[
    0x0D, 0x0A, 0x0D, 0x0A, 0x00, 0x0D, 0x0A, 0x51, 0x55, 0x49, 0x54, 0x0A, 0x20, 0x00, 0x00, 0x00,
];

static REQUEST: &[u8] = b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";

fn connect(server: &Server) -> Result<TcpStream> {
    let stream = TcpStream::connect(server.socket_addr())?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    Ok(stream)
}

/// Sends `bytes` on a fresh connection and returns the full response, which
/// is empty if the server closed the connection without responding.
fn exchange(server: &Server, bytes: &[u8]) -> Result<String> {
    let mut stream = connect(server)?;
    let _ = stream.write_all(bytes);
    let mut response = String::new();
    match stream.read_to_string(&mut response) {
        Ok(_) => Ok(response),
        // The server resets rejected connections with unread input.
        Err(_) if response.is_empty() => Ok(response),
        Err(e) => Err(e.into()),
    }
}

#[track_caller]
fn assert_forwarded(response: &str, addr: &str) {
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "bad response: {response}"
    );
    assert!(
        response.ends_with(addr),
        "expected remote {addr}: {response}"
    );
}

fn test_proxy_protocol() -> Result<()> {
    let server = spawn! {
        Rocket::default()
            .reconfigure_with_toml(PROXY_CONFIG)
            .mount("/", routes![remote])
    }?;

    // A v1 preamble written separately from the request.
    let mut stream = connect(&server)?;
    stream.write_all(b"PROXY TCP4 192.0.2.7 127.0.0.1 41234 80\r\n")?;
    stream.write_all(REQUEST)?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    assert_forwarded(&response, "192.0.2.7:41234");

    // A v2 preamble coalesced with the request in a single write, so bytes
    // read past the preamble must be replayed to the connection.
    let mut bytes = V2_PREAMBLE.to_vec();
    bytes.extend_from_slice(REQUEST);
    let response = exchange(&server, &bytes)?;
    assert_forwarded(&response, "203.0.113.6:4242");

    // A v2 `LOCAL` preamble (health check) conveys no address: the remote
    // endpoint is the connection's actual source address.
    let mut bytes = V2_LOCAL_PREAMBLE.to_vec();
    bytes.extend_from_slice(REQUEST);
    let response = exchange(&server, &bytes)?;
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "bad response: {response}"
    );
    assert!(
        response.contains("\r\n\r\n127.0.0.1:"),
        "expected local remote: {response}"
    );

    // Connections that do not begin with a preamble are rejected without a
    // response: a direct HTTP request, and one that mimics a v1 preamble.
    let response = exchange(&server, REQUEST)?;
    assert!(response.is_empty(), "expected rejection: {response}");

    let response = exchange(&server, b"PROXY TCP4 not an address\r\n")?;
    assert!(response.is_empty(), "expected rejection: {response}");

    Ok(())
}

/// With the feature compiled in but `proxy_protocol` unset, connections pass
/// through unmodified.
fn test_proxy_protocol_unset() -> Result<()> {
    let server = spawn!(Rocket::default().mount("/", routes![remote]))?;

    let client = Client::default();
    let response = client.get(&server, "/")?.send()?;
    assert!(response.text()?.starts_with("127.0.0.1:"));

    Ok(())
}

register!(test_proxy_protocol);
register!(test_proxy_protocol_unset);
