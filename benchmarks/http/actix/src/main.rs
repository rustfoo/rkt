use std::collections::HashMap;
use std::sync::RwLock;

use actix_files::Files;
use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};

type Map = RwLock<HashMap<String, String>>;

#[get("/ping")]
async fn ping() -> impl Responder {
    "pong"
}

#[derive(Serialize)]
struct HelloResponse {
    message: &'static str,
}

#[get("/hello")]
async fn hello() -> impl Responder {
    web::Json(HelloResponse { message: "hello" })
}

#[get("/state/{key}")]
async fn state_get(key: web::Path<String>, data: web::Data<Map>) -> impl Responder {
    match data.read().unwrap().get(key.as_str()).cloned() {
        Some(val) => HttpResponse::Ok().body(val),
        None => HttpResponse::NotFound().finish(),
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

#[get("/query")]
async fn query(params: web::Query<QueryParams>) -> impl Responder {
    let QueryParams { msg, n } = params.into_inner();
    web::Json(QueryResponse { msg, n })
}

#[get("/query-owned")]
async fn query_owned(params: web::Query<QueryParams>) -> impl Responder {
    let QueryParams { msg, n } = params.into_inner();
    web::Json(QueryResponse { msg, n })
}

static MEMORY_1K: [u8; 1024] = [b'x'; 1024];
static MEMORY_64K: [u8; 64 * 1024] = [b'x'; 64 * 1024];
static MEMORY_1M: [u8; 1024 * 1024] = [b'x'; 1024 * 1024];
static STREAM_CHUNK: [u8; 1024] = [b'x'; 1024];

#[get("/memory/1k")]
async fn memory_1k() -> impl Responder {
    web::Bytes::from_static(&MEMORY_1K)
}

#[get("/memory/64k")]
async fn memory_64k() -> impl Responder {
    web::Bytes::from_static(&MEMORY_64K)
}

#[get("/memory/1m")]
async fn memory_1m() -> impl Responder {
    web::Bytes::from_static(&MEMORY_1M)
}

#[get("/stream-slow")]
async fn stream_slow() -> impl Responder {
    let stream = async_stream::stream! {
        for _ in 0..16 {
            actix_web::rt::time::sleep(std::time::Duration::from_millis(5)).await;
            yield Ok::<_, actix_web::Error>(web::Bytes::from_static(&STREAM_CHUNK));
        }
    };

    HttpResponse::Ok()
        .content_type("application/octet-stream")
        .streaming(stream)
}

#[derive(Serialize)]
struct HeadersResponse {
    id: String,
    accept: String,
}

#[get("/headers")]
async fn headers_route(req: HttpRequest) -> impl Responder {
    let id = req
        .headers()
        .get("x-bench-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let accept = req
        .headers()
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    web::Json(HeadersResponse { id, accept })
}

#[get("/headers-full")]
async fn headers_full(req: HttpRequest) -> impl Responder {
    req.headers()
        .iter()
        .map(|(name, value)| name.as_str().len() + value.as_bytes().len())
        .sum::<usize>()
        .to_string()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(8002);

    let map: web::Data<Map> = web::Data::new(RwLock::new({
        let mut m = HashMap::new();
        for i in 0..1000 {
            m.insert(format!("key-{i}"), format!("value-{i}"));
        }
        m
    }));

    HttpServer::new(move || {
        App::new()
            .app_data(map.clone())
            .service(ping)
            .service(hello)
            .service(state_get)
            .service(query)
            .service(query_owned)
            .service(headers_route)
            .service(headers_full)
            .service(memory_1k)
            .service(memory_64k)
            .service(memory_1m)
            .service(stream_slow)
            .service(Files::new("/files", "./static"))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
