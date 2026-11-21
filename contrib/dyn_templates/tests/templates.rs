#[macro_use]
extern crate rkt;


use std::path::{Path, PathBuf};

use rkt::figment::value::Value;
use rkt::serde::{Deserialize, Serialize};
use rkt::Rocket;
use rkt::{config::Config, Build};
use rkt_dyn_templates::{context, Metadata, Template};

#[get("/<engine>/<name>")]
fn template_check(md: Metadata<'_>, engine: &str, name: &str) -> Option<()> {
    md.contains_template(&format!("{}/{}", engine, name))
        .then_some(())
}

#[get("/is_reloading")]
fn is_reloading(md: Metadata<'_>) -> Option<()> {
    if md.reloading() {
        Some(())
    } else {
        None
    }
}

#[allow(dead_code)]
fn template_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("templates")
}

#[allow(dead_code)]
fn rocket() -> Rocket<Build> {
    rkt::custom(Config::figment().merge(("template_dir", template_root())))
        .attach(Template::fairing())
        .mount("/", routes![template_check, is_reloading])
}

#[test]
fn test_callback_error() {
    use rkt::{error::ErrorKind::FailedFairings, local::blocking::Client};

    let rocket = rkt::build().attach(Template::try_custom(|_| {
        Err("error reloading templates!".into())
    }));

    let error = Client::debug(rocket).expect_err("client failure");
    match error.kind() {
        FailedFairings(failures) => assert_eq!(failures[0].name, "Templating"),
        _ => panic!("Wrong kind of launch error"),
    }
}

#[test]
fn test_sentinel() {
    use rkt::{error::ErrorKind::SentinelAborts, local::blocking::Client};

    let err = Client::debug_with(routes![is_reloading]).unwrap_err();
    assert!(matches!(err.kind(), SentinelAborts(vec) if vec.len() == 1));

    let err = Client::debug_with(routes![is_reloading, template_check]).unwrap_err();
    assert!(matches!(err.kind(), SentinelAborts(vec) if vec.len() == 2));

    #[get("/")]
    fn return_template() -> Template {
        Template::render("foo", ())
    }

    let err = Client::debug_with(routes![return_template]).unwrap_err();
    assert!(matches!(err.kind(), SentinelAborts(vec) if vec.len() == 1));

    #[get("/")]
    fn return_opt_template() -> Option<Template> {
        Some(Template::render("foo", ()))
    }

    let err = Client::debug_with(routes![return_opt_template]).unwrap_err();
    assert!(matches!(err.kind(), SentinelAborts(vec) if vec.len() == 1));

    #[derive(rkt::Responder)]
    struct MyThing<T>(T);

    #[get("/")]
    fn return_custom_template() -> MyThing<Template> {
        MyThing(Template::render("foo", ()))
    }

    let err = Client::debug_with(routes![return_custom_template]).unwrap_err();
    assert!(matches!(err.kind(), SentinelAborts(vec) if vec.len() == 1));

    #[derive(rkt::Responder)]
    struct MyOkayThing<T>(Option<T>);

    impl<T> rkt::Sentinel for MyOkayThing<T> {
        fn abort(_: &Rocket<rkt::Ignite>) -> bool {
            false
        }
    }

    #[get("/")]
    fn always_ok_sentinel() -> MyOkayThing<Template> {
        MyOkayThing(None)
    }

    Client::debug_with(routes![always_ok_sentinel]).expect("no sentinel abort");
}

