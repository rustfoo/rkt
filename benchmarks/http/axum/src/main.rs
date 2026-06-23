use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
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
    Json(QueryResponse { msg: params.msg, n: params.n })
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
        .route("/headers", get(headers_handler))
        .nest_service("/files", ServeDir::new("./static"))
        .with_state(map);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
