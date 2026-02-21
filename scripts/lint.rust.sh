#!/usr/bin/env bash
set -euo pipefail

mapfile -t CANISTERS < <(jq -r '.canisters | keys[]' dfx.json)

((${#CANISTERS[@]})) || {
  echo "ERROR: No canisters found in dfx.json."
  exit 1
}

for canister in "${CANISTERS[@]}"; do
  manifest_path="src/$canister/Cargo.toml"

  # Skip non-Rust canisters (or different layouts)
  [[ -f "$manifest_path" ]] || continue

  cargo clippy \
    --manifest-path "$manifest_path" \
    --locked \
    --target wasm32-unknown-unknown \
    --all-features
done
