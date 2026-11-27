#!/usr/bin/env bash
set -euo pipefail

FIX_ARGS=()

CLIPPY_RUSTFLAGS=(-W deprecated)

# On native targets, ic_cdk print macros expand to their std:: equivalents
# (the wasm32 run above already enforces disallowed_macros on production code).
DISALLOWED_MACROS_ALLOW=(-- -A clippy::disallowed_macros)

[[ "${1:-}" != "--help" ]] || {
  cat <<-EOF

  Runs Clippy lint checks for every crate of the Cargo workspace.

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

# Every member of the Cargo workspace is linted: the canisters and the crates
# they share. Listing only the canisters in dfx.json left the tests of a
# library crate such as `shared` unchecked.
MANIFESTS=()
while IFS= read -r member; do
  [[ -n "$member" ]] && MANIFESTS+=("$member/Cargo.toml")
done < <(yq -oy '.workspace.members[]' Cargo.toml)

((${#MANIFESTS[@]})) || {
  echo "ERROR: No workspace members found in Cargo.toml."
  exit 1
}

for manifest_path in "${MANIFESTS[@]}"; do
  [[ -f "$manifest_path" ]] || {
    echo "ERROR: $manifest_path does not exist."
    exit 1
  }

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
