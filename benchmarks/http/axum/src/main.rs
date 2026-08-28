use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{
    body::{Body, Bytes},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::services::ServeDir;

type Map = Arc<RwLock<HashMap<String, String>>>;

async fn ping() -> &'static str {
    "pong"
}

#[derive(Serialize)]
struct HelloResponse {
    message: &'static str,
}

async fn hello() -> Json<HelloResponse> {
    Json(HelloResponse { message: "hello" })
}

async fn state_get(Path(key): Path<String>, State(map): State<Map>) -> (StatusCode, String) {
    match map.read().unwrap().get(&key).cloned() {
        Some(val) => (StatusCode::OK, val),
        None => (StatusCode::NOT_FOUND, String::new()),
    }
}

#[derive(Deserialize)]
struct QueryParams {
    msg: String,
    n: u32,
}

#[derive(Serialize)]
struct QueryResponse {
    msg: String,
    n: u32,
}

async fn query(Query(params): Query<QueryParams>) -> Json<QueryResponse> {
    Json(QueryResponse {
        msg: params.msg,
        n: params.n,
    })
}

static MEMORY_1K: [u8; 1024] = [b'x'; 1024];
static MEMORY_64K: [u8; 64 * 1024] = [b'x'; 64 * 1024];
static MEMORY_1M: [u8; 1024 * 1024] = [b'x'; 1024 * 1024];
static STREAM_CHUNK: [u8; 1024] = [b'x'; 1024];

async fn memory_1k() -> Bytes {
    Bytes::from_static(&MEMORY_1K)
}

async fn memory_64k() -> Bytes {
    Bytes::from_static(&MEMORY_64K)
}

async fn memory_1m() -> Bytes {
    Bytes::from_static(&MEMORY_1M)
}

async fn stream_slow() -> impl IntoResponse {
    let stream = async_stream::stream! {
        for _ in 0..16 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            yield Ok::<_, Infallible>(Bytes::from_static(&STREAM_CHUNK));
        }
    };

    (
        [(header::CONTENT_TYPE, "application/octet-stream")],
        Body::from_stream(stream),
    )
}

#[derive(Serialize)]
struct HeadersResponse {
    id: String,
    accept: String,
}

async fn headers_handler(headers: HeaderMap) -> Json<HeadersResponse> {
    let id = headers
        .get("x-bench-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    Json(HeadersResponse { id, accept })
}

async fn headers_full(headers: HeaderMap) -> String {
    headers
        .iter()
        .map(|(name, value)| name.as_str().len() + value.as_bytes().len())
        .sum::<usize>()
        .to_string()
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(8001);

    let mut map = HashMap::new();
    for i in 0..1000 {
        map.insert(format!("key-{i}"), format!("value-{i}"));
    }
    let map: Map = Arc::new(RwLock::new(map));

    let app = Router::new()
        .route("/ping", get(ping))
        .route("/hello", get(hello))
        .route("/state/{key}", get(state_get))
        .route("/query", get(query))
        .route("/query-owned", get(query))
        .route("/headers", get(headers_handler))
        .route("/headers-full", get(headers_full))
        .route("/memory/1k", get(memory_1k))
        .route("/memory/64k", get(memory_64k))
        .route("/memory/1m", get(memory_1m))
        .route("/stream-slow", get(stream_slow))
        .nest_service("/files", ServeDir::new("./static"))
        .with_state(map);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
