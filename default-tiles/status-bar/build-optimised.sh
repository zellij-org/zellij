#!/bin/sh

# Build a release WASM from Rust with lto on
cargo build --release
# Further optimise for speed (and size)
wasm-opt -O target/wasm32-wasi/release/status-bar.wasm -o target/status-bar.wasm