# HTTP benchmarks

This suite compares equivalent rkt, Axum, and Actix applications over explicit
HTTP/1.1 loopback connections. It is a diagnostic performance suite, not a
correctness-test replacement.

## Running

Install `oha` 1.14.0 and `jq`, then run:

```sh
./benchmarks/http/bench.sh
```

The diagnostic defaults use a two-second warm-up, five ten-second measured
repetitions, and concurrency levels 1, 32, and 100. Framework order rotates
deterministically for every repetition. Only one server runs at a time. A full
default run takes a little over two hours; use the smoke command printed by
`--help` while changing the harness.

Every invocation creates a new timestamped directory under `results/`. It
contains:

- `metadata.json`: commit and dirty state, tool versions, CPU, kernel,
  governor, affinity, protocol, and run configuration;
- `cargo-metadata.json`: the exact locked dependency graph;
- `raw/`: one augmented `oha` JSON document per measured repetition;
- `logs/`: server/load-generator diagnostics;
- `summary.json` and `report.html`: medians and median absolute deviations.

The harness refuses to overwrite a result directory, builds with `--locked`,
checks every measured run for only successful 200 responses, and validates
server liveness afterward. It also unsets an inherited `NO_COLOR` before
invoking `oha` to avoid the 1.14.0 boolean-environment parsing issue.

For a quick harness smoke test:

```sh
./benchmarks/http/bench.sh -d 1 -w 0 -r 1 -c 1 \
  -s ping,headers-wire,cookies-wire
```

Use `python3 benchmarks/http/report.py RUN_DIRECTORY` to regenerate reports
from raw data.

## Scenario interpretation

- `headers-wire` sends 15 non-cookie headers to the ignored `ping` route.
- `cookies-wire` sends only a five-cookie header to the same ignored route.
- `headers-sparse` asks application code for two header values.
- `headers-full` intentionally iterates the complete header map.
- `query-borrowed` preserves the original comparison: rkt borrows `msg`, while
  Axum and Actix deserialize it into an owned `String`.
- `query-owned` uses owned application values in all three frameworks.
- Memory and file scenarios separately cover 1 KiB, 64 KiB, and 1 MiB bodies.
- `stream-slow` sends 16 one-KiB chunks with a five-millisecond delay before
  each chunk, exposing time-to-first-byte and streaming behavior under load.

These distinctions matter: deltas between scenarios identify likely costs,
but do not by themselves prove which internal operation is responsible.

## Relationship to `cargo bench` and tests

Keep the Criterion suite invoked by `cargo bench --manifest-path
benchmarks/Cargo.toml`. It measures the local lifecycle of route-rich
applications, while this suite measures live network behavior. They answer
different questions and should not replace one another.

Correctness tests should continue to run under `cargo test`. A benchmark may
perform untimed setup validation, but timing assertions are too noisy and
environment-dependent to substitute for deterministic functional tests. The
Criterion also contains a narrow linear route-matching benchmark for first,
middle, last, and missing static/dynamic routes at 10, 100, 1,000, and 10,000
routes. Remove the lifecycle-shaped cases only if their exact coverage is
superseded and historical continuity is no longer useful.
