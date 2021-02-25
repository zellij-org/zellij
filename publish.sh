#!/bin/sh

total=6

echo "Building zellij-tile (1/$total)..."
cd zellij-tile
cargo build --release
echo "Building status-bar (2/$total)..."
cd ../default-tiles/status-bar
cargo build --release
echo "Building strider (3/$total)..."
cd ../strider
cargo build --release
echo "Building tab-bar (4/$total)..."
cd ../tab-bar
cargo build --release
echo "Optimising WASM executables (5/$total)..."
cd ../..
wasm-opt -O target/wasm32-wasi/release/status-bar.wasm -o assets/plugins/status-bar.wasm
wasm-opt -O target/wasm32-wasi/release/strider.wasm -o assets/plugins/strider.wasm
wasm-opt -O target/wasm32-wasi/release/tab-bar.wasm -o assets/plugins/tab-bar.wasm
echo "Publishing zellij (6/$total)..."
cargo publish $@
