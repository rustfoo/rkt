#![recursion_limit = "256"]
#![doc(html_root_url = "https://docs.rs/rkt/latest/rkt")]
#![doc(html_favicon_url = "https://rkt.rs/images/favicon.ico")]
#![doc(html_logo_url = "https://rkt.rs/images/logo-boxed.png")]
#![cfg_attr(nightly, feature(doc_cfg))]
#![cfg_attr(nightly, feature(decl_macro))]

//! # rkt - Core API Documentation
//!
//! Hello, and welcome to the core rkt API documentation!
//!
//! This API documentation is highly technical and is purely a reference.
//! There's an [overview] of rkt on the main site as well as a [full,
//! detailed guide]. If you'd like pointers on getting started, see the
//! [quickstart] or [getting started] chapters of the guide.
//!
//! [overview]: https://rkt.rs/overview
//! [full, detailed guide]: https://rkt.rs/guide
//! [quickstart]: https://rkt.rs/guide/quickstart
//! [getting started]: https://rkt.rs/guide/getting-started
//!
//! ## Usage
//!
//! Depend on `rkt` in `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! rkt = { package = version = "1.0.0" }
//! ```
//!
//! See the [guide](https://rkt.rs/guide) for more information on how
//! to write rkt applications. Here's a simple example to get you started:
//!
//! [git dependencies]: https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#specifying-dependencies-from-git-repositories
//!
//! ```rust,no_run
//! # #[macro_use] extern crate rkt;
//!
//! #[get("/")]
//! fn hello() -> &'static str {
//!     "Hello, world!"
//! }
//!
//! #[launch]
//! fn rocket() -> _ {
//!     rkt::build().mount("/", routes![hello])
//! }
//! ```
//!
//! ## Features
//!
//! To avoid compiling unused dependencies, rkt feature-gates optional
//! functionality, some enabled by default:
//!
//! | Feature         | Default? | Description                                             |
//! |-----------------|----------|---------------------------------------------------------|
//! | `trace`         | Yes      | Enables the default rkt tracing [subscriber].        |
//! | `http2`         | Yes      | Support for HTTP/2 (enabled by default).                |
//! | `secrets`       | No       | Support for authenticated, encrypted [private cookies]. |
//! | `tls`           | No       | Support for [TLS] encrypted connections.                |
//! | `mtls`          | No       | Support for verified clients via [mutual TLS].          |
//! | `json`          | No       | Support for [JSON (de)serialization].                   |
//! | `msgpack`       | No       | Support for [MessagePack (de)serialization].            |
//! | `uuid`          | No       | Support for [UUID value parsing and (de)serialization]. |
//! | `tokio-macros`  | No       | Enables the `macros` feature in the exported `tokio`    |
//! | `http3-preview` | No       | Experimental preview support for [HTTP/3].              |
//!
//! Disabled features can be selectively enabled in `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! rkt = { version = "1.0.0", features = ["secrets", "tls", "json"] }
//! ```
//!
//! Conversely, HTTP/2 can be disabled:
//!
//! ```toml
//! [dependencies]
//! rkt = { version = "1.0.0", default-features = false }
//! ```
//!
//! [subscriber]: crate::trace::subscriber
//! [JSON (de)serialization]: crate::serde::json
//! [MessagePack (de)serialization]: crate::serde::msgpack
//! [UUID value parsing and (de)serialization]: crate::serde::uuid
//! [private cookies]: https://rkt.rs/guide/requests/#private-cookies
//! [TLS]: https://rkt.rs/guide/configuration/#tls
//! [mutual TLS]: crate::mtls
//! [HTTP/3]: crate::listener::quic
//!
//! ## Configuration
//!
//! rkt offers a rich, extensible configuration system built on [Figment]. By
//! default, rkt applications are configured via a `Rocket.toml` file
//! and/or `ROCKET_{PARAM}` environment variables, but applications may
//! configure their own sources. See the [configuration guide] for full details.
//!
//! ## Testing
//!
//! The [`local`] module contains structures that facilitate unit and
//! integration testing of a rkt application. The top-level [`local`] module
//! documentation and the [testing guide] include detailed examples.
//!
//! [configuration guide]: https://rkt.rs/guide/configuration/
//! [testing guide]: https://rkt.rs/guide/testing/#testing
//! [Figment]: https://docs.rs/figment

// Allows using rkt's codegen in rkt itself.
extern crate self as rkt;

