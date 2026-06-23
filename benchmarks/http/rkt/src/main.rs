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
use rkt::serde::json::Json;
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

struct BenchHeaders {
    bench_id: String,
    accept: String,
}

#[async_trait]
impl<'r> FromRequest<'r> for BenchHeaders {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, ()> {
        let bench_id = req.headers().get_one("X-Bench-Id").unwrap_or("").to_string();
        let accept = req.headers().get_one("Accept").unwrap_or("").to_string();
        Outcome::Success(BenchHeaders { bench_id, accept })
    }
}

#[derive(Serialize)]
struct HeadersResponse {
    id: String,
    accept: String,
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
        .mount("/", routes![ping, hello, state_get, query, headers_route])
        .mount("/files", FileServer::new("./static"))
        .launch()
        .await?;

    Ok(())
}
