#!/usr/bin/env bash
set -euo pipefail

print_help() {
  cat <<-EOF

		Checks whether a canister has a WASM module installed.

		By default, the script exits with code:
		  0  if the canister is installed (Module hash present)
		  1  if the canister is not installed or doesn't exist

		Optional flags let you print a human-friendly status or the desired
		ICRC-1/2 style variant name to use during deployment (Init/Upgrade).
	EOF

  print_usage
}

print_usage() {
  cat <<-EOF

		Usage:
		  $(basename "$0") <CANISTER_NAME> <NETWORK> [--print-status|--print-variant]

		Examples:
		  $(basename "$0") ledger local
		  $(basename "$0") ledger ic --print-status
		  $(basename "$0") ledger staging --print-variant
	EOF
}

[[ "${1:-}" != "--help" ]] || {
  print_help
  exit 0
}

if (($# < 2 || $# > 3)); then
  echo "ERROR: Expected 2 or 3 arguments." >&2
  print_usage
  exit 1
fi

CANISTER_NAME="$1"
NETWORK="$2"
MODE="${3:-}" # empty | --print-status | --print-variant

if [[ -n "$MODE" && "$MODE" != "--print-status" && "$MODE" != "--print-variant" ]]; then
  echo "ERROR: Unknown flag '$MODE'." >&2
  print_usage
  exit 1
fi

# Treat as "installed" if `dfx canister status` succeeds and Module hash is present and not 'None'
if ! dfx canister status "$CANISTER_NAME" --network "$NETWORK" >/dev/null 2>&1; then
  # Canister doesn't exist or is unreachable on this network
  if [[ "$MODE" == "--print-status" ]]; then
    echo "not installed"
  elif [[ "$MODE" == "--print-variant" ]]; then
    echo "Init"
  fi
  exit 1
fi

MODULE_HASH="$(dfx canister status "$CANISTER_NAME" --network "$NETWORK" | awk -F': ' '/Module hash/ {print $2}')"
IS_INSTALLED=false
if [[ -n "${MODULE_HASH:-}" && "$MODULE_HASH" != "None" ]]; then
  IS_INSTALLED=true
fi

if [[ "$MODE" == "--print-status" ]]; then
  if [[ "$IS_INSTALLED" == "true" ]]; then
    echo "installed"
  else
    echo "not installed"
  fi
elif [[ "$MODE" == "--print-variant" ]]; then
  if [[ "$IS_INSTALLED" == "true" ]]; then
    echo "Upgrade"
  else
    echo "Init"
  fi
fi

if [[ "$IS_INSTALLED" == "true" ]]; then
  exit 0
else
  exit 1
fi
