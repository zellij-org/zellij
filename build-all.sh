#!/bin/sh

# This is temporary while https://github.com/rust-lang/cargo/issues/7004 is open

echo "Building zellij-tile (1/5)..."
cd zellij-tile
cargo build --release
echo "Building status-bar (2/5)..."
cd ../default-tiles/status-bar
cargo build --release
echo "Building strider (3/5)..."
cd ../strider
cargo build --release
echo "Optimising WASM executables (4/5)..."
cd ../..
wasm-opt -O target/wasm32-wasi/release/status-bar.wasm -o target/status-bar.wasm || cp target/wasm32-wasi/release/status-bar.wasm target/status-bar.wasm
wasm-opt -O target/wasm32-wasi/release/strider.wasm -o target/strider.wasm || cp target/wasm32-wasi/release/strider.wasm target/strider.wasm
echo "Building zellij (5/5)..."
cargo build $@