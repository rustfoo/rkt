#![cfg(feature = "http3-preview")]

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use bytes::{BufMut, Bytes};
use quinn::crypto::rustls::QuicClientConfig;
use rkt::data::{Data, ToByteUnit};
use rkt::fairing::AdHoc;
use rkt::figment::Figment;
use rkt::fs::relative;
use rkt::listener::Endpoint;
use rkt::tls::{ClientHello, Resolver, ServerConfig, TlsConfig};
use rkt::tokio::{
    self,
    sync::{oneshot, Notify},
};
use rkt::{Config, Shutdown, State};
use rustls::pki_types::{pem::PemObject, CertificateDer, ServerName};

#[rkt::post("/echo/<stop>", data = "<data>")]
async fn echo(
    stop: bool,
    data: Data<'_>,
    shutdown: Shutdown,
    respond: &State<Arc<Notify>>,
) -> Vec<u8> {
    let body = data.open(1.mebibytes()).into_bytes().await.unwrap();
    assert!(body.is_complete());
    if stop {
        shutdown.notify();
        respond.notified().await;
    }

    body.into_inner()
}

#[rkt::async_test]
async fn http3_echo_and_graceful_shutdown() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let (ready, listening) = oneshot::channel();
        let respond = Arc::new(Notify::new());
        let mut config = Config::debug_default();
        config.shutdown.ctrlc = false;
        config.shutdown.signals.clear();
        config.shutdown.grace = 5;
        config.shutdown.mercy = 1;
        let tls = TlsConfig::from_paths(
            relative!("../../examples/tls/private/rsa_sha256_cert.pem"),
            relative!("../../examples/tls/private/rsa_sha256_key.pem"),
        );
        let figment = Figment::from(config)
            .merge(("address", "127.0.0.1:0"))
            .merge(("tls", tls));
        let rocket = rkt::custom(figment)
            .mount("/", rkt::routes![echo])
            .manage(respond.clone())
            .attach(AdHoc::on_liftoff("Report HTTP/3 endpoint", |rocket| {
                Box::pin(async move {
                    let addr = rocket.endpoints().find_map(|e| e.quic()).unwrap();
                    ready.send((addr, rocket.shutdown())).unwrap();
                })
            }));
        let server = tokio::spawn(rocket.launch());
        let (addr, shutdown) = match listening.await {
            Ok(endpoint) => endpoint,
            Err(_) => panic!("HTTP/3 server exited before liftoff: {:?}", server.await),
        };

        let client = quic_client(None);
        let connection = client.connect(addr, "localhost").unwrap().await.unwrap();
        let (mut driver, mut sender) = h3::client::new(h3_quinn::Connection::new(connection))
            .await
            .unwrap();
        let driver = tokio::spawn(async move { driver.wait_idle().await });

        let payload = Bytes::from((0..=255).cycle().take(256 * 1024).collect::<Vec<u8>>());
        for stop in [false, true] {
            let request =
                http::Request::post(format!("https://localhost:{}/echo/{stop}", addr.port()))
                    .body(())
                    .unwrap();
            let mut stream = sender.send_request(request).await.unwrap();
            stream.send_data(payload.clone()).await.unwrap();
            stream.finish().await.unwrap();
            if stop {
                // Keep the handler in flight until shutdown has been requested.
                shutdown.clone().await;
                respond.notify_one();
            }

            let response = stream.recv_response().await.unwrap();
            assert_eq!(response.status(), http::StatusCode::OK);
            let mut echoed = Vec::new();
            while let Some(chunk) = stream.recv_data().await.unwrap() {
                echoed.put(chunk);
            }
            assert_eq!(echoed, payload);
        }

        drop(sender);
        server.await.unwrap().unwrap();
        driver.await.unwrap();
    })
    .await
    .expect("HTTP/3 exchange or shutdown timed out");
}

fn quic_client(identity: Option<&[u8]>) -> quinn::Endpoint {
    let mut tls = client_tls(identity);
    tls.alpn_protocols = vec![b"h3".to_vec()];

    let mut client = quinn::Endpoint::client((Ipv4Addr::LOCALHOST, 0).into()).unwrap();
    client.set_default_client_config(quinn::ClientConfig::new(Arc::new(
        QuicClientConfig::try_from(tls).unwrap(),
    )));
    client
}

