# `rkt_ws` (deprecated)

WebSocket support now lives in [`rkt::ws`](https://docs.rs/rkt/latest/rkt/ws/).

```toml
[dependencies]
rkt = { version = "1.3.0", features = ["ws"] }
```

Replace `rkt_ws` or `ws` imports with `rkt::ws`:

```rust
use rkt::ws::{WebSocket, Stream};
```

`rkt_ws` remains a source-compatible compatibility shim for one full minor
release cycle and will be removed in the following minor release. Its legacy
`tungstenite` feature remains a no-op compatibility alias.
