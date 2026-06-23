# `ws` [![ci.svg]][ci] [![crates.io]][crate] [![docs.svg]][crate docs]

[crates.io]: https://img.shields.io/crates/v/rkt_ws.svg
[crate]: https://crates.io/crates/rkt_ws
[docs.svg]: https://img.shields.io/badge/web-master-red.svg?style=flat&label=docs&colorB=d33847
[crate docs]: https://docs.rs/rkt_ws/latest/rkt_ws/
[ci.svg]: https://github.com/rustfoo/rkt/workflows/CI/badge.svg
[ci]: https://github.com/rustfoo/rkt/actions

This crate provides WebSocket support for rkt via integration with rkt's
[connection upgrades] API.

# Usage

  1. Depend on `rkt_ws`, renamed here to `ws`:

     ```toml
     [dependencies]
     ws = { package = "rkt_ws", version = "1.0.1" }
     ```

   2. Use it!

      ```rust
      #[get("/echo")]
      fn echo_stream(ws: ws::WebSocket) -> ws::Stream!['static] {
          ws::Stream! { ws =>
              for await message in ws {
                  yield message?;
              }
          }
      }
      ```

See the [crate docs] for full details.
