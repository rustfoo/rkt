# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
