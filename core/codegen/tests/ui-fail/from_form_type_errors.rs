#[macro_use] extern crate rkt;

struct Unknown;

#[derive(FromForm)]
struct BadType3 {
    field: Unknown,
}

struct Foo<T>(T);

#[derive(FromForm)]
struct Other {
    field: Foo<usize>,
}

fn main() {  }
