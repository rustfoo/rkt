use std::io;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use futures::{Future, TryFutureExt};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto::Builder;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::data::{IoStream, RawStream};
use crate::erased::{ErasedIoHandler, ErasedRequest, ErasedResponse};
use crate::error::log_server_error;
use crate::http::Status;
use crate::listener::{BouncedExt, CancellableExt, Connection, Listener};
use crate::request::ConnectionMeta;
use crate::trace::{Trace, TraceAll};
use crate::util::{spawn_inspect, FutureExt, ReaderStream};
use crate::{Ignite, Orbit, Request, Rocket};

type Result<T, E = crate::Error> = std::result::Result<T, E>;

impl Rocket<Orbit> {
    #[tracing::instrument("request", skip_all, fields(
        method = %parts.method,
        uri = %parts.uri,
        autohandled
    ))]
    async fn service<T: for<'a> Into<RawStream<'a>>>(
        self: Arc<Self>,
        parts: http::request::Parts,
        stream: T,
        upgrade: Option<hyper::upgrade::OnUpgrade>,
        connection: ConnectionMeta,
    ) -> Result<hyper::Response<ReaderStream<ErasedResponse>>, http::Error> {
        connection.trace_debug();

        // `span_debug!` runs its closure even when the subscriber discards
        // `DEBUG` events, so also check that the level is enabled to avoid
        // materializing and formatting headers that will never be logged.
        let debug_headers = self.config.debug_headers && tracing::enabled!(tracing::Level::DEBUG);
        let request = ErasedRequest::new(self, parts, |rocket, parts| {
            Request::from_hyp(rocket, parts, connection).unwrap_or_else(|e| e)
        });

        if debug_headers {
            span_debug!("request headers" => request.inner().headers().iter().trace_all_debug());
        }

        let mut response = request
            .into_response(
                stream,
                |rocket, request, data| Box::pin(rocket.preprocess(request, data)),
                |token, rocket, request, data| {
                    Box::pin(async move {
                        if !request.errors.is_empty() {
                            return rocket.dispatch_error(Status::BadRequest, request).await;
                        }

                        rocket.dispatch(token, request, data).await
                    })
                },
            )
            .await;

        // TODO: Should upgrades be handled in dispatch?
        response.inner().trace_info();
        if debug_headers {
            span_debug!("response headers" => response.inner().headers().iter().trace_all_debug());
        }

        let io_handler = response.make_io_handler(Rocket::extract_io_handler);
        if let (Some((proto, handler)), Some(upgrade)) = (io_handler, upgrade) {
            let upgrade = upgrade.map_ok(IoStream::from).map_err(io::Error::other);
            tokio::task::spawn(io_handler_task(proto, upgrade, handler));
        }

        let mut builder = hyper::Response::builder();
        builder = builder.status(response.inner().status().code);
        for header in response.inner().headers().iter() {
            builder = builder.header(header.name().as_str(), header.value());
        }

        let chunk_size = response.inner().body().max_chunk_size();
        builder.body(ReaderStream::with_capacity(response, chunk_size))
    }

    pub(crate) fn alt_svc(&self) -> Option<&str> {
        self.alt_svc.as_deref()
    }
}

#[tracing::instrument("upgrade", skip_all, fields(protocol = proto))]
async fn io_handler_task<S>(proto: String, stream: S, mut handler: ErasedIoHandler)
where
    S: Future<Output = io::Result<IoStream>>,
{
    let stream = match stream.await {
        Ok(stream) => stream,
        Err(e) => return warn!(error = %e, "i/o upgrade failed"),
    };

    debug!("i/o upgrade succeeded");
    if let Err(e) = handler.take().io(stream).await {
        match e.kind() {
            io::ErrorKind::BrokenPipe => warn!("i/o handler closed"),
            _ => warn!(error = %e, "i/o handler terminated unsuccessfully"),
        }
    }
}

