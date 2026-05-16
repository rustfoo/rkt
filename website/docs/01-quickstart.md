---
sidebar_position: 3
sidebar_label: "Quickstart"
---

# Quickstart

Before you can start writing a rkt application, you'll need to install the
Rust toolchain. We recommend using [rustup](https://rustup.rs/). If you don't
have Rust installed and would like extra guidance doing so, see [Getting
Started].

## Running Examples

The absolute fastest way to start experimenting with rkt is to clone the
[rkt repository](https://github.com/rustfoo/rkt) and run the included
examples in the `examples/` directory. For instance, the following set
of commands runs the `hello` example:

```sh
git clone https://github.com/rustfoo/rkt
cd rkt
git checkout main
cd examples/hello
cargo run
```

There are numerous examples in the `examples/` directory. They can all be run
with `cargo run`.

:::note

The examples' `Cargo.toml` files will point to the locally cloned `rkt`
libraries. When copying the examples for your own use, you should modify the
`Cargo.toml` files as explained in the [Getting Started](./getting-started)
guide.
:::
