#[macro_use]
extern crate rkt;

use rkt::form::Form;
use rkt::fs::TempFile;
use rkt::http::ContentType;
use rkt::tokio::io::AsyncReadExt;

#[derive(FromForm)]
struct Upload<'r> {
    file: TempFile<'r>,
    note: &'r str,
}

#[rkt::post("/", data = "<form>")]
async fn upload(form: Form<Upload<'_>>) -> Vec<u8> {
    let mut bytes = form.note.as_bytes().to_vec();
    bytes.push(b'|');

    // A file part that declared no `Content-Type` defaults to binary.
    let content_type = form.file.content_type().map(|ct| ct.to_string());
    bytes.extend(content_type.unwrap_or_else(|| "none".into()).as_bytes());
    bytes.push(b'|');

    // `open()` reads buffered and on-disk uploads alike.
    let mut stream = form.file.open().await.unwrap();
    stream.read_to_end(&mut bytes).await.unwrap();
    bytes
}

/// A file part with a `filename` but no `Content-Type` must be delivered
/// byte-for-byte, not lossily decoded as UTF-8. See issue #2934.
#[test]
fn file_part_without_content_type_preserves_bytes() {
    use rkt::local::blocking::Client;

    let payload: Vec<u8> = (0u8..=255).collect();

    let mut body = Vec::new();
    body.extend(b"--X-BOUNDARY\r\n");
    body.extend(b"Content-Disposition: form-data; name=\"note\"\r\n\r\n");
    body.extend(b"hello\r\n");
    body.extend(b"--X-BOUNDARY\r\n");
    body.extend(b"Content-Disposition: form-data; name=\"file\"; filename=\"blob.bin\"\r\n\r\n");
    body.extend(&payload);
    body.extend(b"\r\n--X-BOUNDARY--\r\n");

    let client = Client::debug_with(routes![upload]).unwrap();
    let response = client
        .post("/")
        .header(
            "multipart/form-data; boundary=X-BOUNDARY"
                .parse::<ContentType>()
                .unwrap(),
        )
        .body(body)
        .dispatch();

    let mut expected = b"hello|application/octet-stream|".to_vec();
    expected.extend(&payload);
    assert_eq!(response.into_bytes().unwrap(), expected);
}

#[rkt::post("/bytes", data = "<form>")]
fn bytes(form: Form<&[u8]>) -> Vec<u8> {
    form.to_vec()
}

fn assert_untyped_part_preserves_bytes(payload: &[u8]) {
    use rkt::http::Status;
    use rkt::local::blocking::Client;

    let mut body =
        b"--X-BOUNDARY\r\nContent-Disposition: form-data; name=\"value\"\r\n\r\n".to_vec();
    body.extend_from_slice(payload);
    body.extend_from_slice(b"\r\n--X-BOUNDARY--\r\n");

    let client = Client::debug_with(routes![bytes]).unwrap();
    let response = client
        .post("/bytes")
        .header(
            "multipart/form-data; boundary=X-BOUNDARY"
                .parse::<ContentType>()
                .unwrap(),
        )
        .body(body)
        .dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert_eq!(response.into_bytes().unwrap(), payload);
}

#[test]
fn filename_less_part_preserves_binary_bytes() {
    let payload: Vec<u8> = (0u8..=255).collect();
    assert_untyped_part_preserves_bytes(&payload);
}

#[test]
fn filename_less_part_preserves_utf8_bom() {
    assert_untyped_part_preserves_bytes("\u{feff}héllo".as_bytes());
}

#[test]
fn filename_less_part_preserves_empty_value() {
    assert_untyped_part_preserves_bytes(b"");
}
