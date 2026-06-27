# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `Request::header()` and `Request::header_values()` for reading individual
  request headers cheaply, without building the full header map. Prefer these
  over `Request::headers()` in `FromRequest` implementations.
- `debug_headers` configuration option (env `ROCKET_DEBUG_HEADERS`) to enable
  logging of request and response headers. Defaults to `false`.

### Changed

- Request headers are now parsed lazily: requests whose handlers never inspect
  headers no longer pay to copy the connection's header map. Common lookups
  (`Content-Type`, `Accept`, proxy-proto, and connection upgrades) read directly
  from the parsed source.
- Request and response headers are no longer logged at the `debug` level by
  default; set `debug_headers = true` (or `ROCKET_DEBUG_HEADERS=1`) to restore
  the previous behavior.

## 1.0.1 - 2026-06-23

### Fixed

- Remove deprecated call to set\_linger

### Changed

- Upgrade dependencies: x509-parser 0.18, rand 0.10, sqlx 0.9, tokio-tungstenite 0.29

## 1.0.0 - 2026-06-04

> [!NOTE]
> `rkt` is a fork of [Rocket](https://github.com/rwf2/Rocket) `0.5.1`.

### Added

- Guide published via Docusaurus.

### Removed

- `rkt_db_pools` and `rkt_sync_db_pools` contrib crates (and their internal
  codegen crates) have been removed from the workspace.
