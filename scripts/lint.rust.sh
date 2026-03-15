#!/usr/bin/env bash
set -euo pipefail

FIX_ARGS=()

CLIPPY_RUSTFLAGS=(-W deprecated)

# On native targets, ic_cdk print macros expand to their std:: equivalents
# (the wasm32 run above already enforces disallowed_macros on production code).
DISALLOWED_MACROS_ALLOW=(-- -A clippy::disallowed_macros)

[[ "${1:-}" != "--help" ]] || {
  cat <<-EOF

  Runs Clippy lint checks for the Rust canister crates.

	Usage: $0 [--fix]

	Options:
	  --fix     Apply automatic fixes with cargo clippy (allow dirty and staged).
	  --help    Show this help message and exit.

	EOF
  exit 0
}

if [[ "${1:-}" == "--fix" ]]; then
  FIX_ARGS+=(--fix --allow-dirty --allow-staged)
fi

CANISTERS=()
while IFS= read -r canister; do
  [[ -n "$canister" ]] && CANISTERS+=("$canister")
done < <(jq -r '.canisters | keys[]' dfx.json)

((${#CANISTERS[@]})) || {
  echo "ERROR: No canisters found in dfx.json."
  exit 1
}

for canister in "${CANISTERS[@]}"; do
  manifest_path="src/$canister/Cargo.toml"

  # Skip non-Rust canisters (or different layouts)
  [[ -f "$manifest_path" ]] || continue

  RUSTFLAGS="${CLIPPY_RUSTFLAGS[*]}" cargo clippy ${FIX_ARGS[@]+"${FIX_ARGS[@]}"} \
    --manifest-path "$manifest_path" \
    --locked \
    --target wasm32-unknown-unknown \
    --all-features

  RUSTFLAGS="${CLIPPY_RUSTFLAGS[*]}" cargo clippy ${FIX_ARGS[@]+"${FIX_ARGS[@]}"} \
    --manifest-path "$manifest_path" \
    --locked \
    --all-features \
    --tests \
    "${DISALLOWED_MACROS_ALLOW[@]}"

  RUSTFLAGS="${CLIPPY_RUSTFLAGS[*]}" cargo clippy ${FIX_ARGS[@]+"${FIX_ARGS[@]}"} \
    --manifest-path "$manifest_path" \
    --locked \
    --all-features \
    --examples \
    --benches \
    "${DISALLOWED_MACROS_ALLOW[@]}"
done
