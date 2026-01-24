//! Redirect all HTTP requests to HTTPs.

use std::net::SocketAddr;

use rkt::{Rocket, Ignite, Orbit, State, Error};
use rkt::http::uri::{Origin, Host};
use rkt::tracing::Instrument;
use rkt::fairing::{Fairing, Info, Kind};
use rkt::response::Redirect;
use rkt::listener::tcp::TcpListener;
use rkt::trace::Trace;

#[derive(Debug, Clone, Copy, Default)]
pub struct Redirector(u16);

#[derive(Debug, Clone)]
pub struct Config {
    server: rkt::Config,
    tls_addr: SocketAddr,
}

#[route("/<_..>")]
fn redirect(config: &State<Config>, uri: &Origin<'_>, host: &Host<'_>) -> Redirect {
    // FIXME: Check the host against a whitelist!
    let domain = host.domain();
    let https_uri = match config.tls_addr.port() {
        443 => format!("https://{domain}{uri}"),
        port => format!("https://{domain}:{port}{uri}"),
    };

    Redirect::permanent(https_uri)
}

impl Redirector {
    pub fn on(port: u16) -> Self {
        Redirector(port)
    }

    // Launch an instance of Rocket than handles redirection on `self.port`.
    pub async fn try_launch(self, config: Config) -> Result<Rocket<Ignite>, Error> {
        rkt::span_info!("HTTP -> HTTPS Redirector" => {
            info!(from = self.0, to = config.tls_addr.port(),  "redirecting");
        });

        let addr = SocketAddr::new(config.tls_addr.ip(), self.0);
        rkt::custom(&config.server)
            .manage(config)
            .mount("/", routes![redirect])
            .try_launch_on(TcpListener::bind(addr))
            .await
    }
}

#[rkt::async_trait]
impl Fairing for Redirector {
    fn info(&self) -> Info {
        Info {
            name: "HTTP -> HTTPS Redirector",
            kind: Kind::Liftoff | Kind::Singleton
        }
    }

    #[tracing::instrument(name = "HTTP -> HTTPS Redirector", skip_all)]
    async fn on_liftoff(&self, rocket: &Rocket<Orbit>) {
        let Some(tls_addr) = rocket.endpoints().find_map(|e| e.tls()?.tcp()) else {
            warn!("Main instance is not being served over TLS/TCP.\n\
                Redirector refusing to start.");

            return;
        };

        let this = *self;
        let shutdown = rocket.shutdown();
        let span = tracing::info_span!("HTTP -> HTTPS Redirector");
        let config = Config { tls_addr, server: rocket.config().clone() };
        rkt::tokio::spawn(async move {
            if let Err(e) = this.try_launch(config).await {
                e.trace_error();
                info!("shutting down main instance");
                shutdown.notify();
            }
        }.instrument(span));
    }
}
