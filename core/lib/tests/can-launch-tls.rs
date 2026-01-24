#![cfg(feature = "tls")]

extern crate rkt;

use rkt::figment::providers::Serialized;
use rkt::fs::relative;
use rkt::local::asynchronous::Client;
use rkt::tls::{CipherSuite, TlsConfig};

#[rkt::async_test]
async fn can_launch_tls() {
    let cert_path = relative!("examples/tls/private/rsa_sha256_cert.pem");
    let key_path = relative!("examples/tls/private/rsa_sha256_key.pem");

    let tls = TlsConfig::from_paths(cert_path, key_path).with_ciphers([
        CipherSuite::TLS_AES_128_GCM_SHA256,
        CipherSuite::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256,
    ]);

    let config = rkt::Config::figment().merge(Serialized::defaults(tls));
    let client = Client::debug(rkt::custom(config)).await.unwrap();
    client.rocket().shutdown().notify();
    client.rocket().shutdown().await;
}
