#[cfg(test)]
mod tests;

use rkt::{Request, Route, Catcher, route, catcher};
use rkt::data::{Data, ToByteUnit};
use rkt::http::{Status, Method::{Get, Post}};
use rkt::response::{Responder, status::Custom};
use rkt::outcome::{try_outcome, IntoOutcome};
use rkt::tokio::fs::File;

fn forward<'r>(_req: &'r Request, data: Data<'r>) -> route::BoxFuture<'r> {
    Box::pin(async move { route::Outcome::forward(data, Status::NotFound) })
}

fn hi<'r>(req: &'r Request, _: Data<'r>) -> route::BoxFuture<'r> {
    route::Outcome::from(req, "Hello!").pin()
}

fn name<'r>(req: &'r Request, _: Data<'r>) -> route::BoxFuture<'r> {
    let param = req.param::<&'r str>(0)
        .and_then(Result::ok)
        .unwrap_or("unnamed");

    route::Outcome::from(req, param).pin()
}

fn echo_url<'r>(req: &'r Request, _: Data<'r>) -> route::BoxFuture<'r> {
    let param_outcome = req.param::<&str>(1)
        .and_then(Result::ok)
        .or_error(Status::BadRequest);

    Box::pin(async move {
        route::Outcome::from(req, try_outcome!(param_outcome))
    })
}

fn upload<'r>(req: &'r Request, data: Data<'r>) -> route::BoxFuture<'r> {
    Box::pin(async move {
        if !req.content_type().map_or(false, |ct| ct.is_plain()) {
            println!("    => Content-Type of upload must be text/plain. Ignoring.");
            return route::Outcome::error(Status::BadRequest);
        }

        let path = req.rocket().config().temp_dir.relative().join("upload.txt");
        let file = File::create(path).await;
        if let Ok(file) = file {
            if let Ok(n) = data.open(2.mebibytes()).stream_to(file).await {
                return route::Outcome::from(req, format!("OK: {} bytes uploaded.", n));
            }

            println!("    => Failed copying.");
            route::Outcome::error(Status::InternalServerError)
        } else {
            println!("    => Couldn't open file: {:?}", file.unwrap_err());
            route::Outcome::error(Status::InternalServerError)
        }
    })
}

fn get_upload<'r>(req: &'r Request, _: Data<'r>) -> route::BoxFuture<'r> {
    let path = req.rocket().config().temp_dir.relative().join("upload.txt");
    route::Outcome::from(req, std::fs::File::open(path).ok()).pin()
}

fn not_found_handler<'r>(_: Status, req: &'r Request) -> catcher::BoxFuture<'r> {
    let responder = Custom(Status::NotFound, format!("Couldn't find: {}", req.uri()));
    Box::pin(async move { responder.respond_to(req) })
}

#[derive(Clone)]
struct CustomHandler {
    data: &'static str
}

impl CustomHandler {
    fn routes(data: &'static str) -> Vec<Route> {
        vec![Route::new(Get, "/<id>", Self { data })]
    }
}

#[rkt::async_trait]
impl route::Handler for CustomHandler {
    async fn handle<'r>(&self, req: &'r Request<'_>, data: Data<'r>) -> route::Outcome<'r> {
        let self_data = self.data;
        let id = req.param::<&str>(0)
            .and_then(Result::ok)
            .or_forward((data, Status::NotFound));

        route::Outcome::from(req, format!("{} - {}", self_data, try_outcome!(id)))
    }
}

#[rkt::launch]
fn rocket() -> _ {
    let always_forward = Route::ranked(1, Get, "/", forward);
    let hello = Route::ranked(2, Get, "/", hi);

    let echo = Route::new(Get, "/echo/<str>", echo_url);
    let name = Route::new(Get, "/<name>", name);
    let post_upload = Route::new(Post, "/", upload);
    let get_upload = Route::new(Get, "/", get_upload);

    let not_found_catcher = Catcher::new(404, not_found_handler);

    rkt::build()
        .mount("/", vec![always_forward, hello, echo])
        .mount("/upload", vec![get_upload, post_upload])
        .mount("/hello", vec![name.clone()])
        .mount("/hi", vec![name])
        .mount("/custom", CustomHandler::routes("some data here"))
        .register("/", vec![not_found_catcher])
}
