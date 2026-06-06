#!/usr/bin/env bash

set -euo pipefail

# Builds the install argument for the minter canister.
#
# The minter uses the ICRC ledger-suite convention of a single candid argument
# shaped as `variant { Init : Config; Upgrade : opt UpgradeArg }`:
#   - on first install we emit `Init` with the full configuration;
#   - on upgrade we emit `Upgrade = null`, because the running configuration is
#     persisted in stable memory and restored in `post_upgrade`. Operators that
#     need to change config at runtime use the `update_config` method instead;
#     a non-null `Upgrade` arg is only there for the rare migration case.
#
# Choosing Init vs Upgrade based on the current install state is what stops dfx
# from prompting for the argument on every deploy.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

DFX_NETWORK="${DFX_NETWORK:-local}"

if ! dfx ping "$DFX_NETWORK" >/dev/null 2>&1; then
  echo "ERROR: Unknown or unreachable DFX network '${DFX_NETWORK}'"
  exit 1
fi

ARG_FILE="$(jq -re '.canisters.minter.init_arg_file' dfx.json)"
mkdir -p "$(dirname "$ARG_FILE")"

VARIANT="$(scripts/check.canister.installed.sh minter "$DFX_NETWORK" --print-variant)"

if [[ "$VARIANT" == "Upgrade" ]]; then
  echo "Building minter Upgrade args for network=$DFX_NETWORK -> $ARG_FILE"
  cat >"$ARG_FILE" <<-EOF
	(
	  variant { Upgrade = null }
	)
	EOF
  exit 0
fi

# Init variant: resolve the ledger canister id the same way clearing does.
if [[ "$DFX_NETWORK" == "local" ]]; then
  LEDGER_ID="$(dfx canister id ledger --network "$DFX_NETWORK" 2>/dev/null || true)"
else
  LEDGER_ID="$(jq -re ".ledger.\"$DFX_NETWORK\"" canister_ids.json 2>/dev/null || true)"
fi
if [[ -z "$LEDGER_ID" ]]; then
  echo "ERROR: Cannot resolve canister id for 'ledger' on network '$DFX_NETWORK'."
  echo "       Deploy the ledger (vUSD) first; the minter mints through it."
  exit 1
fi

# Authorized callers default to empty; populate via MINTER_AUTHORIZED_CALLERS
# (comma-separated principals) or later with the `update_config` method.
AUTHORIZED_CALLERS_VEC=""
if [[ -n "${MINTER_AUTHORIZED_CALLERS:-}" ]]; then
  IFS=',' read -ra _callers <<<"$MINTER_AUTHORIZED_CALLERS"
  for _caller in "${_callers[@]}"; do
    _caller="$(echo "$_caller" | xargs)" # trim whitespace
    [[ -z "$_caller" ]] && continue
    AUTHORIZED_CALLERS_VEC+="principal \"$_caller\"; "
  done
fi

echo "Building minter Init args for network=$DFX_NETWORK (ledger=$LEDGER_ID) -> $ARG_FILE"

cat >"$ARG_FILE" <<-EOF
	(
	  variant {
	    Init = record {
	      ledger_canister = principal "$LEDGER_ID";
	      authorized_callers = vec { $AUTHORIZED_CALLERS_VEC};
	    }
	  }
	)
	EOF
