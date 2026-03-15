#!/bin/bash

# Download ICRC-1 ledger canister (used for vUSD)

DIR=target/ic

if [ ! -d "$DIR" ]; then
  mkdir -p "$DIR"
fi

# Use the version from dfx.json if possible, but for simplicity we'll use a known good one
# or match the one in dfx.json URL
URL="https://github.com/dfinity/ic/releases/download/ledger-suite-icrc-2025-06-19/ic-icrc1-ledger.wasm.gz"

scripts/download-immutable.sh "$URL" "$DIR"/ledger.wasm.gz
gunzip --force "$DIR"/ledger.wasm.gz