#[test]
fn test_context_macro() {
    macro_rules! assert_same_object {
        ($ctx:expr, $obj:expr $(,)?) => {{
            let ser_ctx = Value::serialize(&$ctx).unwrap();
            let deser_ctx = ser_ctx.deserialize().unwrap();
            assert_eq!($obj, deser_ctx);
        }};
    }

    {
        #[derive(Deserialize, PartialEq, Debug)]
        #[serde(crate = "rkt::serde")]
        struct Empty {}

        assert_same_object!(context! {}, Empty {});
    }

    {
        #[derive(Deserialize, PartialEq, Debug)]
        #[serde(crate = "rkt::serde")]
        struct Object {
            a: u32,
            b: String,
        }

        let a = 93;
        let b = "Hello".to_string();

        fn make_context() -> impl Serialize {
            let b = "Hello".to_string();

            context! { a: 93, b: b }
        }

        assert_same_object!(make_context(), Object { a, b },);
    }

    {
        #[derive(Deserialize, PartialEq, Debug)]
        #[serde(crate = "rkt::serde")]
        struct Outer {
            s: String,
            inner: Inner,
        }

        #[derive(Deserialize, PartialEq, Debug)]
        #[serde(crate = "rkt::serde")]
        struct Inner {
            center: Center,
        }

        #[derive(Deserialize, PartialEq, Debug)]
        #[serde(crate = "rkt::serde")]
        struct Center {
            value_a: bool,
            value_b: u8,
        }

        let a = true;
        let value_b = 123;
        let outer_string = String::from("abc 123");

        assert_same_object!(
            context! {
                s: &outer_string,
                inner: context! {
                    center: context! {
                        value_a: a,
                        value_b,
                    },
                },
            },
            Outer {
                s: outer_string,
                inner: Inner {
                    center: Center {
                        value_a: a,
                        value_b,
                    },
                },
            },
        );
    }

    {
        #[derive(Deserialize, PartialEq, Debug)]
        #[serde(crate = "rkt::serde")]
        struct Object {
            a: String,
        }

        let owned = String::from("foo");
        let ctx = context! { a: &owned };
        assert_same_object!(ctx, Object { a: "foo".into() });
        //drop(ctx);
        drop(owned);
    }
}

