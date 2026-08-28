#[macro_use]
extern crate rkt;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use rkt::async_trait;
use rkt::config::{CliColors, ShutdownConfig};
use rkt::figment::Figment;
use rkt::fs::FileServer;
use rkt::http::Status;
use rkt::request::{FromRequest, Outcome, Request};
use rkt::response::stream::ByteStream;
use rkt::serde::json::Json;
use rkt::tokio::time::{sleep, Duration};
use rkt::{Config, State};
use serde::Serialize;

type Map = Arc<RwLock<HashMap<String, String>>>;

#[get("/ping")]
fn ping() -> &'static str {
    "pong"
}

#[derive(Serialize)]
struct HelloResponse {
    message: &'static str,
}

#[get("/hello")]
fn hello() -> Json<HelloResponse> {
    Json(HelloResponse { message: "hello" })
}

#[get("/state/<key>")]
fn state_get(key: &str, map: &State<Map>) -> (Status, String) {
    match map.read().unwrap().get(key) {
        Some(val) => (Status::Ok, val.clone()),
        None => (Status::NotFound, String::new()),
    }
}

#[derive(Serialize)]
struct QueryResponse<'r> {
    msg: &'r str,
    n: u32,
}

#[get("/query?<msg>&<n>")]
fn query(msg: &str, n: u32) -> Json<QueryResponse<'_>> {
    Json(QueryResponse { msg, n })
}

#[derive(Serialize)]
struct OwnedQueryResponse {
    msg: String,
    n: u32,
}

#[get("/query-owned?<msg>&<n>")]
fn query_owned(msg: String, n: u32) -> Json<OwnedQueryResponse> {
    Json(OwnedQueryResponse { msg, n })
}

static MEMORY_1K: [u8; 1024] = [b'x'; 1024];
static MEMORY_64K: [u8; 64 * 1024] = [b'x'; 64 * 1024];
static MEMORY_1M: [u8; 1024 * 1024] = [b'x'; 1024 * 1024];
static STREAM_CHUNK: [u8; 1024] = [b'x'; 1024];

#[get("/memory/1k")]
fn memory_1k() -> &'static [u8] {
    &MEMORY_1K
}

#[get("/memory/64k")]
fn memory_64k() -> &'static [u8] {
    &MEMORY_64K
}

#[get("/memory/1m")]
fn memory_1m() -> &'static [u8] {
    &MEMORY_1M
}

#[get("/stream-slow")]
fn stream_slow() -> ByteStream![&'static [u8]] {
    ByteStream! {
        for _ in 0..16 {
            sleep(Duration::from_millis(5)).await;
            yield &STREAM_CHUNK[..];
        }
    }
}

struct BenchHeaders {
    bench_id: String,
    accept: String,
}

#[async_trait]
impl<'r> FromRequest<'r> for BenchHeaders {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, ()> {
        let bench_id = req
            .headers()
            .get_one("X-Bench-Id")
            .unwrap_or("")
            .to_string();
        let accept = req.headers().get_one("Accept").unwrap_or("").to_string();
        Outcome::Success(BenchHeaders { bench_id, accept })
    }
}

#[derive(Serialize)]
struct HeadersResponse {
    id: String,
    accept: String,
}

struct FullHeaders(usize);

#[async_trait]
impl<'r> FromRequest<'r> for FullHeaders {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, ()> {
        let size = req
            .headers()
            .iter()
            .map(|header| header.name().len() + header.value().len())
            .sum();
        Outcome::Success(FullHeaders(size))
    }
}

#[get("/headers-full")]
fn headers_full(headers: FullHeaders) -> String {
    headers.0.to_string()
}

#[get("/headers")]
fn headers_route(h: BenchHeaders) -> Json<HeadersResponse> {
    Json(HeadersResponse {
        id: h.bench_id,
        accept: h.accept,
    })
}

#[rkt::main]
async fn main() -> Result<(), rkt::Error> {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(8000);

    let mut map = HashMap::new();
    for i in 0..1000 {
        map.insert(format!("key-{i}"), format!("value-{i}"));
    }
    let map: Map = Arc::new(RwLock::new(map));

    let figment = Figment::from(Config {
        log_level: None,
        cli_colors: CliColors::Never,
        shutdown: ShutdownConfig {
            ctrlc: false,
            #[cfg(unix)]
            signals: HashSet::new(),
            ..Default::default()
        },
        ..Default::default()
    })
    .merge(("port", port));

    rkt::custom(figment)
        .manage(map)
        .mount(
            "/",
            routes![
                ping,
                hello,
                state_get,
                query,
                query_owned,
                headers_route,
                headers_full,
                memory_1k,
                memory_64k,
                memory_1m,
                stream_slow,
            ],
        )
        .mount("/files", FileServer::new("./static"))
        .launch()
        .await?;

    Ok(())
}