fn client_tls(identity: Option<&[u8]>) -> rustls::ClientConfig {
    use rustls::pki_types::PrivateKeyDer;

    let ca = include_bytes!("../../../examples/tls/private/ca_cert.pem");
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from_pem_slice(ca).unwrap())
        .unwrap();

    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots);

    match identity {
        Some(pem) => {
            let chain = CertificateDer::pem_slice_iter(pem)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            let key = PrivateKeyDer::from_pem_slice(pem).unwrap();
            config.with_client_auth_cert(chain, key).unwrap()
        }
        None => config.with_no_client_auth(),
    }
}

#[cfg(feature = "mtls")]
#[rkt::get("/certificate")]
fn certificate(cert: rkt::mtls::Certificate<'_>) -> Vec<u8> {
    cert.as_bytes().to_vec()
}

#[cfg(feature = "mtls")]
#[rkt::async_test]
async fn http3_requires_client_certificate() {
    use rkt::mtls::MtlsConfig;

    tokio::time::timeout(Duration::from_secs(15), async {
        let (ready, listening) = oneshot::channel();
        let mut config = Config::debug_default();
        config.shutdown.ctrlc = false;
        config.shutdown.signals.clear();
        let tls = TlsConfig::from_paths(
            relative!("../../examples/tls/private/rsa_sha256_cert.pem"),
            relative!("../../examples/tls/private/rsa_sha256_key.pem"),
        )
        .with_mutual(
            MtlsConfig::from_path(relative!("../../examples/tls/private/ca_cert.pem"))
                .mandatory(true),
        );

        let rocket = rkt::custom(
            Figment::from(config)
                .merge(("address", "127.0.0.1:0"))
                .merge(("tls", tls)),
        )
        .mount("/", rkt::routes![certificate])
        .attach(AdHoc::on_liftoff("Report HTTP/3 endpoint", |rocket| {
            Box::pin(async move {
                let addr = rocket.endpoints().find_map(|e| e.quic()).unwrap();
                ready.send((addr, rocket.shutdown())).unwrap();
            })
        }));

        let server = tokio::spawn(rocket.launch());
        let (addr, shutdown) = match listening.await {
            Ok(endpoint) => endpoint,
            Err(_) => panic!("HTTP/3 server exited before liftoff: {:?}", server.await),
        };

        let client = quic_client(None);
        let rejection = match client.connect(addr, "localhost").unwrap().await {
            Ok(connection) => connection.closed().await,
            Err(error) => error,
        };
        // TLS alert 116 is certificate_required.
        let certificate_required = quinn::TransportErrorCode::crypto(116);
        assert!(
            matches!(rejection, quinn::ConnectionError::ConnectionClosed(ref close)
            if close.error_code == certificate_required),
            "{rejection:?}"
        );

        let identity = include_bytes!("../../../examples/tls/private/client.pem");
        let expected = CertificateDer::from_pem_slice(identity).unwrap();
        let client = quic_client(Some(identity));
        let connection = client.connect(addr, "localhost").unwrap().await.unwrap();
        let (mut driver, mut sender) = h3::client::new(h3_quinn::Connection::new(connection))
            .await
            .unwrap();
        let driver = tokio::spawn(async move { driver.wait_idle().await });

        let request = http::Request::get(format!("https://localhost:{}/certificate", addr.port()))
            .body(())
            .unwrap();
        let mut stream = sender.send_request(request).await.unwrap();
        stream.finish().await.unwrap();
        assert_eq!(
            stream.recv_response().await.unwrap().status(),
            http::StatusCode::OK
        );
        let mut certificate = Vec::new();
        while let Some(chunk) = stream.recv_data().await.unwrap() {
            certificate.put(chunk);
        }
        assert_eq!(certificate, expected.as_ref());

        drop(sender);
        shutdown.notify();
        server.await.unwrap().unwrap();
        driver.await.unwrap();
    })
    .await
    .expect("HTTP/3 mutual TLS exchange or shutdown timed out");
}

#[rkt::get("/")]
fn hello() -> &'static str {
    "Hello, world!"
}

type Server = tokio::task::JoinHandle<Result<rkt::Rocket<rkt::Ignite>, rkt::Error>>;

/// A TLS resolver that always returns the configured TLS configuration.
struct StaticResolver(Arc<ServerConfig>);

