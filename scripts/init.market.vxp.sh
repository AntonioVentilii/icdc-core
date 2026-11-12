#!/usr/bin/env bash

source "$(dirname "$0")/utils.sh" "$@"
source "$(dirname "$0")/init.common.sh"
source "$(dirname "$0")/init.market.common.sh"

# Seeds VXP (ViciXp domain) liquidity on every active market with a RANDOM mid.
# For consensus-driven mids (e.g. the Vici World Cup deck) see
# init.market.vxp.worldcup.sh. Shared logic lives in init.market.common.sh.

resolve_maker_identity

# --- 1. FETCH + PARSE ACTIVE MARKETS ---
fetch_and_parse_markets || exit 0

# --- 2. REQUIRED COLLATERAL ---
REQ_BASE_UNITS=$(compute_required_base_units "$VICI_XP_DECIMALS")
echo "Required $VICI_XP_SYMBOL: $REQ_BASE_UNITS ledger base units"

# --- 3. BALANCE CHECK (no faucet for VXP) ---
echo "Checking balance..."
CUR_BAL_BASE=$(read_ledger_balance "$VICI_XP_LEDGER" "$MY_PRINCIPAL")
echo "Current balance: $CUR_BAL_BASE base units"

if [[ "$CUR_BAL_BASE" -lt "$REQ_BASE_UNITS" ]]; then
  echo "Error: Current balance ($CUR_BAL_BASE base units) is less than required ($REQ_BASE_UNITS base units). Please ensure you have sufficient $VICI_XP_SYMBOL tokens."
  cleanup_market_tmps
  exit 1
fi
echo "Balance sufficient ($CUR_BAL_BASE base units)."

# --- 4. DEPOSIT COLLATERAL ---
deposit_all_collateral "$VICI_XP_SYMBOL" "$VICI_XP_LEDGER" "$VICI_XP_TRANSFER_FEE" "opt variant { ViciXp }" "$CUR_BAL_BASE"

# --- 5. PLACE ORDERS (random mid) ---
place_scalar_orders_random
place_categorical_orders_random

cleanup_market_tmps
echo "Finished."
