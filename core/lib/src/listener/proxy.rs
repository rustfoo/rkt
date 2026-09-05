//! PROXY protocol (v1 and v2) listener.
//!
//! The [PROXY protocol] allows a TCP proxy or load balancer (HAProxy, AWS
//! NLB, Fly.io, and others) to convey the original client address to a
//! backend server by prefixing each proxied connection with a small preamble.
//! Because the preamble is sent on the raw transport before any application
//! bytes, it works below HTTP and TLS where headers like `X-Forwarded-For`
//! are unavailable.
//!
//! # Configuration
//!
//! Reads the following configuration parameters:
//!
//! | parameter        | type    | default | note                                 |
//! |------------------|---------|---------|--------------------------------------|
//! | `proxy_protocol` | boolean | `false` | require a PROXY protocol preamble    |
//!
//! With the `proxy-proto` crate feature enabled, setting `proxy_protocol` to
//! `true` instructs the [`DefaultListener`](crate::listener::DefaultListener)
//! to require a PROXY protocol v1 or v2 preamble on every incoming
//! connection. The forwarded address becomes the connection's peer
//! [`Endpoint`], so [`Request::remote()`] and [`Request::client_ip()`] report
//! the original client address.
//!
//! [`Request::remote()`]: crate::Request::remote()
//! [`Request::client_ip()`]: crate::Request::client_ip()
//!
//! # Security
//!
//! Only enable `proxy_protocol` when *all* connections arrive through a
//! trusted proxy that always sends the preamble. When enabled, a connection
//! that does not begin with a valid preamble is rejected, as the protocol
//! specification requires; otherwise, a client able to bypass the proxy could
//! spoof an arbitrary address. A connection that fails to complete the
//! preamble within a [timeout](ProxyProtocolListener::with_timeout) is also
//! rejected.
//!
//! [PROXY protocol]: https://www.haproxy.org/download/2.9/doc/proxy-protocol.txt

use std::io::{self, Cursor};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use either::{Either, Left, Right};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};

use crate::listener::{Bind, Certificates, Connection, Endpoint, Listener};
use crate::{Ignite, Rocket};

/// A [PROXY protocol] listener over some listener interface `L`.
///
/// Every connection accepted by an enabled `ProxyProtocolListener` must begin
/// with a valid PROXY protocol v1 or v2 preamble; the version is detected
/// automatically. The forwarded source address is exposed as the connection's
/// peer [`Endpoint`]. Connections without a valid preamble are rejected.
///
/// When composed with TLS, the PROXY preamble is sent before the TLS
/// handshake, so this listener must wrap the transport listener and be
/// wrapped by [`TlsListener`](crate::tls::TlsListener):
/// `TlsListener<ProxyProtocolListener<TcpListener>>`.
///
/// [PROXY protocol]: https://www.haproxy.org/download/2.9/doc/proxy-protocol.txt
pub struct ProxyProtocolListener<L> {
    listener: L,
    enabled: bool,
    timeout: Duration,
}

/// A connection returned by a [`ProxyProtocolListener`].
///
/// Reads and writes are forwarded to the underlying connection `C`. The
/// preamble has already been consumed; the address it conveyed, if any, is
/// returned by [`Connection::endpoint()`] and
/// [`ProxyProtocolStream::forwarded_endpoint()`].
pub struct ProxyProtocolStream<C> {
    inner: C,
    remote: Option<Endpoint>,
    buffer: Option<Cursor<Vec<u8>>>,
}

/// Default deadline for receiving a complete preamble.
const DEFAULT_PREAMBLE_TIMEOUT: Duration = Duration::from_secs(5);

impl<L> ProxyProtocolListener<L> {
    /// Wraps `listener`, requiring a PROXY protocol preamble on every
    /// connection.
    pub fn new(listener: L) -> Self {
        Self {
            listener,
            enabled: true,
            timeout: DEFAULT_PREAMBLE_TIMEOUT,
        }
    }

