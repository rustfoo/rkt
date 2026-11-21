#![cfg(feature = "xml")]

#[macro_use] extern crate rkt;

use rkt::http::{ContentType, Status};
use rkt::serde::{Serialize, Deserialize, xml::{self, Xml}};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(crate = "rkt::serde")]
struct Task {
    id: usize,
    complete: bool,
    text: String,
}

#[post("/echo", format = "xml", data = "<task>")]
fn echo(task: Xml<Task>) -> Xml<Task> { task }

#[get("/trailing")]
fn trailing() -> rkt::response::content::RawXml<String> {
    let mut body = xml::to_string(&task()).unwrap();
    body.push_str("<extra/>");
    rkt::response::content::RawXml(body)
}

fn task() -> Task {
    Task { id: 10, complete: false, text: "write <tests> & docs".into() }
}

#[test]
fn blocking_xml_works() {
    use rkt::local::blocking::Client;

    let client = Client::debug_with(routes![echo, trailing]).unwrap();
    let request = client.post("/echo").xml(&task());
    assert_eq!(request.content_type(), Some(&ContentType::XML));

    let response = request.dispatch();
    assert_eq!(response.content_type(), Some(ContentType::XML));
    assert_eq!(response.into_xml::<Task>(), Some(task()));

    #[derive(Debug, Deserialize)]
    #[serde(crate = "rkt::serde")]
    struct Missing { #[allow(dead_code)] missing: String }

    let response = client.post("/echo").xml(&task()).dispatch();
    assert!(response.into_xml::<Missing>().is_none());
    assert!(client.get("/trailing").dispatch().into_xml::<Task>().is_none());
}

#[async_test]
async fn async_xml_works() {
    use rkt::local::asynchronous::Client;

    let client = Client::debug_with(routes![echo, trailing]).await.unwrap();
    let response = client.post("/echo").xml(&task()).dispatch().await;
    assert_eq!(response.into_xml::<Task>().await, Some(task()));
    let response = client.get("/trailing").dispatch().await;
    assert!(response.into_xml::<Task>().await.is_none());
}

#[test]
fn xml_body_limit_is_configurable() {
    use rkt::data::{Limits, ToByteUnit};
    use rkt::local::blocking::Client;

    assert_eq!(Limits::default().get("xml"), Some(1.mebibytes()));

    let body = xml::to_string(&task()).unwrap();
    let mut config = rkt::Config::debug_default();
    config.limits = config.limits.limit("xml", body.len().bytes());
    let client = Client::debug(rkt::custom(config).mount("/", routes![echo])).unwrap();

    let at_limit = client.post("/echo").header(ContentType::XML).body(&body).dispatch();
    assert_eq!(at_limit.status(), Status::Ok);
    let beyond_limit = client.post("/echo")
        .header(ContentType::XML)
        .body(format!("{body} "))
        .dispatch();
    assert_eq!(beyond_limit.status(), Status::PayloadTooLarge);
}

#[test]
fn trailing_content_is_rejected() {
    use rkt::local::blocking::Client;

    let task = "<Task><id>1</id><complete>true</complete><text>hi</text></Task>";
    let decl = "<?xml version=\"1.0\"?>";
    let valid = [
        task.to_string(),
        format!("{task}\n "),
        format!("{task}<!-- comment -->"),
        format!("{task}\n<?pi x?>\n"),
        format!("{decl}\n<!-- comment -->\n{task}"),
        format!("<!DOCTYPE Task>{task}"),
    ];

    for input in &valid {
        assert!(xml::from_str::<Task>(input).is_ok(), "{input:?}");
    }

    let invalid = [
        format!("{task}{task}"),
        format!("{task}text"),
        format!("{task}</Task>"),
        format!("{task}\u{a0}"),
        format!("{task}<![CDATA[text]]>"),
        format!("{task}<!DOCTYPE Task>"),
        format!("text{task}"),
        String::new(),
    ];

    for input in &invalid {
        assert!(xml::from_str::<Task>(input).is_err(), "{input:?}");
    }

    let sequence: Vec<String> = xml::from_str("<a>hi</a><a>yo</a>").unwrap();
    assert_eq!(sequence, ["hi", "yo"]);

    let client = Client::debug_with(routes![echo]).unwrap();
    for body in [format!("{task}{task}"), format!("{task}\u{a0}")] {
        let request = client.post("/echo").header(ContentType::XML).body(&body);
        assert_eq!(request.dispatch().status(), Status::BadRequest, "{body:?}");
    }

    let body = format!("{decl}\n{task}\n");
    let response = client.post("/echo").header(ContentType::XML).body(body).dispatch();
    assert_eq!(response.status(), Status::Ok);
    let expected = Task { id: 1, complete: true, text: "hi".into() };
    assert_eq!(response.into_xml::<Task>(), Some(expected));
}