impl Rocket<Ignite> {
    pub(crate) async fn listen_and_serve<L, R>(
        self,
        listener: L,
        orbit_callback: impl FnOnce(Rocket<Orbit>) -> R,
    ) -> Result<Arc<Rocket<Orbit>>>
    where
        L: Listener + 'static,
        R: Future<Output = Result<Arc<Rocket<Orbit>>>>,
    {
        let endpoint = listener.endpoint()?;

        #[cfg(feature = "http3-preview")]
        match (endpoint.tcp(), endpoint.tls_config()) {
            // QUIC can't consult a `Resolver`: rather than serve HTTP/3 with
            // the default TLS configuration, don't serve HTTP/3 at all.
            (Some(_), Some(tls)) if tls.resolver.is_some() => warn!(
                "HTTP/3 is disabled: a TLS resolver is attached, and resolvers \
                are not yet supported over QUIC.\n\
                Serving HTTP/1 + HTTP/2 only."
            ),
            (Some(addr), Some(tls)) => {
                use crate::error::ErrorKind;

                let quic = crate::listener::quic::bind(addr, tls)
                    .map_err(|e| ErrorKind::Bind(Some(endpoint.clone()), Box::new(e)))
                    .await?;

                let quic_endpoint =
                    crate::listener::Endpoint::Quic(quic.local_addr()?).with_tls(tls);
                let rocket = self.into_orbit(vec![quic_endpoint, endpoint]);
                let rocket = orbit_callback(rocket).await?;

                let http12 = tokio::task::spawn(rocket.clone().serve12(listener));
                let http3 = tokio::task::spawn(rocket.clone().serve3(quic));
                let (r1, r2) = futures::join!(http12, http3);
                r1.map_err(|e| ErrorKind::Liftoff(Err(rocket.clone()), e))??;
                r2.map_err(|e| ErrorKind::Liftoff(Err(rocket.clone()), e))??;
                return Ok(rocket);
            }
            _ => warn!(
                "HTTP/3 cannot start without a valid TCP + TLS configuration.\n\
                Falling back to HTTP/1 + HTTP/2 server."
            ),
        }

        let rocket = self.into_orbit(vec![endpoint]);
        let rocket = orbit_callback(rocket).await?;
        rocket.clone().serve12(listener).await?;
        Ok(rocket)
    }
}

impl Rocket<Orbit> {
    pub(crate) async fn serve12<L>(self: Arc<Self>, listener: L) -> Result<()>
    where
        L: Listener + 'static,
        L::Connection: AsyncRead + AsyncWrite,
    {
        let mut builder = Builder::new(TokioExecutor::new());
        let keep_alive = Duration::from_secs(self.config.keep_alive.into());
        builder
            .http1()
            .half_close(true)
            .timer(TokioTimer::new())
            .keep_alive(keep_alive > Duration::ZERO)
            .preserve_header_case(true)
            .header_read_timeout(Duration::from_secs(15));

        #[cfg(feature = "http2")]
        {
            builder.http2().timer(TokioTimer::new());
            if keep_alive > Duration::ZERO {
                builder
                    .http2()
                    .timer(TokioTimer::new())
                    .keep_alive_interval(keep_alive / 4)
                    .keep_alive_timeout(keep_alive);
            }
        }

        let (listener, server) = (Arc::new(listener.bounced()), Arc::new(builder));
        while let Some(accept) = listener
            .accept()
            .race(self.shutdown())
            .await
            .left()
            .transpose()?
        {
            let (listener, rocket, server) = (listener.clone(), self.clone(), server.clone());
            spawn_inspect(|e| log_server_error(&**e), async move {
                let conn = listener.connect(accept).race_io(rocket.shutdown()).await?;
                let meta =
                    ConnectionMeta::new(conn.endpoint(), conn.certificates(), conn.server_name());
                let service = service_fn(|mut req| {
                    let upgrade = hyper::upgrade::on(&mut req);
                    let (parts, incoming) = req.into_parts();
                    rocket
                        .clone()
                        .service(parts, incoming, Some(upgrade), meta.clone())
                });

                let io = TokioIo::new(conn.cancellable(rocket.shutdown.clone()));
                let mut server = pin!(server.serve_connection_with_upgrades(io, service));
                match server.as_mut().race(rocket.shutdown()).await.left() {
                    Some(result) => result,
                    None => {
                        server.as_mut().graceful_shutdown();
                        server.await
                    }
                }
            });
        }

        Ok(())
    }

