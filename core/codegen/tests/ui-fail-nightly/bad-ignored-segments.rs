#[macro_use] extern crate rkt;

#[get("/<_>")]
fn i0() {}

#[get("/c?<_>")]
fn i1() {}

#[post("/d", data = "<_>")]
fn i2() {}

fn main() {  }
