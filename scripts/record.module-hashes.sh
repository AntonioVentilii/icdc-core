#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$(realpath "$0")")/.."

print_usage() {
  cat <<-EOF

		Prints a Markdown table of the module hash each canister has on chain.

		Usage:
		  $(basename "$0") <NETWORK> <GIT_REF>

		Examples:
		  $(basename "$0") ic v0.1.9
		  $(basename "$0") staging main
	EOF
}

if (($# != 2)); then
  echo "ERROR: Expected 2 arguments." >&2
  print_usage
  exit 1
fi

NETWORK="$1"
GIT_REF="$2"

# The module hash is public, so this needs no identity of ours. stderr is dropped
# because a runner with no identity yet (a deploy that failed early) gets a generated
# "default" one, and dfx writes its seed phrase there.
read_module_hash() {
  dfx --identity anonymous canister --network "$NETWORK" info "$1" 2>/dev/null |
    awk '/^Module hash:/ { print $3 }'
}

CANISTERS="$(jq -r --arg network "$NETWORK" \
  'to_entries[] | select(.value[$network]) | "\(.key) \(.value[$network])"' canister_ids.json)"

echo "| Canister | Canister ID | Module hash | Git ref |"
echo "| --- | --- | --- | --- |"
while read -r canister canister_id; do
  module_hash="$(read_module_hash "$canister_id" || true)"
  if [[ -z "$module_hash" ]]; then
    echo "::warning::Could not read the module hash of $canister ($canister_id) on $NETWORK." >&2
    module_hash="unavailable"
  fi
  echo "| $canister | \`$canister_id\` | \`$module_hash\` | \`$GIT_REF\` |"
done <<<"$CANISTERS"
