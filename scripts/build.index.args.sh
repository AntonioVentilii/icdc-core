#!/usr/bin/env bash
set -euo pipefail

ECHO "Building Index args..."

MODE="${1:-auto}"
case "$MODE" in
auto | init | upgrade) ;;
*)
  ECHO "Usage: $0 [auto|init|upgrade]"
  ECHO "       mode: auto (default), init, upgrade"
  exit 1
  ;;
esac

DFX_NETWORK="${DFX_NETWORK:-local}"
ECHO "Building Index args for network: ${DFX_NETWORK}"

if ! dfx ping "$DFX_NETWORK" >/dev/null 2>&1; then
  echo "ERROR: Unknown DFX network '${DFX_NETWORK:-<unset>}'"
  exit 1
fi

if [[ "$DFX_NETWORK" == "local" ]]; then
  CANISTER_ID_LEDGER="$(dfx canister id ledger)"
else
  CANISTER_ID_LEDGER="$(jq -re ".ledger.\"$DFX_NETWORK\"" canister_ids.json)"
fi

ECHO "Using Ledger canister ID: $CANISTER_ID_LEDGER"

if [[ "$MODE" == "upgrade" ]]; then
  VARIANT="Upgrade"
elif [[ "$MODE" == "init" ]]; then
  VARIANT="Init"
else
  if scripts/check.canister.installed.sh index "$DFX_NETWORK"; then
    VARIANT="Upgrade"
  else
    VARIANT="Init"
  fi
fi

ARG_FILE="$(jq -re .canisters.index.init_arg_file dfx.json)"

mkdir -p "$(dirname "$ARG_FILE")"

if [[ "$VARIANT" == "Upgrade" ]]; then

  # Use Upgrade variant: same values, but everything is opt
  cat <<-EOF >"$ARG_FILE"
  (
  	opt variant {
  		Upgrade = record {
  			ledger_id = opt principal "$CANISTER_ID_LEDGER";
  			retrieve_blocks_from_ledger_interval_seconds = opt 10
  		}
  	}
  )
EOF

else

  # Original Init variant
  cat <<-EOF >"$ARG_FILE"
  (
  	opt variant {
  		Init = record {
  			ledger_id = principal "$CANISTER_ID_LEDGER";
  			retrieve_blocks_from_ledger_interval_seconds = opt 10
  		}
  	}
  )
EOF

fi