    #[cfg(feature = "http3-preview")]
    async fn serve3(self: Arc<Self>, endpoint: quinn::Endpoint) -> Result<()> {
        use std::future::IntoFuture;

        use either::Either;

        use crate::listener::{quic, Endpoint};

        while let Some(Some(incoming)) = endpoint.accept().race(self.shutdown()).await.left() {
            let rocket = self.clone();
            spawn_inspect(|e: &io::Error| log_server_error(e), async move {
                let conn = incoming
                    .into_future()
                    .map_err(io::Error::other)
                    .race_io(rocket.shutdown())
                    .await?;
                let remote = Endpoint::Quic(conn.remote_address()).assume_tls();
                let certs = quic::certificates(&conn);
                let sni = quic::server_name(&conn);
                let meta = ConnectionMeta::new(Ok(remote), certs, sni.as_deref());
                let mut h3 = quic::Connection::new(h3_quinn::Connection::new(conn.clone()))
                    .map_err(io::Error::other)
                    .race_io(rocket.shutdown())
                    .await?;

                loop {
                    let resolver = match h3.accept().race(rocket.shutdown()).await {
                        Either::Left(Ok(Some(resolver))) => resolver,
                        Either::Left(Ok(None)) => break,
                        Either::Left(Err(e)) if is_h3_disconnect(&e) => break,
                        Either::Left(Err(e)) => return Err(io::Error::other(e)),
                        Either::Right(_) => {
                            // Send GOAWAY and let in-flight requests finish within grace.
                            //
                            // h3 0.0.8 only forgets a request stream once it
                            // resolves, and has no API to forget one that
                            // failed to resolve. Such a stream keeps `accept()`
                            // from reporting completion, so we may wait for the
                            // client to close the connection or for the grace
                            // period to elapse, but no longer.
                            h3.shutdown(0)
                                .map_err(io::Error::other)
                                .race_io(rocket.shutdown.grace.clone())
                                .await?;
                            let _ = h3.accept().race(rocket.shutdown.grace.clone()).await;
                            break;
                        }
                    };

                    let (rocket, meta) = (rocket.clone(), meta.clone());
                    spawn_inspect(|e: &io::Error| log_server_error(e), async move {
                        // A failed resolution only affects this stream: h3 resets
                        // or drops the stream, and for connection-level errors,
                        // records the error so that the next `h3.accept()` above
                        // closes the connection with the appropriate code.
                        //
                        // Like HTTP/1's header read timeout, bound how long the
                        // client may take to send headers. Race grace, not the
                        // shutdown start, so that requests accepted before
                        // GOAWAY are still served.
                        let (request, stream) = tokio::time::timeout(
                            Duration::from_secs(15),
                            resolver.resolve_request(),
                        )
                        .map_err(|_| {
                            io::Error::new(io::ErrorKind::TimedOut, "request header timeout")
                        })
                        .and_then(|resolved| async { resolved.map_err(io::Error::other) })
                        .race_io(rocket.shutdown.grace.clone())
                        .await?;
                        let (mut tx, rx) = stream.split();
                        let rx = quic::QuicRx(rx).cancellable(rocket.shutdown.clone());
                        let response = rocket
                            .clone()
                            .service(request.into_parts().0, rx, None, meta)
                            .map_err(io::Error::other)
                            .race_io(rocket.shutdown.mercy.clone())
                            .await?;

                        match quic::send_response(&mut tx, response)
                            .race(rocket.shutdown.grace.clone())
                            .await
                            .left()
                        {
                            Some(result) => result,
                            None => Ok(tx.stop_stream(h3::error::Code::H3_REQUEST_CANCELLED)),
                        }
                    });
                }

                Ok(())
            });
        }

        endpoint.set_server_config(None);
        if endpoint
            .wait_idle()
            .race(self.shutdown.mercy.clone())
            .await
            .left()
            .is_none()
        {
            endpoint.close(
                h3::error::Code::H3_NO_ERROR.value().try_into().unwrap(),
                b"shutdown",
            );

            // Give the endpoint a moment to send CONNECTION_CLOSE to peers.
            let _ = tokio::time::timeout(Duration::from_secs(1), endpoint.wait_idle()).await;
        }

        Ok(())
    }
}

/// Returns `true` if `e` is a normal end to an HTTP/3 connection: a graceful
/// close by either side (`H3_NO_ERROR` or code 0), an idle timeout, or a
/// connection that was closed locally.
#[cfg(feature = "http3-preview")]
fn is_h3_disconnect(e: &h3::error::ConnectionError) -> bool {
    use h3::error::ConnectionError;
    use h3::quic::ConnectionErrorIncoming;

    match e {
        _ if e.is_h3_no_error() => true,
        ConnectionError::Timeout { .. } => true,
        ConnectionError::Remote {
            0: ConnectionErrorIncoming::ApplicationClose { error_code: 0 },
            ..
        } => true,
        ConnectionError::Remote {
            0: ConnectionErrorIncoming::Undefined(e),
            ..
        } => match e.downcast_ref::<quinn::ConnectionError>() {
            Some(quinn::ConnectionError::LocallyClosed | quinn::ConnectionError::TimedOut) => true,
            Some(quinn::ConnectionError::ConnectionClosed(close)) => {
                close.error_code == quinn::TransportErrorCode::NO_ERROR
            }
            _ => false,
        },
        _ => false,
    }
}
