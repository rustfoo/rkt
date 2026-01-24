extern crate rkt;

#[test]
#[should_panic]
fn bad_dynamic_mount() {
    let _ = rkt::build().mount("<name>", vec![]);
}

#[test]
fn good_static_mount() {
    let _ = rkt::build().mount("/abcdefghijkl_mno", vec![]);
}