#[doc(hidden)]
pub use async_stream;
pub use either;
pub use figment;
pub use futures;
pub use time;
pub use tokio;
pub use tracing;
/// These are public dependencies! Update docs if these are changed, especially
/// figment's version number in docs.
#[doc(hidden)]
pub use yansi;

#[macro_use]
pub mod trace;
#[macro_use]
pub mod outcome;
#[macro_use]
pub mod data;
pub mod catcher;
pub mod config;
pub mod error;
pub mod fairing;
pub mod form;
pub mod fs;
pub mod http;
pub mod listener;
pub mod local;
#[cfg(feature = "mtls")]
#[cfg_attr(nightly, doc(cfg(feature = "mtls")))]
pub mod mtls;
pub mod request;
pub mod response;
pub mod route;
#[doc(hidden)]
pub mod sentinel;
pub mod serde;
pub mod shield;
pub mod shutdown;
#[cfg(feature = "tls")]
#[cfg_attr(nightly, doc(cfg(feature = "tls")))]
pub mod tls;

mod erased;
mod lifecycle;
mod phase;
#[path = "rocket.rs"]
mod rocket;
mod router;
mod server;
mod state;
mod util;

#[doc(inline)]
pub use rkt_codegen::*;

#[doc(inline)]
pub use crate::catcher::Catcher;
#[doc(inline)]
pub use crate::config::Config;
#[doc(inline)]
pub use crate::data::Data;
#[doc(inline)]
pub use crate::error::Error;
#[doc(inline)]
pub use crate::phase::{Build, Ignite, Orbit, Phase};
#[doc(inline)]
pub use crate::request::Request;
#[doc(inline)]
pub use crate::response::Response;
#[doc(inline)]
pub use crate::rocket::Rocket;
#[doc(inline)]
pub use crate::route::Route;
#[doc(inline)]
pub use crate::sentinel::{Sentinel, Sentry};
#[doc(inline)]
pub use crate::shutdown::Shutdown;
#[doc(inline)]
pub use crate::state::State;

/// Retrofits support for `async fn` in trait impls and declarations.
///
/// Any trait declaration or trait `impl` decorated with `#[async_trait]` is
/// retrofitted with support for `async fn`s:
///
/// ```rust
/// # use rkt::*;
/// #[async_trait]
/// trait MyAsyncTrait {
///     async fn do_async_work();
/// }
///
/// #[async_trait]
/// impl MyAsyncTrait for () {
///     async fn do_async_work() { /* .. */ }
/// }
/// ```
///
/// All `impl`s for a trait declared with `#[async_trait]` must themselves be
/// decorated with `#[async_trait]`. Many of rkt's traits, such as
/// [`FromRequest`](crate::request::FromRequest) and
/// [`Fairing`](crate::fairing::Fairing) are `async`. As such, implementations
/// of said traits must be decorated with `#[async_trait]`. See the individual
/// trait docs for trait-specific details.
///
/// For more details on `#[async_trait]`, see [`async_trait`](mod@async_trait).
#[doc(inline)]
pub use async_trait::async_trait;

const WORKER_PREFIX: &str = "rocket-worker";

/// Creates a [`Rocket`] instance with the default config provider: aliases
/// [`Rocket::build()`].
pub fn build() -> Rocket<Build> {
    Rocket::build()
}

/// Creates a [`Rocket`] instance with a custom config provider: aliases
/// [`Rocket::custom()`].
pub fn custom<T: figment::Provider>(provider: T) -> Rocket<Build> {
    Rocket::custom(provider)
}

/// WARNING: This is unstable! Do not use this method outside of rkt!
#[doc(hidden)]
pub fn async_run<F, R>(fut: F, workers: usize, sync: usize, force_end: bool, name: &str) -> R
where
    F: std::future::Future<Output = R>,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_name(name)
        .worker_threads(workers)
        .max_blocking_threads(sync)
        .enable_all()
        .build()
        .expect("create tokio runtime");

    let result = runtime.block_on(fut);
    if force_end {
        runtime.shutdown_timeout(std::time::Duration::from_millis(500));
    }

    result
}

/// WARNING: This is unstable! Do not use this method outside of rkt!
#[doc(hidden)]
pub fn async_test<R>(fut: impl std::future::Future<Output = R>) -> R {
    async_run(fut, 1, 32, true, &format!("{WORKER_PREFIX}-test-thread"))
}

