#[macro_use] extern crate rkt;

fn main() {
    let _ = catchers![a b];
    let _ = catchers![];
    let _ = catchers![a::, ];
    let _ = catchers![a::];
}
