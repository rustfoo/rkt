// must-compile-successfully

#[macro_use] extern crate rkt;

// Check for unknown media types.

#[get("/", format = "application/x-custom")]
fn f0() {}

#[get("/", format = "x-custom/plain")]
fn f1() {}

#[get("/", format = "x-custom/x-custom")]
fn f2() {}

#[suppress(unknown_format)]
#[get("/", format = "x-custom/x-custom")]
fn f3() {}

// Check if a data argument is used with a usually non-payload bearing method.

#[get("/", data = "<_foo>")]
fn g0(_foo: rkt::Data<'_>) {}

#[head("/", data = "<_foo>")]
fn g1(_foo: rkt::Data<'_>) {}

#[suppress(dubious_payload)]
#[head("/", data = "<_foo>")]
fn g2(_foo: rkt::Data<'_>) {}

fn main() {
    compile_error!("checking for warnings!")
}