/// WARNING: This is unstable! Do not use this method outside of rkt!
#[doc(hidden)]
pub fn async_main<R>(fut: impl std::future::Future<Output = R> + Send) -> R {
    fn bail<T, E: crate::trace::Trace>(e: E) -> T {
        e.trace_error();
        panic!("aborting due to error")
    }

    // FIXME: We need to run `fut` to get the user's `Figment` to properly set
    // up the async env, but we need the async env to run `fut`. So we're stuck.
    // Tokio doesn't let us take the state from one async env and migrate it to
    // another, so we need to use one, making this impossible.
    //
    // So as a result, we only use values from rkt's figment. These
    // values won't reflect swaps of `Rocket` in attach fairings with different
    // config values, or values from non-rkt configs. See tokio-rs/tokio#3329
    // for a necessary resolution in `tokio`.
    let fig = Config::figment();
    let workers = fig.extract_inner(Config::WORKERS).unwrap_or_else(bail);
    let max_blocking = fig.extract_inner(Config::MAX_BLOCKING).unwrap_or_else(bail);
    let force = fig
        .focus(Config::SHUTDOWN)
        .extract_inner("force")
        .unwrap_or_else(bail);
    async_run(
        fut,
        workers,
        max_blocking,
        force,
        &format!("{WORKER_PREFIX}-thread"),
    )
}

/// Executes a `future` to completion on a new tokio-based rkt async runtime.
///
/// The runtime is terminated on shutdown, and the future's resolved value is
/// returned.
///
/// # Considerations
///
/// This function is a low-level mechanism intended to be used to execute the
/// future returned by [`Rocket::launch()`] in a self-contained async runtime
/// designed for rkt. It runs futures in exactly the same manner as
/// [`#[launch]`](crate::launch) and [`#[main]`](crate::main) do and is thus
/// _never_ the preferred mechanism for running a rkt application. _Always_
/// prefer to use the [`#[launch]`](crate::launch) or [`#[main]`](crate::main)
/// attributes. For example [`#[main]`](crate::main) can be used even when
/// rkt is just a small part of a bigger application:
///
/// ```rust,no_run
/// #[rkt::main]
/// async fn main() {
///     # let should_start_server_in_foreground = false;
///     # let should_start_server_in_background = false;
///     let rocket = rkt::build();
///     if should_start_server_in_foreground {
///         rkt::build().launch().await;
///     } else if should_start_server_in_background {
///         rkt::tokio::spawn(rocket.launch());
///     } else {
///         // do something else
///     }
/// }
/// ```
///
/// See [rkt#launching] for more on using these attributes.
///
/// # Example
///
/// Build an instance of rkt, launch it, and wait for shutdown:
///
/// ```rust,no_run
/// use rkt::fairing::AdHoc;
///
/// let rocket = rkt::build()
///     .attach(AdHoc::on_liftoff("Liftoff Printer", |_| Box::pin(async move {
///         println!("Stalling liftoff for a second...");
///         rkt::tokio::time::sleep(std::time::Duration::from_secs(1)).await;
///         println!("And we're off!");
///     })));
///
/// rkt::execute(rocket.launch());
/// ```
///
/// Launch a pre-built instance of rkt and wait for it to shutdown:
///
/// ```rust,no_run
/// use rkt::{Rocket, Ignite, Phase, Error};
///
/// fn launch<P: Phase>(rocket: Rocket<P>) -> Result<Rocket<Ignite>, Error> {
///     rkt::execute(rocket.launch())
/// }
/// ```
///
/// Do async work to build an instance of rkt, launch, and wait for shutdown:
///
/// ```rust,no_run
/// use rkt::fairing::AdHoc;
///
/// // This line can also be inside of the `async` block.
/// let rocket = rkt::build();
///
/// rkt::execute(async move {
///     let rocket = rocket.ignite().await?;
///     let config = rocket.config();
///     rocket.launch().await
/// });
/// ```
pub fn execute<R, F>(future: F) -> R
where
    F: std::future::Future<Output = R> + Send,
{
    async_main(future)
}

/// Returns a future that evaluates to `true` exactly when there is a presently
/// running tokio async runtime that was likely started by rkt.
fn running_within_rocket_async_rt() -> impl std::future::Future<Output = bool> {
    use futures::FutureExt;

    tokio::task::spawn_blocking(|| {
        let this = std::thread::current();
        this.name().is_some_and(|s| s.starts_with(WORKER_PREFIX))
    })
    .map(|r| r.unwrap_or(false))
}
