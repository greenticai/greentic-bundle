# bundle-standard-core

Pure-Rust workspace assembly + ZIP for the `bundle-standard` Greentic
bundle-extension recipe. Cross-compiles cleanly to `wasm32-wasip2` and
runs entirely in-memory (no `tempfile`, no `walkdir`, no native `std::fs`
mutation).

Consumers: `bundle-standard` WASM extension (in `greentic-bundle-extensions`),
plus the legacy builtin bridge in `greentic-bundle/src/ext/builtin_bridge.rs`
during the migration window.
