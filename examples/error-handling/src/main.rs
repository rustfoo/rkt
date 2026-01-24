#[macro_use] extern crate rkt;

#[cfg(test)] mod tests;

use rkt::{Rocket, Request, Build};
use rkt::response::{content, status};
use rkt::http::Status;

#[get("/hello/<name>/<age>")]
fn hello(name: &str, age: i8) -> String {
    format!("Hello, {} year old named {}!", age, name)
}

#[get("/<code>")]
fn forced_error(code: u16) -> Status {
    Status::new(code)
}

#[catch(404)]
fn general_not_found() -> content::RawHtml<&'static str> {
    content::RawHtml(r#"
        <p>Hmm... What are you looking for?</p>
        Say <a href="/hello/Sergio/100">hello!</a>
    "#)
}

#[catch(404)]
fn hello_not_found(req: &Request<'_>) -> content::RawHtml<String> {
    content::RawHtml(format!("\
        <p>Sorry, but '{}' is not a valid path!</p>\
        <p>Try visiting /hello/&lt;name&gt;/&lt;age&gt; instead.</p>",
        req.uri()))
}

#[catch(default)]
fn sergio_error() -> &'static str {
    "I...don't know what to say."
}

#[catch(default)]
fn default_catcher(status: Status, req: &Request<'_>) -> status::Custom<String> {
    let msg = format!("{} ({})", status, req.uri());
    status::Custom(status, msg)
}

#[allow(dead_code)]
#[get("/unmanaged")]
fn unmanaged(_u8: &rkt::State<u8>, _string: &rkt::State<String>) { }

fn rocket() -> Rocket<Build> {
    rkt::build()
        // .mount("/", routes![hello, hello]) // uncomment this to get an error
        // .mount("/", routes![unmanaged]) // uncomment this to get a sentinel error
        .mount("/", routes![hello, forced_error])
        .register("/", catchers![general_not_found, default_catcher])
        .register("/hello", catchers![hello_not_found])
        .register("/hello/Sergio", catchers![sergio_error])
}

#[rkt::main]
async fn main() {
    if let Err(e) = rocket().launch().await {
        println!("Whoops! Rocket didn't launch!");
        // We drop the error to get a Rocket-formatted panic.
        drop(e);
    };
}
