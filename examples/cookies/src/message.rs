use rkt::form::Form;
use rkt::response::Redirect;
use rkt::http::CookieJar;
use rkt_dyn_templates::{Template, context};

#[macro_export]
macro_rules! message_uri {
    ($($t:tt)*) => (rkt::uri!("/message", $crate::message:: $($t)*))
}

pub use message_uri as uri;

#[post("/", data = "<message>")]
fn submit(cookies: &CookieJar<'_>, message: Form<&str>) -> Redirect {
    cookies.add(("message", message.to_string()));
    Redirect::to(uri!(index))
}

#[delete("/")]
fn delete(cookies: &CookieJar<'_>) -> Redirect {
    cookies.remove("message");
    Redirect::to(uri!(index))
}

#[get("/")]
fn index(cookies: &CookieJar<'_>) -> Template {
    let message = cookies.get("message").map(|c| c.value());
    let present = cookies.get("message").is_some();
    Template::render("message", context! { present, message })
}

pub fn routes() -> Vec<rkt::Route> {
    routes![index, submit, delete]
}
