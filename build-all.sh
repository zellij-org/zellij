#!/bin/sh

total=5

# This is temporary while https://github.com/rust-lang/cargo/issues/7004 is open

echo "Building status-bar (1/$total)..."
cd default-tiles/status-bar
cargo build --release --target-dir ../../target

echo "Building strider (2/$total)..."
cd ../strider
cargo build --release --target-dir ../../target

echo "Building tab-bar (3/$total)..."
cd ../tab-bar
cargo build --release --target-dir ../../target

echo "Optimising WASM executables (4/$total)..."
cd ../..
wasm-opt -O target/wasm32-wasi/release/status-bar.wasm -o target/status-bar.wasm || cp target/wasm32-wasi/release/status-bar.wasm target/status-bar.wasm
wasm-opt -O target/wasm32-wasi/release/strider.wasm -o target/strider.wasm || cp target/wasm32-wasi/release/strider.wasm target/strider.wasm
wasm-opt -O target/wasm32-wasi/release/tab-bar.wasm -o target/tab-bar.wasm || cp target/wasm32-wasi/release/tab-bar.wasm target/tab-bar.wasm
echo "Building zellij (5/$total)..."
cargo build --target-dir target $@
