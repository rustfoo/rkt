//! Deprecated compatibility shim for [`rkt::ws`].
//!
//! New applications should enable rkt's `ws` feature and import WebSocket
//! types from `rkt::ws`. Replace `rkt_ws` (or a `ws` dependency alias) imports
//! with `rkt::ws`. This crate will be removed after one full minor release
//! cycle.

#![deprecated(note = "rkt_ws is deprecated; enable rkt's `ws` feature and use rkt::ws instead")]
#![doc(html_root_url = "https://docs.rs/rkt_ws/latest/rkt_ws")]

#[deprecated(since = "1.3.0", note = "use `rkt::ws::WebSocket`")]
pub type WebSocket = rkt::ws::WebSocket;

#[deprecated(since = "1.3.0", note = "use `rkt::ws::Channel`")]
pub type Channel<'r> = rkt::ws::Channel<'r>;

#[deprecated(since = "1.3.0", note = "use `rkt::ws::Config`")]
pub type Config = rkt::ws::Config;

#[deprecated(since = "1.3.0", note = "use `rkt::ws::Message`")]
pub type Message = rkt::ws::Message;

/// Types representing incoming and/or outgoing `async` [`Message`] streams.
pub mod stream {
    #[deprecated(since = "1.3.0", note = "use `rkt::ws::stream::DuplexStream`")]
    pub type DuplexStream = rkt::ws::stream::DuplexStream;

    #[deprecated(since = "1.3.0", note = "use `rkt::ws::stream::MessageStream`")]
    pub type MessageStream<'r, S> = rkt::ws::stream::MessageStream<'r, S>;
}

/// Structures for constructing raw WebSocket frames.
pub mod frame {
    #[deprecated(since = "1.3.0", note = "use `rkt::ws::frame::CloseCode`")]
    pub type CloseCode = rkt::ws::frame::CloseCode;

    #[deprecated(since = "1.3.0", note = "use `rkt::ws::frame::CloseFrame`")]
    pub type CloseFrame = rkt::ws::frame::CloseFrame;

    #[deprecated(since = "1.3.0", note = "use `rkt::ws::frame::Frame`")]
    pub type Frame = rkt::ws::frame::Frame;

    #[doc(hidden)]
    pub use rkt::ws::frame::Message;
}

/// Library [`Error`](result::Error) and [`Result`](result::Result) types.
pub mod result {
    #[deprecated(since = "1.3.0", note = "use `rkt::ws::result::Error`")]
    pub type Error = rkt::ws::result::Error;

    #[deprecated(since = "1.3.0", note = "use `rkt::ws::result::Result`")]
    pub type Result<T, E = rkt::ws::result::Error> = rkt::ws::result::Result<T, E>;
}

// A `macro_rules!` re-export cannot carry a deprecation: the macro expands via
// `$crate` to `rkt::ws`, bypassing this crate entirely. Users reach it through
// the `WebSocket` guard, which warns above.
pub use rkt::ws::Stream;
