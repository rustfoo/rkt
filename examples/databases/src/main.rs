#[macro_use] extern crate rkt;
#[macro_use] extern crate rkt_sync_db_pools;

#[cfg(test)] mod tests;

mod sqlx;
mod diesel_sqlite;
mod diesel_mysql;
mod rusqlite;

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
        .attach(rusqlite::stage())
        .attach(diesel_sqlite::stage())
        .attach(diesel_mysql::stage())
}
