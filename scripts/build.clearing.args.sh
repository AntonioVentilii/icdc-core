#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

DFX_NETWORK="${DFX_NETWORK:-local}"

if ! dfx ping "$DFX_NETWORK" >/dev/null 2>&1; then
  echo "ERROR: Unknown or unreachable DFX network '${DFX_NETWORK}'"
  exit 1
fi

ARG_FILE="$(jq -re '.canisters.clearing.init_arg_file' dfx.json)"
mkdir -p "$(dirname "$ARG_FILE")"

if [[ "$DFX_NETWORK" == "local" ]]; then
  LEDGER_ID="$(dfx canister id ledger --network "$DFX_NETWORK" 2>/dev/null || true)"
else
  LEDGER_ID="$(jq -re ".ledger.\"$DFX_NETWORK\"" canister_ids.json 2>/dev/null || true)"
fi
if [[ -z "$LEDGER_ID" ]]; then
  echo "ERROR: Cannot resolve canister id for 'ledger' on network '$DFX_NETWORK'."
  echo "       Deploy the ledger (vUSD) first; clearing declares a dependency on it in dfx.json."
  exit 1
fi

# Anonymous principal (placeholder until EVM / signer canisters are wired).
ANONYMOUS_PRINCIPAL="aaaaa-aa"

CLEARING_INSURANCE_FEE_BPS="${CLEARING_INSURANCE_FEE_BPS:-10}"
CLEARING_PROTOCOL_FEE_BPS="${CLEARING_PROTOCOL_FEE_BPS:-5}"
CLEARING_VERSION="${CLEARING_VERSION:-1}"
CLEARING_EVM_RPC_PRINCIPAL="${CLEARING_EVM_RPC_PRINCIPAL:-$ANONYMOUS_PRINCIPAL}"
CLEARING_SIGNER_PRINCIPAL="${CLEARING_SIGNER_PRINCIPAL:-$ANONYMOUS_PRINCIPAL}"
CLEARING_INTERNAL_ASSET_ID="${CLEARING_INTERNAL_ASSET_ID:-vUSD}"
CLEARING_INTERNAL_SYMBOL="${CLEARING_INTERNAL_SYMBOL:-vUSD}"
CLEARING_INTERNAL_DECIMALS="${CLEARING_INTERNAL_DECIMALS:-8}"

echo "Building clearing init args for network=$DFX_NETWORK (ledger=$LEDGER_ID) -> $ARG_FILE"

cat >"$ARG_FILE" <<-EOF
(
  record {
    insurance_fund_fee_ratio = $CLEARING_INSURANCE_FEE_BPS : nat16;
    internal_ledger = record {
      decimals = $CLEARING_INTERNAL_DECIMALS : nat8;
      asset = variant { Icrc = principal "$LEDGER_ID" };
      is_enabled = true;
      allowed_balance_domains = vec { variant { Settlement }; variant { Playground } };
      oracle_id = null;
      asset_id = "$CLEARING_INTERNAL_ASSET_ID";
      symbol = "$CLEARING_INTERNAL_SYMBOL";
    };
    signer_canister = principal "$CLEARING_SIGNER_PRINCIPAL";
    version = $CLEARING_VERSION : nat32;
    evm_rpc = principal "$CLEARING_EVM_RPC_PRINCIPAL";
    protocol_fee_ratio = $CLEARING_PROTOCOL_FEE_BPS : nat16;
  }
)
EOF
