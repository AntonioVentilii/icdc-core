#!/usr/bin/env bash

function generate_did() {
  local canister=$1

  local crate_name="${canister//-/_}"

  canister_root="src/$canister"

  cargo build --manifest-path="$canister_root/Cargo.toml" \
    --target wasm32-unknown-unknown \
    --release --package "$canister"

  # cargo install candid-extractor
  candid-extractor "target/wasm32-unknown-unknown/release/$crate_name.wasm" >"$canister_root/$canister.did"
}

generate_did bucket
generate_did directory
