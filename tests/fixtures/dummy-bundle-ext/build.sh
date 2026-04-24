#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
command -v cargo-component >/dev/null || cargo install cargo-component --locked
cargo component build --release --target wasm32-wasip2
# cargo-component places the final component in wasm32-wasip2/release/
cp target/wasm32-wasip2/release/dummy_bundle_ext.wasm extension.wasm
echo "Built: $(pwd)/extension.wasm ($(wc -c < extension.wasm) bytes)"
