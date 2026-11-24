use std::io;

use rkt::{Build, Rocket};
use rustls_pki_types::{pem::PemObject, CertificateDer};

use testbench::{Error, Result};

pub static DEFAULT_CONFIG: &str = r#"
    [default]
    address = "tcp:127.0.0.1"
    workers = 2
    port = 0
    cli_colors = false
    log_level = "debug"
    secret_key = "itlYmFR2vYKrOmFhupMIn/hyB6lYCCTXz4yaQX89XVg="

    [default.shutdown]
    grace = 1
    mercy = 1
"#;

pub static TLS_CONFIG: &str = r#"
    [default.tls]
    certs = "{ROCKET}/examples/tls/private/rsa_sha256_cert.pem"
    key = "{ROCKET}/examples/tls/private/rsa_sha256_key.pem"
"#;

pub trait RocketExt {
    fn default() -> Self;
    fn tls_default() -> Self;
    fn reconfigure_with_toml(self, toml: &str) -> Self;
}

impl RocketExt for Rocket<Build> {
    fn default() -> Self {
        rkt::build().reconfigure_with_toml(DEFAULT_CONFIG)
    }

    fn tls_default() -> Self {
        rkt::build()
            .reconfigure_with_toml(DEFAULT_CONFIG)
            .reconfigure_with_toml(TLS_CONFIG)
    }

    fn reconfigure_with_toml(self, toml: &str) -> Self {
        use rkt::figment::{
            providers::{Format, Toml},
            Figment,
        };

        let toml = toml.replace("{ROCKET}", rkt::fs::relative!("../"));
        let config = Figment::from(self.figment()).merge(Toml::string(&toml).nested());

        self.reconfigure(config)
    }
}

pub fn read(path: &str) -> Result<Vec<u8>> {
    let path = path.replace("{ROCKET}", rkt::fs::relative!("../"));
    Ok(std::fs::read(path)?)
}

pub fn cert(path: &str) -> Result<Vec<u8>> {
    let data = read(path)?;
    let cert = CertificateDer::pem_slice_iter(&data)
        .last()
        .ok_or(Error::MissingCertificate)?
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(cert.to_vec())
}
