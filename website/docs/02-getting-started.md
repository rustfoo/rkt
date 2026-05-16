---
sidebar_position: 4
sidebar_label: "Getting Started"
---

# Getting Started

Let's create and run our first rkt application. We'll ensure we have a
compatible Rust toolchain installed, create a new Cargo project that depends on
rkt, and then run the application.

## Installing Rust

rkt makes use of the latest Rust features. Because of this, we'll need a
recent release of Rust to run rkt applications. If you already have a working
installation of the latest Rust compiler, feel free to skip to the next section.

To install the latest version of Rust, we recommend using `rustup`. Install
`rustup` by following the instructions on [its website](https://rustup.rs/).
Once `rustup` is installed, ensure the latest toolchain is installed by running
the command:

```sh
rustup default stable
```

## Hello, world!

Let's write our first rkt application! Start by creating a new binary-based
Cargo project and changing into the new directory:

```sh
cargo new hello-rkt --bin
cd hello-rkt
```

Now, add rkt as a dependency in your `Cargo.toml`:

```toml
[dependencies]
rkt = "1.0.0"
```

Modify `src/main.rs` so that it contains the code for the rkt `Hello, world!`
program, reproduced below:

```rust
#[macro_use] extern crate rkt;

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[launch]
fn rocket() -> _ {
    rkt::build().mount("/", routes![index])
}
```

This example creates an `index` route, _mounts_ the route at the `/` path,
and _launches_ the application.

Compile and run the program with `cargo run`. You should see the following:

```sh
> cargo run
🔧 Configured for debug.
   >> address: 127.0.0.1
   >> port: 8000
   >> workers: [..]
   >> keep-alive: 5s
   >> limits: [..]
   >> tls: disabled
   >> temp dir: /tmp
   >> log level: normal
   >> cli colors: true
🛰  Routes:
   >> (index) GET /
🚀 Rocket has launched from http://127.0.0.1:8000
```

Visit `http://localhost:8000` to see your first rkt application in action!

:::tip[Don't like colors or emoji?]

You can disable colors and emoji by setting the `ROCKET_CLI_COLORS`
environment variable to `0` or `false` when running a Rocket binary:
`ROCKET_CLI_COLORS=false cargo run`.
:::
