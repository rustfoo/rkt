// Shared by the `body-policy-*` tests, which need to control exactly when the
// bytes of a request body arrive. The local client cannot: it hands the server
// a body it already holds. These helpers launch a real server on a loopback
// port and speak raw HTTP/1 to it, so a test can stall, trickle, or abandon a
// body at will.
//
// Time is real, not paused: with real sockets a paused clock would advance
// whenever the runtime went idle waiting on the network. Tests therefore use
// short but generously separated durations (a 300ms limit against an 800ms
// stall) so they stay quick without depending on scheduling.

#![allow(dead_code)]

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use rkt::config::{Config, ConfigDuration};
use rkt::fairing::AdHoc;
use rkt::futures::channel::oneshot;
use rkt::listener::tcp::TcpListener;
use rkt::{Build, Rocket};
use rkt::tokio::io::{AsyncReadExt, AsyncWriteExt};
use rkt::tokio::net::TcpStream;
use rkt::tokio::time::{sleep, timeout};

/// How long a client waits for a response before concluding there is none.
pub const RESPONSE_WAIT: Duration = Duration::from_secs(4);

pub fn ms(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

pub fn duration(ms: u64) -> Option<ConfigDuration> {
    Some(ConfigDuration(Duration::from_millis(ms)))
}

/// The configuration to start from.
pub fn config() -> Config {
    Config { log_level: None, ..Config::debug_default() }
}

/// Launches `rocket` on a free loopback port and returns the port.
pub async fn launch(rocket: Rocket<Build>) -> u16 {
    let (tx, rx) = oneshot::channel();
    let rocket = rocket.attach(AdHoc::on_liftoff("Send Port", move |rocket| {
        Box::pin(async move {
            let tcp = rocket.endpoints().find_map(|v| v.tcp());
            tx.send(tcp.unwrap().port()).expect("send okay");
        })
    }));

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    rkt::tokio::spawn(rocket.try_launch_on(TcpListener::bind(addr)));
    rx.await.expect("server launched")
}

/// What a client does while sending a request.
#[derive(Debug, Clone)]
pub enum Step {
    Send(Vec<u8>),
    Wait(Duration),
    /// Closes the connection, as a client that disconnects mid-request.
    Close,
}

pub fn send(bytes: impl AsRef<[u8]>) -> Step {
    Step::Send(bytes.as_ref().to_vec())
}

pub fn close() -> Step {
    Step::Close
}

pub fn wait(ms: u64) -> Step {
    Step::Wait(Duration::from_millis(ms))
}

/// A chunk of a `Transfer-Encoding: chunked` body.
pub fn chunk(bytes: impl AsRef<[u8]>) -> Step {
    let bytes = bytes.as_ref();
    let mut chunk = format!("{:x}\r\n", bytes.len()).into_bytes();
    chunk.extend_from_slice(bytes);
    chunk.extend_from_slice(b"\r\n");
    Step::Send(chunk)
}

/// The final chunk of a chunked body.
pub fn end() -> Step {
    send("0\r\n\r\n")
}

/// A request head declaring a body of `length` bytes.
pub fn head(method: &str, path: &str, length: usize) -> Step {
    head_with(method, path, &format!("Content-Length: {length}"))
}

/// A request head declaring a chunked body.
pub fn head_chunked(method: &str, path: &str) -> Step {
    head_with(method, path, "Transfer-Encoding: chunked")
}

/// A request head with `headers`, a block of `Name: value` lines.
pub fn head_with(method: &str, path: &str, headers: &str) -> Step {
    send(format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n{headers}\r\n\r\n"))
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_ascii_lowercase()).map(|v| v.as_str())
    }
}

/// An HTTP/1 client connection.
pub struct Client {
    read: rkt::tokio::net::tcp::OwnedReadHalf,
    write: Option<rkt::tokio::net::tcp::OwnedWriteHalf>,
    buffer: Vec<u8>,
    /// Set when the request is `HEAD`, whose response carries no body.
    head: bool,
}

impl Client {
    pub async fn connect(port: u16) -> Client {
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await.expect("connect");
        let (read, write) = stream.into_split();
        Client { read, write: Some(write), buffer: vec![], head: false }
    }

    /// Plays `steps` in the background, so the response can be read while the
    /// client is still sending, as it can in life. Write errors, such as the
    /// server closing the connection after a failure, end the script quietly.
    pub fn play(&mut self, steps: Vec<Step>) -> rkt::tokio::task::JoinHandle<()> {
        self.head = matches!(steps.first(), Some(Step::Send(b)) if b.starts_with(b"HEAD "));
        let mut write = self.write.take().expect("connection can still write");
        rkt::tokio::spawn(async move {
            for step in steps {
                match step {
                    Step::Wait(duration) => sleep(duration).await,
                    Step::Close => return,
                    Step::Send(bytes) => if write.write_all(&bytes).await.is_err() {
                        return;
                    }
                }
            }

            // Keep the connection open: closing it would look like a client
            // that gave up, which is not what the script describes.
            sleep(Duration::from_secs(30)).await;
        })
    }

    /// Reads one response, or `None` if the connection closes or `RESPONSE_WAIT`
    /// passes before a complete one arrives.
    pub async fn response(&mut self) -> Option<Response> {
        timeout(RESPONSE_WAIT, self.read_response()).await.ok().flatten()
    }

    async fn fill(&mut self) -> bool {
        let mut chunk = [0u8; 4096];
        match self.read.read(&mut chunk).await {
            Ok(0) | Err(_) => false,
            Ok(n) => {
                self.buffer.extend_from_slice(&chunk[..n]);
                true
            }
        }
    }

    async fn read_response(&mut self) -> Option<Response> {
        let split = loop {
            if let Some(i) = self.buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                break i;
            }

            if !self.fill().await {
                return None;
            }
        };

        let head = String::from_utf8_lossy(&self.buffer[..split]).into_owned();
        self.buffer.drain(..split + 4);

        let mut lines = head.lines();
        let status = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
        let headers: HashMap<String, String> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
            .collect();

        // Responses in these tests are sized or empty.
        let length = headers.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
        let length = if self.head { 0 } else { length };
        while self.buffer.len() < length {
            if !self.fill().await {
                return None;
            }
        }

        let body = String::from_utf8_lossy(&self.buffer[..length]).into_owned();
        self.buffer.drain(..length);
        Some(Response { status, headers, body })
    }

    /// Whether the server has closed its end of the connection, within `wait`.
    /// Bytes still in flight, such as an unread response, are discarded.
    pub async fn closed_within(&mut self, wait: Duration) -> bool {
        timeout(wait, async { while self.fill().await {} }).await.is_ok()
    }
}

/// Plays `steps` against a fresh connection and returns the first response.
pub async fn request(port: u16, steps: Vec<Step>) -> Option<Response> {
    let mut client = Client::connect(port).await;
    let _script = client.play(steps);
    client.response().await
}

/// The status of `request(port, steps)`.
pub async fn status(port: u16, steps: Vec<Step>) -> Option<u16> {
    request(port, steps).await.map(|r| r.status)
}
