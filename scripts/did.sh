#!/usr/bin/env bash
set -euo pipefail

# candid-extractor < 0.1.5 fails on WASM from ic-cdk 0.18+ with errors like:
#   unknown import: `ic0::canister_liquid_cycle_balance128` has not been defined
# 0.1.5+ mocks arbitrary ic0 imports (see candid-extractor changelog).
MIN_CANDID_EXTRACTOR_PATCH=5

require_candid_extractor() {
  if ! command -v candid-extractor >/dev/null 2>&1; then
    echo "error: candid-extractor not found in PATH." >&2
    echo "  From the repository root, run: ./scripts/setup candid-extractor" >&2
    echo "  (installs the version pinned in dev-tools.json; ensure ~/.cargo/bin is on PATH)" >&2
    exit 1
  fi

  local ver raw major minor patch
  raw="$(candid-extractor --version 2>&1 | awk 'NR==1 {print $2}')"
  ver="${raw%%-*}"
  # Do not append fake segments: with IFS=., read puts all extra segments into the last variable
  # (e.g. 0.1.6.0.0 would set patch to "6.0.0" and break arithmetic).
  IFS=. read -r major minor patch <<<"$ver"
  major="${major:-0}"
  minor="${minor:-0}"
  patch="${patch:-0}"

  if ((major == 0 && minor == 1 && patch < MIN_CANDID_EXTRACTOR_PATCH)); then
    echo "error: candid-extractor ${raw} is too old (need >= 0.1.${MIN_CANDID_EXTRACTOR_PATCH} for current ic-cdk)." >&2
    echo "  From the repository root, run: ./scripts/setup candid-extractor" >&2
    exit 1
  fi
}

function generate_did() {
  local canister=$1

  local crate_name="${canister//-/_}"

  canister_root="src/$canister"

  cargo build --manifest-path="$canister_root/Cargo.toml" \
    --target wasm32-unknown-unknown \
    --release --package "$canister"

  candid-extractor "target/wasm32-unknown-unknown/release/$crate_name.wasm" >"$canister_root/$canister.did"
}

require_candid_extractor

generate_did clearing
generate_did registry
generate_did minter
