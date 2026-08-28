# Performance benchmarks

This directory has two complementary suites:

- `cargo bench --manifest-path benchmarks/Cargo.toml` runs Criterion
  microbenchmarks and local-lifecycle benchmarks. The linear route matcher
  covers static and dynamic first/middle/last/missing lookups at route-table
  sizes from 10 through 10,000. The historical route-table cases intentionally
  retain request cloning, local dispatch, handlers, and response finalization.
- `benchmarks/http/bench.sh` runs repeated HTTP/1.1 loopback comparisons of
  rkt, Axum, and Actix. See [http/README.md](http/README.md) for its scenarios
  and reproducibility controls.

Both lockfiles are committed because these are executable benchmark
workspaces. Use `--locked` for comparable runs.

Benchmarks do not replace tests. Keep deterministic behavior and edge-case
coverage under `cargo test`; use benchmarks to measure distributions and catch
performance changes. Likewise, keep the local Criterion cases and HTTP suite
until a benchmark with the same measured scope deliberately supersedes one of
them—their numbers are not interchangeable.