    /// Wraps `listener` without enabling PROXY protocol handling: connections
    /// pass through unmodified. Used when the `proxy-proto` feature is
    /// compiled in but `proxy_protocol` is not configured.
    pub fn passthrough(listener: L) -> Self {
        Self {
            listener,
            enabled: false,
            timeout: DEFAULT_PREAMBLE_TIMEOUT,
        }
    }

    /// Sets the deadline for receiving a complete preamble. Defaults to 5
    /// seconds. Connections that fail to deliver a full preamble in time are
    /// rejected.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl<L: Bind> Bind for ProxyProtocolListener<L> {
    type Error = Either<figment::Error, L::Error>;

    async fn bind(rocket: &Rocket<Ignite>) -> Result<Self, Self::Error> {
        let enabled = rocket
            .figment()
            .extract_inner::<bool>("proxy_protocol")
            .or_else(|e| if e.missing() { Ok(false) } else { Err(e) })
            .map_err(Left)?;

        let listener = L::bind(rocket).await.map_err(Right)?;
        match enabled {
            true => Ok(Self::new(listener)),
            false => Ok(Self::passthrough(listener)),
        }
    }

    fn bind_endpoint(rocket: &Rocket<Ignite>) -> Result<Endpoint, Self::Error> {
        L::bind_endpoint(rocket).map_err(Right)
    }
}

impl<L: Listener> Listener for ProxyProtocolListener<L> {
    type Accept = L::Accept;

    type Connection = ProxyProtocolStream<L::Connection>;

    async fn accept(&self) -> io::Result<Self::Accept> {
        self.listener.accept().await
    }

    async fn connect(&self, accept: Self::Accept) -> io::Result<Self::Connection> {
        let mut conn = self.listener.connect(accept).await?;
        if !self.enabled {
            return Ok(ProxyProtocolStream {
                inner: conn,
                remote: None,
                buffer: None,
            });
        }

        let (remote, buffer) = tokio::time::timeout(self.timeout, read_preamble(&mut conn))
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "PROXY protocol: timed out waiting for preamble",
                )
            })??;

        Ok(ProxyProtocolStream {
            inner: conn,
            remote,
            buffer,
        })
    }

    fn endpoint(&self) -> io::Result<Endpoint> {
        self.listener.endpoint()
    }
}

impl<C> ProxyProtocolStream<C> {
    /// The client endpoint forwarded in the preamble, if the preamble
    /// conveyed one. `None` for pass-through connections, `LOCAL` (v2)
    /// connections, and `UNKNOWN` (v1) connections.
    pub fn forwarded_endpoint(&self) -> Option<&Endpoint> {
        self.remote.as_ref()
    }
}

impl<C: Connection> Connection for ProxyProtocolStream<C> {
    fn endpoint(&self) -> io::Result<Endpoint> {
        match self.remote.clone() {
            Some(endpoint) => Ok(endpoint),
            None => self.inner.endpoint(),
        }
    }

    fn certificates(&self) -> Option<Certificates<'_>> {
        self.inner.certificates()
    }

    fn server_name(&self) -> Option<&str> {
        self.inner.server_name()
    }
}

impl<C: AsyncRead + Unpin> AsyncRead for ProxyProtocolStream<C> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        // Replay any bytes read past the end of the preamble first.
        if let Some(cursor) = this.buffer.as_mut() {
            if buf.remaining() == 0 {
                return Poll::Ready(Ok(()));
            }

            let pos = cursor.position() as usize;
            let bytes = &cursor.get_ref()[pos..];
            let n = bytes.len().min(buf.remaining());
            buf.put_slice(&bytes[..n]);
            match pos + n == cursor.get_ref().len() {
                true => this.buffer = None,
                false => cursor.set_position((pos + n) as u64),
            }

            return Poll::Ready(Ok(()));
        }

        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl<C: AsyncWrite + Unpin> AsyncWrite for ProxyProtocolStream<C> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

