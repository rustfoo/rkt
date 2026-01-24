#[macro_use] extern crate rkt;

#[cfg(test)] mod tests;

mod request_local;
mod managed_hit_count;
mod managed_queue;

#[launch]
fn rocket() -> _ {
    rkt::build()
        .attach(request_local::stage())
        .attach(managed_hit_count::stage())
        .attach(managed_queue::stage())
}