#[cfg(feature = "tera")]
mod tera_tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rkt::http::{ContentType, Status};
    use rkt::request::FromRequest;
    use std::collections::HashMap;

    const UNESCAPED_EXPECTED: &str = "\nh_start\ntitle: _test_\nh_end\n\n\n<script />\n\nfoot";
    const ESCAPED_EXPECTED: &str =
        "\nh_start\ntitle: _test_\nh_end\n\n\n&lt;script /&gt;\n\nfoot";

    #[async_test]
    async fn test_tera_templates() {
        use rkt::local::asynchronous::Client;

        let client = Client::debug(rocket()).await.unwrap();
        let req = client.get("/");
        let metadata = Metadata::from_request(&req).await.unwrap();

        let mut map = HashMap::new();
        map.insert("title", "_test_");
        map.insert("content", "<script />");

        // Test with a txt file, which shouldn't escape.
        let template = Template::show(client.rocket(), "tera/txt_test", &map);
        let md_rendered = metadata.render("tera/txt_test", &map);
        assert_eq!(template, Some(UNESCAPED_EXPECTED.into()));
        assert_eq!(
            md_rendered,
            Some((ContentType::Text, UNESCAPED_EXPECTED.into()))
        );

        // Now with an HTML file, which should escape.
        let template = Template::show(client.rocket(), "tera/html_test", &map);
        let md_rendered = metadata.render("tera/html_test", &map);
        assert_eq!(template, Some(ESCAPED_EXPECTED.into()));
        assert_eq!(
            md_rendered,
            Some((ContentType::HTML, ESCAPED_EXPECTED.into()))
        );
    }

    #[async_test]
    async fn test_globby_paths() {
        use rkt::local::asynchronous::Client;

        let client = Client::debug(rocket()).await.unwrap();
        let req = client.get("/");
        let metadata = Metadata::from_request(&req).await.unwrap();
        assert!(metadata.contains_template("tera/[test]/html_test"));
    }

    #[test]
    fn test_tera_escaping() {
        use rkt::local::blocking::Client;

        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("sub");
        std::fs::create_dir(&dir).unwrap();
        for file in ["b.xml.tera", "c.htm.tera", "d.txt.tera", "e.tera"] {
            std::fs::write(dir.join(file), "{{ v }}").unwrap();
        }

        std::fs::write(root.path().join("a.html.tera"), "{{ v }}").unwrap();
        std::fs::write(dir.join("a.txt.tera"), "{{ v }}").unwrap();
        std::fs::write(
            root.path().join("base.html.tera"),
            "{% block text %}{% endblock %}",
        ).unwrap();
        std::fs::write(
            root.path().join("child.html.tera"),
            r#"{% extends "base" %}{% block text %}{% include "sub/a" %}{% endblock %}"#,
        ).unwrap();

        let figment = Config::figment().merge(("template_dir", root.path()));
        let rocket = rkt::custom(figment)
            .attach(Template::custom(|engines| {
                for name in ["raw.html", "raw.html.tera", "raw.htm.tera", "raw.xml.tera", "raw"] {
                    engines.tera.add_raw_template(name, "{{ v }}").unwrap();
                }

                engines.tera.add_raw_template("raw-child.html.tera",
                    r#"{% extends "base" %}{% block text %}{{ v }}{% endblock %}"#,
                ).unwrap();
            }));

        let client = Client::debug(rocket).unwrap();
        let render = |name| Template::show(client.rocket(), name, context! { v: "<b>" });
        for name in [
            "a", "sub/b", "sub/c", "child", "raw.html",
            "raw.html.tera", "raw.htm.tera", "raw.xml.tera", "raw-child.html.tera",
        ] {
            assert_eq!(render(name).unwrap(), "&lt;b&gt;", "{name} should be escaped");
        }

        for name in ["sub/a", "sub/d", "sub/e", "raw"] {
            assert_eq!(render(name).unwrap(), "<b>", "{name} should not be escaped");
        }
    }

    #[test]
    fn file_backed_tera_template_uses_custom_filter() {
        use rkt::local::blocking::Client;
        use rkt_dyn_templates::tera::{self, Value};

        fn decorate(value: &Value, _: tera::Kwargs, _: &tera::State) -> tera::TeraResult<Value> {
            Ok(Value::from(format!("[{}]", value.as_str().unwrap())))
        }

        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("filtered.txt.tera"), "{{ v | decorate }}").unwrap();
        let figment = Config::figment().merge(("template_dir", root.path()));
        let rocket = rkt::custom(figment).attach(Template::try_custom_with_preload(
            |engines| {
                engines.tera.register_filter("decorate", decorate);
                Ok(())
            },
            |engines| {
                assert!(engines.tera.contains_template("filtered"));
                Ok(())
            },
        ));

        let client = Client::debug(rocket).unwrap();
        let rendered = Template::show(client.rocket(), "filtered", context! { v: "value" });
        assert_eq!(rendered.as_deref(), Some("[value]"));
    }

    #[test]
    fn custom_registers_tera_items_used_by_template_files() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        use rkt::local::blocking::Client;
        use rkt_dyn_templates::tera::{self, Value};

        fn decorate(value: &Value, _: tera::Kwargs, _: &tera::State) -> tera::TeraResult<Value> {
            Ok(Value::from(format!("[{}]", value.as_str().unwrap())))
        }

        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("filtered.txt.tera"),
            "{{ v | decorate }}{% if v is short %}!{% endif %}{{ stamp() }}",
        ).unwrap();

        let calls = Arc::new(AtomicUsize::new(0));
        let callback_calls = calls.clone();
        let figment = Config::figment().merge(("template_dir", root.path()));
        let rocket = rkt::custom(figment).attach(Template::custom(move |engines| {
            callback_calls.fetch_add(1, Ordering::SeqCst);
            engines.tera.register_filter("decorate", decorate);
            engines.tera.register_test("short", |v: &str, _: tera::Kwargs, _: &tera::State| {
                v.len() < 10
            });
            engines.tera.register_function("stamp", |_: tera::Kwargs, _: &tera::State| "#");
        }));

        let client = Client::debug(rocket).unwrap();
        let rendered = Template::show(client.rocket(), "filtered", context! { v: "value" });
        assert_eq!(rendered.as_deref(), Some("[value]!#"));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "callback runs once per load");
    }

    #[async_test]
    async fn registered_tera_template_shadows_template_file() {
        use rkt::local::asynchronous::Client;

        let root = tempfile::tempdir().unwrap();
        // Named `report`: the file escapes HTML, the registered template doesn't.
        std::fs::write(root.path().join("report.html.tera"), "file {{ v }}").unwrap();
        // Named `page.html`: the file doesn't escape HTML, the registered one does.
        std::fs::write(root.path().join("page.html.txt.tera"), "file {{ v }}").unwrap();

        let figment = Config::figment().merge(("template_dir", root.path()));
        let rocket = rkt::custom(figment).attach(Template::try_custom_with_preload(
            |engines| {
                engines.tera.add_raw_template("report", "raw {{ v }}")?;
                engines.tera.add_raw_template("page.html", "raw {{ v }}")?;
                Ok(())
            },
            |_| Ok(()),
        ));

        let client = Client::debug(rocket).await.unwrap();
        let req = client.get("/");
        let metadata = Metadata::from_request(&req).await.unwrap();
        let render = |name| metadata.render(name, context! { v: "<b>" }).unwrap();

        assert_eq!(render("report"), (ContentType::Text, "raw <b>".into()));
        assert_eq!(render("page.html"), (ContentType::HTML, "raw &lt;b&gt;".into()));
    }

    #[cfg(unix)]
    #[test]
    fn tera_escapes_templates_under_non_utf8_paths() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(OsStr::from_bytes(b"templates-\xff"));
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("index.html.tera"), "{{ v }}").unwrap();

        let figment = Config::figment().merge(("template_dir", root.path()));
        let rocket = rkt::custom(figment).attach(Template::fairing());
        let client = rkt::local::blocking::Client::debug(rocket).unwrap();
        let rendered = Template::show(client.rocket(), "templates-�/index", context! { v: "<b>" });
        assert_eq!(rendered.as_deref(), Some("&lt;b&gt;"));
    }

    // u128 is not supported. enable when it is.
    // #[test]
    // fn test_tera_u128() {
    //     const EXPECTED: &'static str
    //         = "\nh_start\ntitle: 123\nh_end\n\n\n1208925819614629174706176\n\nfoot\n";
    //
    //     let client = Client::debug(rocket()).unwrap();
    //     let mut map = HashMap::new();
    //     map.insert("title", 123);
    //     map.insert("number", 1u128 << 80);
    //
    //     let template = Template::show(client.rocket(), "tera/txt_test", &map);
    //     assert_eq!(template, Some(EXPECTED.into()));
    // }

    #[test]
    fn test_template_metadata_with_tera() {
        use rkt::local::blocking::Client;

        let client = Client::debug(rocket()).unwrap();

        let response = client.get("/tera/txt_test").dispatch();
        assert_eq!(response.status(), Status::Ok);

        let response = client.get("/tera/html_test").dispatch();
        assert_eq!(response.status(), Status::Ok);

        let response = client.get("/tera/not_existing").dispatch();
        assert_eq!(response.status(), Status::NotFound);

        let response = client.get("/hbs/txt_test").dispatch();
        assert_eq!(response.status(), Status::NotFound);
    }
}

