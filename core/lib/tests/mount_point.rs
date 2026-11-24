extern crate rkt;

#[test]
#[should_panic]
fn bad_dynamic_mount() {
    let _ = rkt::build().mount("<name>", vec![]);
}

#[test]
#[should_panic]
fn bad_dynamic_utf8_mount() {
    let _ = rkt::build().mount("/häcker/<x>", vec![]);
}

#[test]
#[should_panic]
fn bad_dynamic_nested_mount() {
    let _ = rkt::build().mount("/a/<x>", vec![]);
}

#[test]
#[should_panic]
fn bad_dynamic_register() {
    let _ = rkt::build().register("/a/<x>", vec![]);
}

#[test]
fn good_static_mount() {
    let _ = rkt::build().mount("/abcdefghijkl_mno", vec![]);
}

#[test]
fn good_utf8_mount() {
    let _ = rkt::build().mount("/häcker", vec![]).register("/häcker", vec![]);
}

#[test]
fn nested_utf8_catcher_rebase() {
    use rkt::Catcher;
    use rkt::catcher::dummy_handler;

    let catcher = Catcher::new(404, dummy_handler).rebase("/wörld").rebase("/häcker");
    assert_eq!(catcher.base(), "/häcker/wörld");
}

#[test]
fn whitespace_bases() {
    use rkt::{Route, Catcher};
    use rkt::http::Method;

    let route = || Route::new(Method::Get, "/", rkt::route::dummy_handler);
    let catcher = || Catcher::new(404, rkt::catcher::dummy_handler);
    for base in ["/a ", "/a/ ", "/ "] {
        let rebased = route().rebase(base);
        assert_eq!(rebased.uri.base(), base);
        assert_eq!(rebased.uri.path(), base);
        assert_eq!(route().map_base(|_| base.into()).unwrap().uri, base);
        assert_eq!(catcher().rebase(base).base(), base);
        assert_eq!(catcher().map_base(|_| base.into()).unwrap().base(), base);
    }

    assert_eq!(route().rebase("/b ").rebase("/a ").uri, "/a /b ");
    assert_eq!(catcher().rebase("/b ").rebase("/a ").base(), "/a /b ");

    assert!(!route().rebase("/a ").collides_with(&route().rebase("/a")));
    assert!(!catcher().rebase("/a ").collides_with(&catcher().rebase("/a")));
}
