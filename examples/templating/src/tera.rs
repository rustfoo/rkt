use rkt::Request;
use rkt::response::Redirect;

use rkt_dyn_templates::{Template, context};

#[get("/")]
pub fn index() -> Redirect {
    Redirect::to(uri!("/tera", hello(name = "Your Name")))
}

#[get("/hello/<name>")]
pub fn hello(name: &str) -> Template {
    Template::render("tera/index", context! {
        title: "Hello",
        name: Some(name),
        items: vec!["One", "Two", "Three"],
    })
}

#[get("/about")]
pub fn about() -> Template {
    Template::render("tera/about", context! {
        title: "About",
    })
}

#[catch(404)]
pub fn not_found(req: &Request<'_>) -> Template {
    Template::render("tera/error/404", context! {
        uri: req.uri()
    })
}
