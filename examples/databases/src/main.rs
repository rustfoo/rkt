#[macro_use] extern crate rkt;

#[cfg(test)] mod tests;

mod sqlx;
mod diesel_sqlite;

use rkt::response::Redirect;

#[get("/")]
fn index() -> Redirect {
    Redirect::to(uri!("/sqlx", sqlx::list()))
}

#[launch]
fn rocket() -> _ {
    rkt::build()
        .mount("/", routes![index])
        .attach(sqlx::stage())
        .attach(diesel_sqlite::stage())
}
