#!/usr/bin/env bash

source "$(dirname "$0")/utils.sh" "$@"
source "$(dirname "$0")/init.common.sh"
source "$(dirname "$0")/init.market.common.sh"

# Seeds ICP / TESTICP / TICRC1 (Settlement or Playground domain) liquidity on
# every active market with a RANDOM mid. Token is chosen via the TOKEN env var.
# Shared logic lives in init.market.common.sh.

resolve_maker_identity

TOKEN=${TOKEN:-TESTICP}

if [[ "$TOKEN" == "ICP" ]]; then
  TARGET_SYMBOL=$ICP_SYMBOL
  TARGET_LEDGER=$ICP_LEDGER
  TARGET_DECIMALS=$ICP_DECIMALS
  TARGET_FEE=$DEFAULT_LEDGER_FEE
  TARGET_DOMAIN="opt variant { Settlement }"
elif [[ "$TOKEN" == "TICRC1" ]]; then
  TARGET_SYMBOL=$TICRC1_SYMBOL
  TARGET_LEDGER=$TICRC1_LEDGER
  TARGET_DECIMALS=$TICRC1_DECIMALS
  TARGET_FEE=$DEFAULT_LEDGER_FEE
  TARGET_DOMAIN="opt variant { Playground }"
else
  TARGET_SYMBOL=$TESTICP_SYMBOL
  TARGET_LEDGER=$TESTICP_LEDGER
  TARGET_DECIMALS=$TESTICP_DECIMALS
  TARGET_FEE=$DEFAULT_LEDGER_FEE
  TARGET_DOMAIN="opt variant { Playground }"
fi

echo "Using token: $TARGET_SYMBOL (${TARGET_DOMAIN})"

# --- 1. FETCH + PARSE ACTIVE MARKETS ---
fetch_and_parse_markets || exit 0

# --- 2. REQUIRED COLLATERAL ---
REQ_BASE_UNITS=$(compute_required_base_units "$TARGET_DECIMALS")
echo "Required $TARGET_SYMBOL: $REQ_BASE_UNITS ledger base units"

# --- 3. BALANCE CHECK (top up from faucet until sufficient) ---
echo "Checking balance..."
CUR_BAL_BASE=$(read_ledger_balance "$TARGET_LEDGER" "$MY_PRINCIPAL")
echo "Current balance: $CUR_BAL_BASE base units"

while [[ "$CUR_BAL_BASE" -lt "$REQ_BASE_UNITS" ]]; do
  echo "Current balance ($CUR_BAL_BASE) is less than required ($REQ_BASE_UNITS). Topping up from faucet..."
  dfx identity use default
  if [[ -n "$MY_ACCOUNT_ID" ]]; then
    dfx canister call "$FAUCET_CANISTER" transfer_icp "(\"$MY_ACCOUNT_ID\")" --network "$NETWORK"
  else
    echo "Error: Could not determine Account ID for $MY_PRINCIPAL. Falling back to principal..."
    dfx canister call "$FAUCET_CANISTER" transfer_icrc1 "(principal \"$MY_PRINCIPAL\")" --network "$NETWORK"
  fi
  dfx identity use "$MY_IDENTITY"

  echo "Waiting 5 seconds for balance to update..."
  sleep 5
  CUR_BAL_BASE=$(read_ledger_balance "$TARGET_LEDGER" "$MY_PRINCIPAL")
done

echo "Balance sufficient ($CUR_BAL_BASE base units)."

# --- 4. DEPOSIT COLLATERAL ---
deposit_all_collateral "$TARGET_SYMBOL" "$TARGET_LEDGER" "$TARGET_FEE" "$TARGET_DOMAIN" "$CUR_BAL_BASE"

# --- 5. PLACE ORDERS (random mid) ---
place_scalar_orders_random
place_categorical_orders_random

cleanup_market_tmps
echo "Finished."
