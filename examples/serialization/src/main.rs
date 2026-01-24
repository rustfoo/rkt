#[macro_use] extern crate rkt;

#[cfg(test)] mod tests;

mod json;
mod msgpack;
mod uuid;

#[launch]
fn rocket() -> _ {
    rkt::build()
        .attach(json::stage())
        .attach(msgpack::stage())
        .attach(uuid::stage())
}
