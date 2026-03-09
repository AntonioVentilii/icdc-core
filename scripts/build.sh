#!/usr/bin/env bash

cargo build --locked --target wasm32-unknown-unknown --release -p clearing

gzip -c target/wasm32-unknown-unknown/release/clearing.wasm >target/wasm32-unknown-unknown/release/clearing.wasm.gz

cargo build --locked --target wasm32-unknown-unknown --release -p registry

gzip -c target/wasm32-unknown-unknown/release/registry.wasm >target/wasm32-unknown-unknown/release/registry.wasm.gz

cargo build --locked --target wasm32-unknown-unknown --release -p minter

gzip -c target/wasm32-unknown-unknown/release/minter.wasm >target/wasm32-unknown-unknown/release/minter.wasm.gz