#[cfg(feature = "handlebars")]
mod handlebars_tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rkt::http::{ContentType, Status};
    use rkt::request::FromRequest;
    use std::collections::HashMap;

    #[async_test]
    async fn test_handlebars_templates() {
        use rkt::local::asynchronous::Client;

        const EXPECTED: &'static str = "Hello _test_!\n<main> &lt;script /&gt; hi </main>\nDone.\n";

        let client = Client::debug(rocket()).await.unwrap();
        let req = client.get("/");
        let metadata = Metadata::from_request(&req).await.unwrap();

        let mut map = HashMap::new();
        map.insert("title", "_test_");
        map.insert("content", "<script /> hi");

        // Test with a txt file, which shouldn't escape.
        let template = Template::show(client.rocket(), "hbs/test", &map);
        let md_rendered = metadata.render("hbs/test", &map);
        assert_eq!(template, Some(EXPECTED.into()));
        assert_eq!(md_rendered, Some((ContentType::HTML, EXPECTED.into())));
    }

    // u128 is not supported. enable when it is.
    // #[test]
    // fn test_handlebars_u128() {
    //     const EXPECTED: &'static str
    //         = "Hello 123!\n\n<main> 1208925819614629174706176 </main>\nDone.\n\n";
    //
    //     let client = Client::debug(rocket()).unwrap();
    //     let mut map = HashMap::new();
    //     map.insert("title", 123);
    //     map.insert("number", 1u128 << 80);
    //
    //     let template = Template::show(client.rocket(), "hbs/test", &map);
    //     assert_eq!(template, Some(EXPECTED.into()));
    // }

    #[test]
    fn test_template_metadata_with_handlebars() {
        use rkt::local::blocking::Client;

        let client = Client::debug(rocket()).unwrap();

        let response = client.get("/hbs/test").dispatch();
        assert_eq!(response.status(), Status::Ok);

        let response = client.get("/hbs/not_existing").dispatch();
        assert_eq!(response.status(), Status::NotFound);

        let response = client.get("/tera/test").dispatch();
        assert_eq!(response.status(), Status::NotFound);
    }

    #[test]
    #[cfg(debug_assertions)]
    fn test_template_reload() {
        use std::fs::File;
        use std::io::Write;
        use std::time::Duration;

        use rkt::local::blocking::Client;

        const RELOAD_TEMPLATE: &str = "hbs/reload";
        const INITIAL_TEXT: &str = "initial";
        const NEW_TEXT: &str = "reload";

        fn write_file(path: &Path, text: &str) {
            let mut file = File::create(path).expect("open file");
            file.write_all(text.as_bytes()).expect("write file");
            file.sync_all().expect("sync file");
        }

        // set up the template before initializing the rkt instance so
        // that it will be picked up in the initial loading of templates.
        let reload_path = template_root().join("hbs").join("reload.txt.hbs");
        write_file(&reload_path, INITIAL_TEXT);

        // set up the client. if we can't reload templates, then just quit
        let client = Client::debug(rocket()).unwrap();
        let res = client.get("/is_reloading").dispatch();
        if res.status() != Status::Ok {
            return;
        }

        // verify that the initial content is correct
        let initial_rendered = Template::show(client.rocket(), RELOAD_TEMPLATE, ());
        assert_eq!(initial_rendered, Some(INITIAL_TEXT.into()));

        // write a change to the file
        write_file(&reload_path, NEW_TEXT);

        for _ in 0..6 {
            // dispatch any request to trigger a template reload
            client.get("/").dispatch();

            // if the new content is correct, we are done
            let new_rendered = Template::show(client.rocket(), RELOAD_TEMPLATE, ());
            if new_rendered == Some(NEW_TEXT.into()) {
                write_file(&reload_path, INITIAL_TEXT);
                return;
            }

            // otherwise, retry a few times, waiting 250ms in between
            std::thread::sleep(Duration::from_millis(250));
        }

        panic!("failed to reload modified template in 1.5s");
    }
}

