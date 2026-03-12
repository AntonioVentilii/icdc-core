#!/usr/bin/env bash

source "$(dirname "$0")/utils.sh" "$@"

# --- CONFIGURATION ---
NUM_ORDERS_PER_SIDE=${NUM_ORDERS_PER_SIDE:-3}
ORDER_VALUE_USD=${ORDER_VALUE_USD:-10}
MID_MIN=${MID_MIN:-10}
MID_MAX=${MID_MAX:-90}
SPREAD_MIN=${SPREAD_MIN:-2}
SPREAD_MAX=${SPREAD_MAX:-8}
WIGGLE_ROOM=${WIGGLE_ROOM:-1.3}

# Canister IDs
CLEARING_CANISTER=$(dfx canister id clearing --network "$NETWORK" 2>/dev/null)
TEST_ICP_LEDGER="xafvr-biaaa-aaaai-aql5q-cai"
FAUCET_CANISTER="nqoci-rqaaa-aaaap-qp53q-cai"

if [[ -z "$CLEARING_CANISTER" ]]; then
  echo "Error: Could not find clearing canister ID."
  exit 1
fi

MY_PRINCIPAL=$(dfx identity get-principal)
MY_ACCOUNT_ID=$(dfx ledger account-id --of-principal "$MY_PRINCIPAL" 2>/dev/null || echo "")
echo "My Principal: $MY_PRINCIPAL"
echo "My Account ID: $MY_ACCOUNT_ID"

## --- 1. FETCH ACTIVE MARKETS ---
echo "Fetching active markets..."
ALL_SERIES=$(dfx canister call clearing list_series --network "$NETWORK") || {
  echo "Failed to fetch series"
  exit 1
}

# Function to place orders for a given set of outcome prices
# Usage: place_outcome_orders SERIES_ID OUTCOME_ID MID_VAL SPREAD_VAL
place_outcome_orders() {
  local SID=$1
  local OID=$2
  local MID_VAL=$3    # in 6-decimal USD (e.g. 500000 for 0.5)
  local SPREAD_VAL=$4 # in 6-decimal USD

  echo "    Placing orders for Outcome: ${OID:-Binary} (Mid: $MID_VAL, Spread: $SPREAD_VAL)"

  for i in $(seq 1 "$NUM_ORDERS_PER_SIDE"); do
    local OFFSET=$(((i - 1) * SPREAD_VAL + SPREAD_VAL / 2))
    local BID_VAL=$((MID_VAL - OFFSET))
    local ASK_VAL=$((MID_VAL + OFFSET))

    # Keep within [0.01, 0.99] range
    [[ "$BID_VAL" -lt 10000 ]] && BID_VAL=10000
    [[ "$ASK_VAL" -gt 990000 ]] && ASK_VAL=990000

    local OBID
    OBID=$(openssl rand -hex 8)
    local OASK
    OASK=$(openssl rand -hex 8)
    local QTY=10

    local OARG="$OID"
    if [[ "$OID" != "null" ]]; then OARG="opt \"$OID\""; fi

    echo "      Level $i: Buy @ $BID_VAL, Sell @ $ASK_VAL"
    dfx canister call clearing submit_limit_order "(record { qty = $QTY : int; outcome_id = $OARG; series_id = \"$SID\"; side = variant { Buy }; order_id = \"$OBID\"; price = record { decimal = record { value = $BID_VAL : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; })" --network "$NETWORK" >/dev/null
    dfx canister call clearing submit_limit_order "(record { qty = $QTY : int; outcome_id = $OARG; series_id = \"$SID\"; side = variant { Sell }; order_id = \"$OASK\"; price = record { decimal = record { value = $ASK_VAL : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; })" --network "$NETWORK" >/dev/null
  done
}

# --- PARSE MARKETS ---
BINARY_MARKETS=$(echo "$ALL_SERIES" | grep -B 2 'payoff_type = variant { Binary }' | grep 'series_id ="' | sed 's/.*series_id ="\([^"]*\)".*/\1/' || true)

# Group Categorical Outcomes by Series
TMP_CAT_INFO=$(mktemp)
echo "$ALL_SERIES" | awk '
/series_id = "/ { series_id = $3; gsub("\"", "", series_id); gsub(";", "", series_id) }
/payoff_type = variant { Categorical }/ { is_cat = 1 }
/id = "/ { if (is_cat) { oid = $3; gsub("\"", "", oid); gsub(";", "", oid); print series_id, oid } }
/};/ { if ($1 == "};") { is_cat = 0 } }
' >"$TMP_CAT_INFO"

NUM_BINARY=0
[[ -n "$BINARY_MARKETS" ]] && NUM_BINARY=$(echo "$BINARY_MARKETS" | wc -l | xargs)
NUM_CATEGORICAL_OUTCOMES=$(wc -l <"$TMP_CAT_INFO" | xargs)
TOTAL_UNITS=$((NUM_BINARY + NUM_CATEGORICAL_OUTCOMES))

if [[ "$TOTAL_UNITS" -eq 0 ]]; then
  echo "No active markets found."
  rm "$TMP_CAT_INFO"
  exit 0
fi

echo "Found $NUM_BINARY binary markets and $NUM_CATEGORICAL_OUTCOMES categorical outcomes."