/// Reads a complete preamble from `conn`, returning the forwarded endpoint,
/// if any, and any bytes read past the end of the preamble.
async fn read_preamble<C: AsyncRead + Unpin>(
    conn: &mut C,
) -> io::Result<(Option<Endpoint>, Option<Cursor<Vec<u8>>>)> {
    let mut buf = Vec::with_capacity(256);
    loop {
        if let Preamble::Complete { remote, consumed } = parse_preamble(&buf)? {
            let endpoint = match remote {
                Remote::Inet(addr) => Some(Endpoint::Tcp(addr)),
                Remote::Unix(path) => Some(Endpoint::Unix(path)),
                Remote::Unknown => None,
            };

            buf.drain(..consumed);
            let leftover = (!buf.is_empty()).then(|| Cursor::new(buf));
            return Ok((endpoint, leftover));
        }

        if conn.read_buf(&mut buf).await? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "PROXY protocol: connection closed before preamble was complete",
            ));
        }
    }
}

/// The source address conveyed by a preamble.
#[derive(Debug, Clone, PartialEq)]
enum Remote {
    /// TCP/UDP over IPv4 or IPv6.
    Inet(SocketAddr),
    /// A Unix socket address (v2 only).
    Unix(PathBuf),
    /// No usable address: v1 `UNKNOWN`, v2 `LOCAL`, or v2 `AF_UNSPEC`.
    Unknown,
}

#[derive(Debug, PartialEq)]
enum Preamble {
    Complete { remote: Remote, consumed: usize },
    Incomplete,
}

const V1_PREFIX: &[u8] = b"PROXY ";

/// A v1 preamble, including the CRLF, is at most 107 bytes.
const V1_MAX_LEN: usize = 107;

const V2_SIGNATURE: &[u8] = &[
    0x0D, 0x0A, 0x0D, 0x0A, 0x00, 0x0D, 0x0A, 0x51, 0x55, 0x49, 0x54, 0x0A,
];

fn invalid(msg: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("PROXY protocol: {msg}"))
}

/// Incrementally parse a preamble from the front of `buf`. Returns
/// `Incomplete` if `buf` is a valid prefix of a preamble but more bytes are
/// required, and an error as soon as `buf` cannot begin with a valid v1 or v2
/// preamble.
fn parse_preamble(buf: &[u8]) -> io::Result<Preamble> {
    let sig_len = buf.len().min(V2_SIGNATURE.len());
    if buf[..sig_len] == V2_SIGNATURE[..sig_len] {
        return parse_v2(buf);
    }

    let v1_len = buf.len().min(V1_PREFIX.len());
    if buf[..v1_len] == V1_PREFIX[..v1_len] {
        return parse_v1(buf);
    }

    Err(invalid(
        "connection does not begin with a v1 or v2 preamble",
    ))
}

/// Parse a v1 (human-readable) preamble. The caller has verified that `buf`
/// begins with as much of `PROXY ` as is present.
fn parse_v1(buf: &[u8]) -> io::Result<Preamble> {
    let window = &buf[..buf.len().min(V1_MAX_LEN)];
    let Some(end) = window.windows(2).position(|w| w == b"\r\n") else {
        return match buf.len() < V1_MAX_LEN {
            true => Ok(Preamble::Incomplete),
            false => Err(invalid("v1 preamble missing CRLF within 107 bytes")),
        };
    };

    let consumed = end + 2;
    let line = std::str::from_utf8(&window[..end])
        .map_err(|_| invalid("v1 preamble contains invalid UTF-8"))?;

    let mut tokens = line.split(' ');
    if tokens.next() != Some("PROXY") {
        return Err(invalid("v1 preamble must begin with `PROXY `"));
    }

    let family = match tokens.next() {
        Some(family @ ("TCP4" | "TCP6")) => family,
        // Anything else up to the CRLF must be ignored for `UNKNOWN`.
        Some("UNKNOWN") => {
            return Ok(Preamble::Complete {
                remote: Remote::Unknown,
                consumed,
            })
        }
        _ => return Err(invalid("v1 family must be TCP4, TCP6, or UNKNOWN")),
    };

    let missing = || invalid("v1 preamble is missing address fields");
    let src_addr = tokens.next().ok_or_else(missing)?;
    let dst_addr = tokens.next().ok_or_else(missing)?;
    let src_port = tokens.next().ok_or_else(missing)?;
    let dst_port = tokens.next().ok_or_else(missing)?;
    if tokens.next().is_some() {
        return Err(invalid("v1 preamble contains trailing fields"));
    }

    let parse_addr = |addr: &str| -> io::Result<IpAddr> {
        match family {
            "TCP4" => addr
                .parse::<Ipv4Addr>()
                .map(IpAddr::from)
                .map_err(|_| invalid("v1 TCP4 address is malformed")),
            _ => addr
                .parse::<Ipv6Addr>()
                .map(IpAddr::from)
                .map_err(|_| invalid("v1 TCP6 address is malformed")),
        }
    };

    let src_ip = parse_addr(src_addr)?;
    let _ = parse_addr(dst_addr)?;
    let src_port = parse_v1_port(src_port)?;
    let _ = parse_v1_port(dst_port)?;

    Ok(Preamble::Complete {
        remote: Remote::Inet(SocketAddr::new(src_ip, src_port)),
        consumed,
    })
}