#[cfg(feature = "minijinja")]
mod j2_tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rkt::http::{ContentType, Status};
    use rkt::request::FromRequest;
    use std::collections::HashMap;

    const UNESCAPED_EXPECTED: &'static str =
        "\nh_start\ntitle: _test_\nh_end\n\n\n<script />\n\nfoot";
    const ESCAPED_EXPECTED: &'static str =
        "\nh_start\ntitle: _test_\nh_end\n\n\n&lt;script &#x2f;&gt;\n\nfoot";

    #[async_test]
    async fn test_j2_templates() {
        use rkt::local::asynchronous::Client;

        let client = Client::debug(rocket()).await.unwrap();
        let req = client.get("/");
        let metadata = Metadata::from_request(&req).await.unwrap();

        let mut map = HashMap::new();
        map.insert("title", "_test_");
        map.insert("content", "<script />");

        // Test with a txt file, which shouldn't escape.
        let template = Template::show(client.rocket(), "j2/txt_test", &map);
        let md_rendered = (&metadata).render("j2/txt_test", &map);
        assert_eq!(template, Some(UNESCAPED_EXPECTED.into()));
        assert_eq!(
            md_rendered,
            Some((ContentType::Text, UNESCAPED_EXPECTED.into()))
        );

        // Now with an HTML file, which should escaped.
        let template = Template::show(client.rocket(), "j2/html_test", &map);
        let md_rendered = metadata.render("j2/html_test", &map);
        assert_eq!(template, Some(ESCAPED_EXPECTED.into()));
        assert_eq!(
            md_rendered,
            Some((ContentType::HTML, ESCAPED_EXPECTED.into()))
        );
    }

    #[async_test]
    async fn test_globby_paths() {
        use rkt::local::asynchronous::Client;

        let client = Client::debug(rocket()).await.unwrap();
        let req = client.get("/");
        let metadata = Metadata::from_request(&req).await.unwrap();
        assert!(metadata.contains_template("j2/[test]/html_test"));
    }

    #[test]
    fn test_j2_u128() {
        const EXPECTED: &'static str =
            "\nh_start\ntitle: 123\nh_end\n\n\n1208925819614629174706176\n\nfoot";

        use rkt::local::blocking::Client;

        let client = Client::debug(rocket()).unwrap();
        let mut map = HashMap::new();
        map.insert("title", 123);
        map.insert("content", 1u128 << 80);

        let template = Template::show(client.rocket(), "j2/txt_test", &map);
        assert_eq!(template, Some(EXPECTED.into()));
    }

    #[test]
    fn test_template_metadata_with_j2() {
        use rkt::local::blocking::Client;

        let client = Client::debug(rocket()).unwrap();

        let response = client.get("/j2/txt_test").dispatch();
        assert_eq!(response.status(), Status::Ok);

        let response = client.get("/j2/html_test").dispatch();
        assert_eq!(response.status(), Status::Ok);

        let response = client.get("/j2/not_existing").dispatch();
        assert_eq!(response.status(), Status::NotFound);

        let response = client.get("/hbs/txt_test").dispatch();
        assert_eq!(response.status(), Status::NotFound);
    }
}
