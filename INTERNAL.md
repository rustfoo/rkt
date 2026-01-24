This file contains notes for maintainers.

# Versions

This section lists file paths and approximate line numbers where current
version package numbers are mentions - these need to be consistent for
a release.

## rkt(_codegen,_http)

* ./contrib/db_pools/lib/Cargo.toml (36)
* ./contrib/dyn_templates/Cargo.toml (39)
* ./contrib/sync_db_pools/lib/Cargo.toml (46, 55)
* ./contrib/ws/Cargo.toml (27)
* ./core/codegen/Cargo.toml (3)
* ./core/http/Cargo.toml (3)
* ./core/lib/Cargo.toml (3, 91, 96)
* ./docs/tests/Cargo.toml (3)

* ./contrib/db_pools/lib/src/diesel.rs (11)
* ./core/codegen/src/lib.rs (22)
* ./core/lib/src/listener/quic.rs (9)
* ./core/lib/src/serde/json.rs (13)
* ./core/lib/src/serde/msgpack.rs (13)
* ./core/lib/src/serde/uuid.rs (11)
* ./core/lib/src/lib.rs (28, 78, 85)

* ./docs/guide/03-getting-started.md (50)
* ./docs/guide/05-requests.md (665, 844)
* ./docs/guide/10-configuration.md (250, 325)
* ./docs/guide/12-pastebin.md (59)
* ./docs/guide/14-faq.md (585)
* ./README.md (15, 21)


## rkt_db_pools(_codegen)

* ./contrib/db_pools/codegen/Cargo.toml (3)
* ./contrib/db_pools/lib/Cargo.toml (3, 45)

* ./contrib/db_pools/lib/src/diesel.rs (16)
* ./contrib/db_pools/lib/src/lib.rs (11, 174)

* ./contrib/db_pools/README.md (21)
* ./docs/guide/07-state.md (240, 309)


## rkt_dyn_templates

* ./contrib/dyn_templates/Cargo.toml (3)

* ./contrib/dyn_templates/src/lib.rs (16)

* ./contrib/dyn_templates/README.md (26)


## rkt_sync_db_pools(_codegen)

* ./contrib/sync_db_pools/codegen/Cargo.toml (3)
* ./contrib/sync_db_pools/lib/Cargo.toml (3, 41)

* ./contrib/sync_db_pools/lib/src/lib.rs (34)

* ./contrib/sync_db_pools/README.md (23)


## rkt_ws

* ./contrib/ws/Cargo.toml (3)

* ./contrib/ws/README.md (19)