/// Ports must be decimal, in range, and without leading zeroes.
fn parse_v1_port(port: &str) -> io::Result<u16> {
    let malformed = || invalid("v1 port is malformed");
    if port.is_empty() || (port.len() > 1 && port.starts_with('0')) {
        return Err(malformed());
    }

    if !port.bytes().all(|b| b.is_ascii_digit()) {
        return Err(malformed());
    }

    port.parse().map_err(|_| malformed())
}

/// v2 command: the low nibble of byte 12.
const V2_CMD_LOCAL: u8 = 0x0;

/// v2 address family: the high nibble of byte 13.
const V2_AF_UNSPEC: u8 = 0x0;
const V2_AF_INET: u8 = 0x1;
const V2_AF_INET6: u8 = 0x2;
const V2_AF_UNIX: u8 = 0x3;

/// Parse a v2 (binary) preamble. The caller has verified that `buf` begins
/// with as much of the v2 signature as is present.
fn parse_v2(buf: &[u8]) -> io::Result<Preamble> {
    // 12-byte signature + version/command + family/protocol + 2-byte length.
    if buf.len() < 16 {
        return Ok(Preamble::Incomplete);
    }

    let (ver_cmd, fam_proto) = (buf[12], buf[13]);
    if ver_cmd >> 4 != 0x2 {
        return Err(invalid("v2 version nibble is not 2"));
    }

    let cmd = ver_cmd & 0x0F;
    if cmd > 0x1 {
        return Err(invalid("v2 command must be LOCAL or PROXY"));
    }

    let (family, protocol) = (fam_proto >> 4, fam_proto & 0x0F);
    if family > V2_AF_UNIX || protocol > 0x2 {
        return Err(invalid("v2 address family or protocol is invalid"));
    }

    let len = u16::from_be_bytes([buf[14], buf[15]]) as usize;
    let consumed = 16 + len;
    if buf.len() < consumed {
        return Ok(Preamble::Incomplete);
    }

    let complete = |remote| Ok(Preamble::Complete { remote, consumed });

    // For LOCAL, the receiver must use the real connection endpoints; for
    // AF_UNSPEC, the sender provided no usable address. Either way, any
    // address block present must be skipped.
    if cmd == V2_CMD_LOCAL || family == V2_AF_UNSPEC {
        return complete(Remote::Unknown);
    }

    // Address block layout: src addr, dst addr, then for inet families,
    // src port, dst port. Anything beyond it (within `len`) is TLVs.
    let addr = &buf[16..consumed];
    match family {
        V2_AF_INET => {
            if addr.len() < 12 {
                return Err(invalid("v2 length is too small for AF_INET"));
            }

            let ip = Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]);
            let port = u16::from_be_bytes([addr[8], addr[9]]);
            complete(Remote::Inet(SocketAddr::new(ip.into(), port)))
        }
        V2_AF_INET6 => {
            if addr.len() < 36 {
                return Err(invalid("v2 length is too small for AF_INET6"));
            }

            let mut octets = [0u8; 16];
            octets.copy_from_slice(&addr[..16]);
            let port = u16::from_be_bytes([addr[32], addr[33]]);
            complete(Remote::Inet(SocketAddr::new(
                Ipv6Addr::from(octets).into(),
                port,
            )))
        }
        V2_AF_UNIX => {
            if addr.len() < 216 {
                return Err(invalid("v2 length is too small for AF_UNIX"));
            }

            let raw = &addr[..108];
            let path = raw.iter().position(|&b| b == 0).map_or(raw, |i| &raw[..i]);
            let path = std::str::from_utf8(path)
                .map_err(|_| invalid("v2 AF_UNIX address is not UTF-8"))?;
            complete(Remote::Unix(PathBuf::from(path)))
        }
        _ => unreachable!("family validated above"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete(remote: Remote, consumed: usize) -> Preamble {
        Preamble::Complete { remote, consumed }
    }

    fn inet(addr: &str) -> Remote {
        Remote::Inet(addr.parse().unwrap())
    }

    #[track_caller]
    fn assert_invalid(buf: &[u8]) {
        let error = parse_preamble(buf).expect_err("expected parse error");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[track_caller]
    fn assert_incomplete(buf: &[u8]) {
        assert_eq!(parse_preamble(buf).unwrap(), Preamble::Incomplete);
    }

    #[test]
    fn v1_tcp4() {
        let buf = b"PROXY TCP4 192.168.0.1 192.168.0.11 56324 443\r\n";
        let preamble = parse_preamble(buf).unwrap();
        assert_eq!(preamble, complete(inet("192.168.0.1:56324"), buf.len()));
    }

    #[test]
    fn v1_tcp4_with_leftover() {
        let mut buf = b"PROXY TCP4 1.2.3.4 5.6.7.8 65535 80\r\n".to_vec();
        let preamble_len = buf.len();
        buf.extend_from_slice(b"GET / HTTP/1.1\r\n");
        let preamble = parse_preamble(&buf).unwrap();
        assert_eq!(preamble, complete(inet("1.2.3.4:65535"), preamble_len));
    }

    #[test]
    fn v1_tcp6() {
        let buf = b"PROXY TCP6 2001:db8::1 ::1 4124 443\r\n";
        let preamble = parse_preamble(buf).unwrap();
        assert_eq!(preamble, complete(inet("[2001:db8::1]:4124"), buf.len()));
    }

    #[test]
    fn v1_unknown() {
        let buf = b"PROXY UNKNOWN\r\n";
        let preamble = parse_preamble(buf).unwrap();
        assert_eq!(preamble, complete(Remote::Unknown, buf.len()));

        // anything between `UNKNOWN` and CRLF must be ignored
        let buf = b"PROXY UNKNOWN ffff::1 ffff::2 4124 443\r\nextra";
        let preamble = parse_preamble(buf).unwrap();
        assert_eq!(preamble, complete(Remote::Unknown, buf.len() - 5));
    }

    #[test]
    fn v1_incomplete() {
        assert_incomplete(b"");
        assert_incomplete(b"P");
        assert_incomplete(b"PROXY ");
        assert_incomplete(b"PROXY TCP4 1.2.3.4");
        assert_incomplete(b"PROXY TCP4 1.2.3.4 5.6.7.8 123 456");
        // CR alone: the LF may still arrive
        assert_incomplete(b"PROXY TCP4 1.2.3.4 5.6.7.8 123 456\r");
    }

    #[test]
    fn v1_wrong_prefix_rejected_early() {
        assert_invalid(b"GET / HTTP/1.1\r\n");
        assert_invalid(b"G");
        assert_invalid(b"PROXZ");
        assert_invalid(b"proxy TCP4 1.2.3.4 5.6.7.8 123 456\r\n");
        assert_invalid(b"\x16\x03\x01"); // TLS ClientHello
    }

    #[test]
    fn v1_missing_crlf_at_limit() {
        let mut buf = b"PROXY TCP4 1.2.3.4 5.6.7.8 123 456".to_vec();
        buf.extend_from_slice(&[b' '; 107]);
        assert_invalid(&buf);

        // LF without CR does not terminate the preamble
        let mut buf = b"PROXY TCP4 1.2.3.4 5.6.7.8 123 456\n".to_vec();
        buf.extend_from_slice(&[b' '; 107]);
        assert_invalid(&buf);
    }

    #[test]
    fn v1_bad_family() {
        assert_invalid(b"PROXY TCP5 1.2.3.4 5.6.7.8 123 456\r\n");
        assert_invalid(b"PROXY UDP4 1.2.3.4 5.6.7.8 123 456\r\n");
        assert_invalid(b"PROXY tcp4 1.2.3.4 5.6.7.8 123 456\r\n");
        assert_invalid(b"PROXY \r\n");
    }

    #[test]
    fn v1_bad_address() {
        // family and address must agree
        assert_invalid(b"PROXY TCP4 ::1 ::2 123 456\r\n");
        assert_invalid(b"PROXY TCP6 1.2.3.4 5.6.7.8 123 456\r\n");
        // destination is validated too
        assert_invalid(b"PROXY TCP4 1.2.3.4 example.com 123 456\r\n");
        assert_invalid(b"PROXY TCP4 1.2.3.256 5.6.7.8 123 456\r\n");
    }

    #[test]
    fn v1_bad_port() {
        assert_invalid(b"PROXY TCP4 1.2.3.4 5.6.7.8 65536 456\r\n");
        assert_invalid(b"PROXY TCP4 1.2.3.4 5.6.7.8 123 -1\r\n");
        assert_invalid(b"PROXY TCP4 1.2.3.4 5.6.7.8 0123 456\r\n");
        assert_invalid(b"PROXY TCP4 1.2.3.4 5.6.7.8 12a 456\r\n");
        assert_invalid(b"PROXY TCP4 1.2.3.4 5.6.7.8 123 \r\n");
    }

    #[test]
    fn v1_wrong_field_count() {
        assert_invalid(b"PROXY TCP4 1.2.3.4 5.6.7.8 123\r\n");
        assert_invalid(b"PROXY TCP4 1.2.3.4 5.6.7.8 123 456 789\r\n");
        // double space produces an empty field
        assert_invalid(b"PROXY TCP4  1.2.3.4 5.6.7.8 123 456\r\n");
    }

    #[test]
    fn v1_port_zero_is_valid() {
        let buf = b"PROXY TCP4 1.2.3.4 5.6.7.8 0 0\r\n";
        let preamble = parse_preamble(buf).unwrap();
        assert_eq!(preamble, complete(inet("1.2.3.4:0"), buf.len()));
    }

    fn v2_header(cmd: u8, fam_proto: u8, addr: &[u8]) -> Vec<u8> {
        let mut buf = V2_SIGNATURE.to_vec();
        buf.push(0x20 | cmd);
        buf.push(fam_proto);
        buf.extend_from_slice(&(addr.len() as u16).to_be_bytes());
        buf.extend_from_slice(addr);
        buf
    }

    fn v2_inet_addr(src: [u8; 4], dst: [u8; 4], sport: u16, dport: u16) -> Vec<u8> {
        let mut addr = vec![];
        addr.extend_from_slice(&src);
        addr.extend_from_slice(&dst);
        addr.extend_from_slice(&sport.to_be_bytes());
        addr.extend_from_slice(&dport.to_be_bytes());
        addr
    }

    #[test]
    fn v2_inet() {
        let addr = v2_inet_addr([1, 2, 3, 4], [5, 6, 7, 8], 56324, 443);
        let buf = v2_header(0x1, 0x11, &addr);
        let preamble = parse_preamble(&buf).unwrap();
        assert_eq!(preamble, complete(inet("1.2.3.4:56324"), buf.len()));
    }

    #[test]
    fn v2_inet_with_leftover() {
        let addr = v2_inet_addr([1, 2, 3, 4], [5, 6, 7, 8], 56324, 443);
        let mut buf = v2_header(0x1, 0x11, &addr);
        let preamble_len = buf.len();
        buf.extend_from_slice(b"GET / HTTP/1.1\r\n");
        let preamble = parse_preamble(&buf).unwrap();
        assert_eq!(preamble, complete(inet("1.2.3.4:56324"), preamble_len));
    }

    #[test]
    fn v2_inet6() {
        let src = "2001:db8::1".parse::<Ipv6Addr>().unwrap();
        let mut addr = src.octets().to_vec();
        addr.extend_from_slice(&Ipv6Addr::LOCALHOST.octets());
        addr.extend_from_slice(&4124u16.to_be_bytes());
        addr.extend_from_slice(&443u16.to_be_bytes());

        let buf = v2_header(0x1, 0x21, &addr);
        let preamble = parse_preamble(&buf).unwrap();
        assert_eq!(preamble, complete(inet("[2001:db8::1]:4124"), buf.len()));
    }

    #[test]
    fn v2_unix() {
        let mut addr = [0u8; 216];
        addr[..15].copy_from_slice(b"/tmp/proxy.sock");
        let buf = v2_header(0x1, 0x31, &addr);
        let preamble = parse_preamble(&buf).unwrap();
        let remote = Remote::Unix(PathBuf::from("/tmp/proxy.sock"));
        assert_eq!(preamble, complete(remote, buf.len()));
    }

    #[test]
    fn v2_local_health_check() {
        // e.g., an AWS NLB health check: LOCAL with no address block
        let buf = v2_header(0x0, 0x00, &[]);
        let preamble = parse_preamble(&buf).unwrap();
        assert_eq!(preamble, complete(Remote::Unknown, buf.len()));

        // LOCAL with an address block: the block must be skipped
        let addr = v2_inet_addr([1, 2, 3, 4], [5, 6, 7, 8], 56324, 443);
        let buf = v2_header(0x0, 0x11, &addr);
        let preamble = parse_preamble(&buf).unwrap();
        assert_eq!(preamble, complete(Remote::Unknown, buf.len()));
    }

    #[test]
    fn v2_unspec_family() {
        let buf = v2_header(0x1, 0x00, &[0u8; 8]);
        let preamble = parse_preamble(&buf).unwrap();
        assert_eq!(preamble, complete(Remote::Unknown, buf.len()));
    }

    #[test]
    fn v2_tlvs_are_skipped() {
        let mut addr = v2_inet_addr([1, 2, 3, 4], [5, 6, 7, 8], 56324, 443);
        addr.extend_from_slice(&[0xEA, 0x00, 0x02, 0xAB, 0xCD]); // a TLV
        let buf = v2_header(0x1, 0x11, &addr);
        let preamble = parse_preamble(&buf).unwrap();
        assert_eq!(preamble, complete(inet("1.2.3.4:56324"), buf.len()));
    }

    #[test]
    fn v2_incomplete() {
        let addr = v2_inet_addr([1, 2, 3, 4], [5, 6, 7, 8], 56324, 443);
        let buf = v2_header(0x1, 0x11, &addr);
        for len in 0..buf.len() {
            assert_incomplete(&buf[..len]);
        }
    }

    #[test]
    fn v2_bad_signature() {
        let mut buf = V2_SIGNATURE.to_vec();
        buf[11] = 0x0B;
        assert_invalid(&buf);
    }

    #[test]
    fn v2_bad_version() {
        let addr = v2_inet_addr([1, 2, 3, 4], [5, 6, 7, 8], 1, 2);
        let mut buf = v2_header(0x1, 0x11, &addr);
        buf[12] = 0x11; // version 1 in a v2 preamble
        assert_invalid(&buf);
    }

    #[test]
    fn v2_bad_command() {
        let addr = v2_inet_addr([1, 2, 3, 4], [5, 6, 7, 8], 1, 2);
        let buf = v2_header(0x2, 0x11, &addr);
        assert_invalid(&buf);
    }

    #[test]
    fn v2_bad_family_or_protocol() {
        let addr = v2_inet_addr([1, 2, 3, 4], [5, 6, 7, 8], 1, 2);
        assert_invalid(&v2_header(0x1, 0x41, &addr));
        assert_invalid(&v2_header(0x1, 0x13, &addr));
    }

    #[test]
    fn v2_header_validated_before_length() {
        // an invalid version is rejected even before `len` bytes arrive
        let addr = v2_inet_addr([1, 2, 3, 4], [5, 6, 7, 8], 1, 2);
        let mut buf = v2_header(0x1, 0x11, &addr);
        buf[12] = 0x31;
        assert_invalid(&buf[..16]);
    }

    #[test]
    fn v2_address_block_too_small() {
        assert_invalid(&v2_header(0x1, 0x11, &[0u8; 8]));
        assert_invalid(&v2_header(0x1, 0x21, &[0u8; 12]));
        assert_invalid(&v2_header(0x1, 0x31, &[0u8; 215]));
    }

    async fn read_from(input: &[u8]) -> io::Result<(Option<Endpoint>, Vec<u8>)> {
        let (mut client, mut server) = tokio::io::duplex(64);
        let input = input.to_vec();
        let writer = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            // write in small chunks to exercise incremental parsing
            for chunk in input.chunks(7) {
                client.write_all(chunk).await.unwrap();
                client.flush().await.unwrap();
            }
        });

        let (remote, buffer) = read_preamble(&mut server).await?;
        writer.await.unwrap();

        let mut stream = ProxyProtocolStream {
            inner: server,
            remote: remote.clone(),
            buffer,
        };
        let mut rest = Vec::new();
        stream.read_to_end(&mut rest).await?;
        Ok((remote, rest))
    }

    #[tokio::test]
    async fn preamble_read_with_replay() {
        let payload = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let mut input = b"PROXY TCP4 1.2.3.4 5.6.7.8 56324 443\r\n".to_vec();
        input.extend_from_slice(payload);

        let (remote, rest) = read_from(&input).await.unwrap();
        assert_eq!(
            remote,
            Some(Endpoint::Tcp("1.2.3.4:56324".parse().unwrap()))
        );
        assert_eq!(rest, payload);
    }

    #[tokio::test]
    async fn preamble_read_replay_in_small_chunks() {
        let payload = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let addr = v2_inet_addr([9, 8, 7, 6], [5, 6, 7, 8], 1234, 443);
        let mut input = v2_header(0x1, 0x11, &addr);
        input.extend_from_slice(payload);

        let (mut client, mut server) = tokio::io::duplex(1024);
        tokio::io::AsyncWriteExt::write_all(&mut client, &input)
            .await
            .unwrap();
        drop(client);

        let (remote, buffer) = read_preamble(&mut server).await.unwrap();
        assert_eq!(remote, Some(Endpoint::Tcp("9.8.7.6:1234".parse().unwrap())));

        // drain replayed + live bytes through 3-byte reads
        let mut stream = ProxyProtocolStream {
            inner: server,
            remote,
            buffer,
        };
        let mut rest = Vec::new();
        let mut chunk = [0u8; 3];
        loop {
            match stream.read(&mut chunk).await.unwrap() {
                0 => break,
                n => rest.extend_from_slice(&chunk[..n]),
            }
        }

        assert_eq!(rest, payload);
    }

    #[tokio::test]
    async fn preamble_eof_is_an_error() {
        for input in [&b""[..], b"PROXY TCP4 1.2.3.4", &V2_SIGNATURE[..4]] {
            let error = read_from(input).await.unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        }
    }

    #[tokio::test]
    async fn preamble_garbage_is_an_error() {
        let error = read_from(b"GET / HTTP/1.1\r\n\r\n").await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn stream_write_passthrough() {
        let (mut client, server) = tokio::io::duplex(64);
        let mut stream = ProxyProtocolStream {
            inner: server,
            remote: None,
            buffer: None,
        };

        use tokio::io::AsyncWriteExt;
        stream.write_all(b"hello").await.unwrap();
        stream.flush().await.unwrap();
        drop(stream);

        let mut received = Vec::new();
        client.read_to_end(&mut received).await.unwrap();
        assert_eq!(received, b"hello");
    }
}
