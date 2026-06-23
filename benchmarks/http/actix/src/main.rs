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
    web::Json(QueryResponse {
        msg: params.msg.clone(),
        n: params.n,
    })
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
            .service(headers_route)
            .service(Files::new("/files", "./static"))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
