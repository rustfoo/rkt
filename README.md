# rkt

[![Build Status](https://github.com/rustfoo/rkt/workflows/CI/badge.svg)](https://github.com/rustfoo/rkt/actions)
[![Current Crates.io Version](https://img.shields.io/crates/v/rkt.svg)](https://crates.io/crates/rkt)
[![Minimum Rust Version](https://img.shields.io/badge/rustc-1.82.0+-orange.svg)](https://www.rust-lang.org/)

**rkt** is an async web framework for Rust with a focus on usability, security, extensibility, and speed.

```rust
#[macro_use] extern crate rkt;

#[get("/<name>/<age>")]
fn hello(name: &str, age: u8) -> String {
    format!("Hello, {} year old named {}!", age, name)
}

#[launch]
fn rocket() -> _ {
    rkt::build().mount("/hello", routes![hello])
}
```

Visiting `localhost:8000/hello/John/58` will trigger the `hello` route, returning
`Hello, 58 year old named John!`. If `<age>` can't be parsed as a `u8`, the route
won't be called and a 404 is returned automatically.

> **rkt** is a community-maintained continuation of [Rocket](https://github.com/rwf2/Rocket),
> the Rust web framework originally created by [Sergio Benitez](https://github.com/SergioBenitez)
> and the Rocket contributors. We are deeply grateful for their foundational work —
> rkt would not exist without it. The original project remains available at
> [github.com/rwf2/Rocket](https://github.com/rwf2/Rocket).

## Getting Started

Add `rkt` to your `Cargo.toml`:

```toml
[dependencies]
rkt = "1.0.0"
```

## Crates

| Crate | Description | Docs |
|-------|-------------|------|
| `rkt` | Core framework | [![docs.rs](https://img.shields.io/docsrs/rkt)](https://docs.rs/rkt) |
| `rkt_dyn_templates` | Dynamic template rendering (Tera, Handlebars, MiniJinja) | [![docs.rs](https://img.shields.io/docsrs/rkt_dyn_templates)](https://docs.rs/rkt_dyn_templates) |
| `rkt_ws` | WebSocket support | [![docs.rs](https://img.shields.io/docsrs/rkt_ws)](https://docs.rs/rkt_ws) |

## Features

- **HTTP/1.1 & HTTP/2** — built on Hyper
- **HTTP/3 preview** — via s2n-quic (enable with `http3-preview`)
- **TLS & mTLS** — via Rustls (enable with `tls` / `mtls`)
- **Secret cookies** — signed and encrypted cookie support (enable with `secrets`)
- **WebSockets** — first-class support via `rkt_ws`
- **Dynamic templates** — Tera, Handlebars, and MiniJinja via `rkt_dyn_templates`
- **Tracing** — structured logging via the `tracing` ecosystem
- **Type-safe routing** — compile-time checked routes, guards, and responders
- **Extensible** — fairings, request guards, and custom responders

## Documentation

- [Guide](https://rkt.rs/guide/) — detailed reference covering all features
- [API Docs](https://docs.rs/rkt) — full rustdoc reference
- [Examples](examples#readme) — runnable example projects in this repo

## Examples

Each subdirectory under [`examples/`](examples#readme) is a self-contained Cargo crate.
To run one:

```sh
cd examples/hello
cargo run
```

## Getting Help

- Open a [GitHub Discussion](https://github.com/rustfoo/rkt/discussions) for questions
- File a [bug report or feature request](https://github.com/rustfoo/rkt/issues)

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) before
submitting a pull request. You can also help by:

1. Reporting bugs or requesting features via [issues](https://github.com/rustfoo/rkt/issues)
2. Improving documentation
3. Sharing projects built with rkt in [Show & Tell](https://github.com/rustfoo/rkt/discussions)

## Acknowledgements

rkt is built on the shoulders of the [Rocket](https://github.com/rwf2/Rocket) project.
Sincere thanks to [Sergio Benitez](https://github.com/SergioBenitez) for creating
Rocket and to all past and present Rocket contributors for the work that makes
this framework possible.

## License

rkt is dual-licensed under your choice of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

Any contribution you submit will be dual-licensed under the same terms, unless
you explicitly state otherwise.
