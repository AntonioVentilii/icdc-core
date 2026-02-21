#!/usr/bin/env bash
set -euo pipefail

print_help() {
  cat <<-EOF

	Checks that the candid file adheres to our policies.

	EOF
}

[[ "${1:-}" != "--help" ]] || {
  print_help
  exit 0
}

mapfile -t CANDID_FILES < <(
  jq -r '.canisters | to_entries[] | .value.candid? // empty' dfx.json
)

((${#CANDID_FILES[@]})) || {
  echo "ERROR: No candid files found in dfx.json (.canisters[].candid)."
  exit 1
}

has_result_types() {
  local candid_file=$1
  git grep -w Result -- "$candid_file" || git grep -E 'Result_[0-9]' -- "$candid_file"
}

check_result_types() {
  local candid_file=$1
  ! has_result_types "$candid_file" || {
    echo "ERROR: $candid_file should not contain Result or Result_[0-9]."
    echo "       Please define custom Result types with specific names."
    exit 1
  }
}

check() {
  local f
  for f in "${CANDID_FILES[@]}"; do
    check_result_types "$f"
  done
}

check
