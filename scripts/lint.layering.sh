#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$(realpath "$0")")/.." || exit

print_help() {
  cat <<-EOF

	Fails if a lower layer of the clearing crate imports from its api/ layer.

	Dependencies point down: lib.rs -> api/ -> domain modules -> types/, utils/,
	memory.rs (see docs/ai/structure.md). A type or error a lower layer needs
	belongs in types/, not in api/.

	The files in BASELINE already break the rule and are tolerated until they are
	fixed. Remove a file from BASELINE once it is clean; never add one.
	EOF
}

[[ "${1:-}" != "--help" ]] || {
  print_help
  exit 0
}

LOWER_LAYERS=(
  src/clearing/src/account
  src/clearing/src/assets
  src/clearing/src/migrations
  src/clearing/src/payoffs
  src/clearing/src/trade
  src/clearing/src/types
  src/clearing/src/utils
  src/clearing/src/guards.rs
  src/clearing/src/memory.rs
  src/clearing/src/traits.rs
)

BASELINE=(
  src/clearing/src/account/reassignment.rs
  src/clearing/src/account/service.rs
  src/clearing/src/trade/service.rs
  src/clearing/src/types/plans.rs
  src/clearing/src/utils/series.rs
)

# Prints "file:line: text" for every import of crate::api, either written in full
# or as an `api::` item inside a `use crate::{ ... }` group. Doc comments are skipped.
api_imports() {
  git ls-files -- "${LOWER_LAYERS[@]}" | grep '\.rs$' | while read -r file; do
    awk -v file="$file" '
      /^[[:space:]]*\/\/\// { next }
      /^[[:space:]]*(pub(\([a-z]+\))? )?use [a-z_]+::\{/ {
        root = $0
        sub(/^[[:space:]]*(pub(\([a-z]+\))? )?use /, "", root)
        sub(/::.*/, "", root)
      }
      /crate::api::/ || (root == "crate" && /^[[:space:]]*api::/) {
        print file ":" FNR ": " $0
      }
      /^[[:space:]]*\};/ { root = "" }
    ' "$file"
  done
}

is_baseline() {
  local file="$1" allowed
  for allowed in "${BASELINE[@]}"; do
    [[ "$file" == "$allowed" ]] && return 0
  done
  return 1
}

matches="$(api_imports)"

violations=""
while IFS= read -r match; do
  [[ -n "$match" ]] || continue
  is_baseline "${match%%:*}" || violations+="$match"$'\n'
done <<<"$matches"

[[ -z "$violations" ]] || {
  echo "ERROR: lower layers of the clearing must not import from api/:"
  printf '%s' "$violations"
  echo "Move the type or error into types/ and import it from there (docs/ai/structure.md)."
  exit 1
}

# A baseline entry that no longer imports from api/ must be removed, so the
# tolerated set can only shrink.
for allowed in "${BASELINE[@]}"; do
  grep -q "^$allowed:" <<<"$matches" || {
    echo "ERROR: $allowed no longer imports from api/: remove it from BASELINE in $0."
    exit 1
  }
done
