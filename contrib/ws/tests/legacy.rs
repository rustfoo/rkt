//! Ensures the pre-promotion import path and macro remain source compatible.

// This crate is the deprecated shim; exercising it is the point of this test.
#![allow(deprecated)]

use rkt_ws::{Stream, WebSocket};

#[rkt::get("/")]
fn echo(ws: WebSocket) -> Stream![] {
    Stream! { ws =>
        yield "legacy".into();
    }
}

#[test]
fn legacy_api_is_available() {
    let _ = echo;
}