#[rkt::async_trait]
impl Resolver for StaticResolver {
    async fn init(rocket: &rkt::Rocket<rkt::Build>) -> rkt::tls::Result<Self> {
        let config: TlsConfig = rocket.figment().extract_inner("tls")?;
        Ok(StaticResolver(Arc::new(config.server_config().await?)))
    }

    async fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<ServerConfig>> {
        Some(self.0.clone())
    }
}

async fn launch_hello(grace: u32) -> (SocketAddr, Shutdown, Server) {
    let (endpoints, shutdown, server) = launch_hello_with(grace, false).await;
    let addr = endpoints.iter().find_map(|e| e.quic()).unwrap();
    (addr, shutdown, server)
}

/// Launches `hello` over TLS, with a `StaticResolver` attached if `resolver`,
/// and returns the endpoints reported at liftoff.
async fn launch_hello_with(grace: u32, resolver: bool) -> (Vec<Endpoint>, Shutdown, Server) {
    let (ready, listening) = oneshot::channel();
    let mut config = Config::debug_default();
    config.shutdown.ctrlc = false;
    config.shutdown.signals.clear();
    config.shutdown.grace = grace;
    config.shutdown.mercy = 1;
    let tls = TlsConfig::from_paths(
        relative!("../../examples/tls/private/rsa_sha256_cert.pem"),
        relative!("../../examples/tls/private/rsa_sha256_key.pem"),
    );
    let figment = Figment::from(config)
        .merge(("address", "127.0.0.1:0"))
        .merge(("tls", tls));
    let mut rocket =
        rkt::custom(figment)
            .mount("/", rkt::routes![hello])
            .attach(AdHoc::on_liftoff("Report endpoints", |rocket| {
                Box::pin(async move {
                    let endpoints = rocket.endpoints().cloned().collect();
                    ready.send((endpoints, rocket.shutdown())).unwrap();
                })
            }));
    if resolver {
        rocket = rocket.attach(StaticResolver::fairing());
    }

    let server = tokio::spawn(rocket.launch());
    match listening.await {
        Ok((endpoints, shutdown)) => (endpoints, shutdown, server),
        Err(_) => panic!("server exited before liftoff: {:?}", server.await),
    }
}

/// Sends `GET /` over HTTP/1.1 + TLS and returns the response head.
async fn https_get_head(addr: SocketAddr) -> String {
    use rkt::tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut tls = client_tls(None);
    tls.alpn_protocols = vec![b"http/1.1".to_vec()];
    let connector = tokio_rustls::TlsConnector::from(Arc::new(tls));
    let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
    let name = ServerName::try_from("localhost").unwrap();
    let mut stream = connector.connect(name, tcp).await.unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();

    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        head.push(stream.read_u8().await.unwrap());
    }

    String::from_utf8(head).unwrap()
}

/// Returns the value of the header `name` in the response head `head`.
fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// An HTTP/3 HEADERS frame for `GET https://localhost/`, encoded using only
/// QPACK's static table.
fn get_headers_frame() -> Vec<u8> {
    // Required insert count and base are 0. Then indexed static fields
    // `:method: GET` (17), `:scheme: https` (23), and `:path: /` (1), and
    // `:authority` (0) with the literal value `localhost`.
    let mut fields = vec![0x00, 0x00, 0xC0 | 17, 0xC0 | 23, 0xC0 | 1, 0x50, 9];
    fields.extend_from_slice(b"localhost");
    let mut frame = vec![0x01, fields.len() as u8];
    frame.extend(fields);
    frame
}

/// Reads a raw HTTP/3 response stream and checks that it begins with HEADERS.
async fn assert_raw_response(mut rx: quinn::RecvStream) {
    let response = rx
        .read_to_end(64 * 1024)
        .await
        .expect("response stream failed");
    assert_eq!(response.first(), Some(&0x01), "no HEADERS: {response:?}");
}