# --- 2. THRESHOLD ---
REQ_ICP=$(echo "$TOTAL_UNITS * $NUM_ORDERS_PER_SIDE * 2 * 1 * $WIGGLE_ROOM" | bc)
REQ_E8S=$(echo "$REQ_ICP * 100000000 / 1" | bc | cut -d'.' -f1)
echo "Required TEST_ICP: $REQ_ICP ($REQ_E8S e8s)"

# --- 3. BALANCE & FAUCET ---
echo "Checking balance..."
if ! BAL_RES=$(dfx canister call "$TEST_ICP_LEDGER" icrc1_balance_of "(record { owner = principal \"$MY_PRINCIPAL\" })" --network "$NETWORK" 2>/dev/null); then
  echo "Warning: Balance check failed."
  CUR_BAL_E8S=0
else
  CUR_BAL_E8S=$(echo "$BAL_RES" | grep -oE '[0-9_]+ : nat' | head -n1 | awk '{print $1}' | tr -d '_')
fi
echo "Current balance: $CUR_BAL_E8S e8s"

while [[ "$CUR_BAL_E8S" -lt "$REQ_E8S" ]]; do
  echo "Current balance ($CUR_BAL_E8S) is less than required ($REQ_E8S). Calling faucet..."
  dfx identity use default
  if [[ -n "$MY_ACCOUNT_ID" ]]; then
    dfx canister call "$FAUCET_CANISTER" transfer_icp "(\"$MY_ACCOUNT_ID\")" --network "$NETWORK"
  else
    echo "Error: Could not determine Account ID for $MY_PRINCIPAL. Falling back to principal..."
    dfx canister call "$FAUCET_CANISTER" transfer_icrc1 "(principal \"$MY_PRINCIPAL\")" --network "$NETWORK"
  fi
  dfx identity use "$MY_PRINCIPAL"

  echo "Waiting 5 seconds for balance to update..."
  sleep 5

  # Re-check balance
  if ! BAL_RES=$(dfx canister call "$TEST_ICP_LEDGER" icrc1_balance_of "(record { owner = principal \"$MY_PRINCIPAL\" })" --network "$NETWORK" 2>/dev/null); then
    echo "Warning: Balance check failed during retry loop."
  else
    CUR_BAL_E8S=$(echo "$BAL_RES" | grep -oE '[0-9_]+ : nat' | head -n1 | awk '{print $1}' | tr -d '_')
  fi
done

echo "Balance sufficient ($CUR_BAL_E8S e8s)."

# --- 4. PLACE ORDERS ---

# 4.1. Binary Markets
for SID in $BINARY_MARKETS; do
  echo "Processing Binary Market: $SID"
  # Binary is simpler - just Pick a random mid and spread
  MID_VAL=$(((RANDOM % (MID_MAX - MID_MIN + 1) + MID_MIN) * 10000))
  SPREAD_VAL=$(((RANDOM % (SPREAD_MAX - SPREAD_MIN + 1) + SPREAD_MIN) * 10000))
  place_outcome_orders "$SID" "null" "$MID_VAL" "$SPREAD_VAL"
done

# 4.2. Categorical Markets
CAT_SERIES_IDS=$(awk '{print $1}' <"$TMP_CAT_INFO" | sort -u)
for SID in $CAT_SERIES_IDS; do
  echo "Processing Categorical Market: $SID"
  OUTCOMES=$(grep "^$SID " <"$TMP_CAT_INFO" | awk '{print $2}')
  NUM_OUTCOMES=$(echo "$OUTCOMES" | wc -l | xargs)

  # Generate random weights
  WEIGHTS=()
  TOTAL_WEIGHT=0
  for ((i = 0; i < NUM_OUTCOMES; i++)); do
    W=$((RANDOM % 100 + 1))
    WEIGHTS+=("$W")
    TOTAL_WEIGHT=$((TOTAL_WEIGHT + W))
  done

  # Normalize so they sum to 1,000,000 (1.0 USD)
  MID_VALS=()
  CURRENT_SUM=0
  for ((i = 0; i < NUM_OUTCOMES; i++)); do
    M=$((WEIGHTS[i] * 1000000 / TOTAL_WEIGHT))
    # Ensure at least 0.05 per outcome
    [[ "$M" -lt 50000 ]] && M=50000
    MID_VALS+=("$M")
    CURRENT_SUM=$((CURRENT_SUM + M))
  done

  # Adjust the last one to perfectly sum to 1.0
  ADJUSTMENT=$((1000000 - CURRENT_SUM))
  MID_VALS[NUM_OUTCOMES - 1]=$((MID_VALS[NUM_OUTCOMES - 1] + ADJUSTMENT))

  # Pick a common spread range for all outcomes in this series
  SPREAD_VAL=$(((RANDOM % (SPREAD_MAX - SPREAD_MIN + 1) + SPREAD_MIN) * 10000))

  idx=0
  for OID in $OUTCOMES; do
    place_outcome_orders "$SID" "$OID" "${MID_VALS[$idx]}" "$SPREAD_VAL"
    idx=$((idx + 1))
  done
done

rm "$TMP_CAT_INFO"
echo "Finished."
