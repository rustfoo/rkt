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