#[rkt::async_test]
async fn http3_stream_error_keeps_connection_open() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let (addr, shutdown, server) = launch_hello(1).await;
        let client = quic_client(None);
        let connection = client.connect(addr, "localhost").unwrap().await.unwrap();
        let (mut driver, mut sender) =
            h3::client::new(h3_quinn::Connection::new(connection.clone()))
                .await
                .unwrap();
        let driver = tokio::spawn(async move { driver.wait_idle().await });

        // A request stream that ends without headers (`H3_REQUEST_INCOMPLETE`)...
        let (mut tx, _rx) = connection.open_bi().await.unwrap();
        tx.write_all(&[]).await.unwrap();
        tx.finish().unwrap();

        // ...and one that the client cancels before sending all of its headers.
        let (mut tx, _rx) = connection.open_bi().await.unwrap();
        tx.write_all(&get_headers_frame()[..1]).await.unwrap();
        tx.reset(
            h3::error::Code::H3_REQUEST_CANCELLED
                .value()
                .try_into()
                .unwrap(),
        )
        .unwrap();

        // Neither should affect other requests on the same connection. The
        // server accepts streams in order, so this request's stream is handled
        // after the two above.
        let request = http::Request::get(format!("https://localhost:{}/", addr.port()))
            .body(())
            .unwrap();
        let mut stream = sender.send_request(request).await.unwrap();
        stream.finish().await.unwrap();
        let response = stream.recv_response().await.unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
        let mut body = Vec::new();
        while let Some(chunk) = stream.recv_data().await.unwrap() {
            body.put(chunk);
        }
        assert_eq!(body, b"Hello, world!");
        assert!(
            connection.close_reason().is_none(),
            "{:?}",
            connection.close_reason()
        );

        drop(sender);
        shutdown.notify();
        server.await.unwrap().unwrap();
        driver.await.unwrap();
    })
    .await
    .expect("HTTP/3 exchange or shutdown timed out");
}

#[rkt::async_test]
async fn http3_serves_requests_accepted_before_shutdown() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let (addr, shutdown, server) = launch_hello(5).await;
        let client = quic_client(None);
        let connection = client.connect(addr, "localhost").unwrap().await.unwrap();
        let headers = get_headers_frame();

        // Start a request but don't finish sending its headers yet.
        let (mut pending_tx, pending_rx) = connection.open_bi().await.unwrap();
        pending_tx.write_all(&headers[..1]).await.unwrap();

        // The server accepts streams in order, so once this later request is
        // answered, the pending request has been accepted too.
        let (mut tx, rx) = connection.open_bi().await.unwrap();
        tx.write_all(&headers).await.unwrap();
        tx.finish().unwrap();
        assert_raw_response(rx).await;

        // Begin shutting down, then finish sending the pending request's
        // headers. It was accepted before GOAWAY, so it must be served.
        shutdown.notify();
        shutdown.clone().await;
        tokio::time::sleep(Duration::from_millis(250)).await;
        pending_tx.write_all(&headers[1..]).await.unwrap();
        pending_tx.finish().unwrap();
        assert_raw_response(pending_rx).await;

        // Close the connection so the server needn't wait out the grace period.
        connection.close(0u32.into(), b"done");
        server.await.unwrap().unwrap();
    })
    .await
    .expect("HTTP/3 exchange or shutdown timed out");
}

#[rkt::async_test]
async fn http3_disabled_when_tls_resolver_attached() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let (endpoints, shutdown, server) = launch_hello_with(1, true).await;
        assert!(endpoints.iter().all(|e| !e.is_quic()), "{endpoints:?}");

        let addr = endpoints.iter().find_map(|e| e.tcp()).unwrap();
        let head = https_get_head(addr).await;
        assert!(head.starts_with("HTTP/1.1 200 OK\r\n"), "{head}");
        assert_eq!(header(&head, "Alt-Svc"), None, "{head}");

        shutdown.notify();
        server.await.unwrap().unwrap();
    })
    .await
    .expect("HTTPS exchange or shutdown timed out");
}

#[rkt::async_test]
async fn http3_advertised_without_tls_resolver() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let (endpoints, shutdown, server) = launch_hello_with(1, false).await;
        let quic = endpoints.iter().find_map(|e| e.quic()).unwrap();

        let addr = endpoints.iter().find_map(|e| e.tcp()).unwrap();
        let head = https_get_head(addr).await;
        assert!(head.starts_with("HTTP/1.1 200 OK\r\n"), "{head}");
        let alt_svc = format!("h3=\":{}\"", quic.port());
        assert_eq!(header(&head, "Alt-Svc"), Some(&*alt_svc), "{head}");

        shutdown.notify();
        server.await.unwrap().unwrap();
    })
    .await
    .expect("HTTPS exchange or shutdown timed out");
}
