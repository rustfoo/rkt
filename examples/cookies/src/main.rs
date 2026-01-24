#[macro_use] extern crate rkt;

#[cfg(test)] mod tests;

mod session;
mod message;

use rkt::response::content::RawHtml;
use rkt_dyn_templates::Template;

#[get("/")]
fn index() -> RawHtml<&'static str> {
    RawHtml(r#"<a href="message">Set a Message</a> or <a href="session">Use Sessions</a>."#)
}

#[launch]
fn rocket() -> _ {
    rkt::build()
        .attach(Template::fairing())
        .mount("/", routes![index])
        .mount("/message", message::routes())
        .mount("/session", session::routes())
}
