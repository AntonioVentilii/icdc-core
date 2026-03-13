#!/usr/bin/env bash

source "$(dirname "$0")/utils.sh" "$@"
source "$(dirname "$0")/init.common.sh"

# --- CONFIGURATION ---
NUM_ORDERS_PER_SIDE=${NUM_ORDERS_PER_SIDE:-3}
ORDER_VALUE_USD=${ORDER_VALUE_USD:-1}
MID_MIN=${MID_MIN:-10}
MID_MAX=${MID_MAX:-90}
SPREAD_MIN=${SPREAD_MIN:-2}
SPREAD_MAX=${SPREAD_MAX:-8}
WIGGLE_ROOM=${WIGGLE_ROOM:-1.3}

MY_IDENTITY=$(dfx identity whoami)
MY_PRINCIPAL=$(dfx identity get-principal)
MY_ACCOUNT_ID=$(dfx ledger account-id --of-principal "$MY_PRINCIPAL" 2>/dev/null || echo "")
echo "My Identity: $MY_IDENTITY"
echo "My Principal: $MY_PRINCIPAL"
echo "My Account ID: $MY_ACCOUNT_ID"

## --- 1. FETCH ACTIVE MARKETS FROM REGISTRY ---
echo "Fetching active markets from Registry..."
NOW_NS=$(date +%s%N)
# Fetch first 100 series (assuming for demo purposes this is enough, otherwise a loop would be needed)
ALL_SERIES=$(dfx canister call registry list_series "(record { limit = opt 100 : opt nat64; cursor = null })" --network "$NETWORK") || {
  echo "Failed to fetch series from Registry"
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
    dfx canister call clearing submit_limit_order "(record { qty = $QTY : int; outcome_id = $OARG; series_id = \"$SID\"; side = variant { Buy }; order_id = \"$OBID\"; price = record { decimal = record { value = $BID_VAL : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; })" --network "$NETWORK"
    dfx canister call clearing submit_limit_order "(record { qty = $QTY : int; outcome_id = $OARG; series_id = \"$SID\"; side = variant { Sell }; order_id = \"$OASK\"; price = record { decimal = record { value = $ASK_VAL : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; })" --network "$NETWORK"
  done
}

# --- PARSE AND FILTER ACTIVE MARKETS FROM REGISTRY ---
# Use "record {" as the separator to ensure title and series_id stay together
# Use parameter expansion to insert record boundaries for parsing
PARSABLE_DATA="${ALL_SERIES//record \{/$'\n---RECORD---\n'}"

TMP_TITLES=$(mktemp)
TMP_SCALAR=$(mktemp)
TMP_CAT_INFO=$(mktemp)

echo "$PARSABLE_DATA" | awk -v now="$NOW_NS" '
BEGIN { RS="---RECORD---"; FS="\n"; sid=""; current_payoff=""; }
{
    # Update sid ONLY if found in this record
    if (match($0, /series_id = "[^"]+"/)) {
        temp = substr($0, RSTART, RLENGTH);
        sub(/series_id = "/, "", temp);
        sub(/"/, "", temp);
        sid = temp;
        # Reset payoff when we find a new series_id
        current_payoff = "";
    }

    # title extraction: find title = "..."
    if (match($0, /title = "[^"]+"/)) {
        temp = substr($0, RSTART, RLENGTH);
        sub(/title = "/, "", temp);
        sub(/"/, "", temp);
        title = temp;
    } else title = "";

    # expiry extraction: find expiry_ns = ...
    if (match($0, /expiry_ns = [0-9_]+/)) {
        temp = substr($0, RSTART, RLENGTH);
        sub(/expiry_ns = /, "", temp);
        gsub("_", "", temp);
        expiry = temp;
    } else expiry = 0;

    # payoff_type extraction: find variant { X }
    if (match($0, /payoff_type = variant \{ [^}]+ \}/)) {
        temp = substr($0, RSTART, RLENGTH);
        sub(/payoff_type = variant \{ /, "", temp);
        sub(/ \}/, "", temp);
        gsub(/^[ \t]+|[ \t]+$/, "", temp); # Trim
        current_payoff = temp;
    }

    if (sid != "" && (expiry > now || expiry == 0)) {
        if (title != "") print sid "|" title > "'"$TMP_TITLES"'"
        
        if (current_payoff ~ /^(Binary|Call|Put)$/) {
            print sid > "'"$TMP_SCALAR"'"
        } else if (current_payoff == "Categorical") {
            # id extraction (for outcomes) - look for " id =" (with space) to distinguish from series_id
            outcomes_part = $0;
            while (match(outcomes_part, / id = "[^"]+"/)) {
                temp = substr(outcomes_part, RSTART, RLENGTH);
                sub(/ id = "/, "", temp);
                sub(/"/, "", temp);
                oid = temp;
                print sid, oid > "'"$TMP_CAT_INFO"'"
                outcomes_part = substr(outcomes_part, RSTART + RLENGTH);
            }
        }
    }
}
'

get_title() {
  local SID=$1
  grep "^$SID|" "$TMP_TITLES" | head -n1 | cut -d'|' -f2- || echo "Unknown Market"
}

# --- 1.2. COUNT AND VALIDATE ---
NUM_SCALAR=$(wc -l <"$TMP_SCALAR" | tr -d '[:space:]')
NUM_CATEGORICAL_OUTCOMES=$(wc -l <"$TMP_CAT_INFO" | tr -d '[:space:]')
TOTAL_UNITS=$((NUM_SCALAR + NUM_CATEGORICAL_OUTCOMES))

if [[ "$TOTAL_UNITS" -eq 0 ]]; then
  echo "No active markets found in Registry."
  rm "$TMP_CAT_INFO" "$TMP_TITLES" "$TMP_SCALAR"
  exit 0
fi

echo "Found $NUM_SCALAR scalar markets and $NUM_CATEGORICAL_OUTCOMES categorical outcomes."

SCALAR_MARKETS=$(sort -u "$TMP_SCALAR" 2>/dev/null || true)

# --- 2. THRESHOLD ---
# Formula: NUM_MARKETS * ORDERS_PER_SIDE * 2_SIDES * UNITS_PER_ORDER * WIGGLE_ROOM
REQ_ICP=$(echo "$TOTAL_UNITS * $NUM_ORDERS_PER_SIDE * 2 * $ORDER_VALUE_USD * $WIGGLE_ROOM" | bc)
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
  dfx identity use "$MY_IDENTITY"

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

# --- 4. DEPOSIT COLLATERAL TO CLEARING ---
echo "Depositing collateral to Clearing..."
DID=$(openssl rand -hex 8)

# Deduct ledger fees: one for icrc2_approve and one for icrc2_transfer_from
# Assume 10,000 e8s fee per call (standard for ICRC-1 ledgers)
LEDGER_FEE=$DEFAULT_LEDGER_FEE
APPROVE_AMOUNT=$((CUR_BAL_E8S - LEDGER_FEE))
DEPOSIT_AMOUNT=$((CUR_BAL_E8S - 2 * LEDGER_FEE))

[[ "$APPROVE_AMOUNT" -lt 0 ]] && APPROVE_AMOUNT=0
[[ "$DEPOSIT_AMOUNT" -lt 0 ]] && DEPOSIT_AMOUNT=0

echo "  Approving Clearing to spend $APPROVE_AMOUNT e8s of TESTICP..."
dfx canister call "$TEST_ICP_LEDGER" icrc2_approve "(record { 
    amount = $APPROVE_AMOUNT : nat; 
    spender = record { owner = principal \"$CLEARING_CANISTER\" };
})" --network "$NETWORK"

echo "  Executing deposit_collateral on Clearing..."
dfx canister call clearing deposit_collateral "(record { 
    amount = $DEPOSIT_AMOUNT : nat; 
    asset_id = \"$TESTICP_SYMBOL\"; 
    deposit_id = \"$DID\"; 
    domain = opt variant { Settlement };
})" --network "$NETWORK"

# --- 5. PLACE ORDERS ---

# 4.1. Scalar Markets (Binary, Call, Put)
for SID in $SCALAR_MARKETS; do
  TITLE=$(get_title "$SID")
  echo "Processing Scalar Market: $TITLE ($SID)"
  # Pick a random mid and spread
  MID_VAL=$(((RANDOM % (MID_MAX - MID_MIN + 1) + MID_MIN) * 10000))
  SPREAD_VAL=$(((RANDOM % (SPREAD_MAX - SPREAD_MIN + 1) + SPREAD_MIN) * 10000))
  place_outcome_orders "$SID" "null" "$MID_VAL" "$SPREAD_VAL"
done

# 4.2. Categorical Markets
CAT_SERIES_IDS=$(awk '{print $1}' <"$TMP_CAT_INFO" | sort -u)
for SID in $CAT_SERIES_IDS; do
  TITLE=$(get_title "$SID")
  echo "Processing Categorical Market: $TITLE ($SID)"
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

  # Normalize so they sum to 1.0 USD
  MID_VALS=()
  CURRENT_SUM=0
  DECIMAL_FACTOR=$((10 ** USD_DECIMALS))
  for ((i = 0; i < NUM_OUTCOMES; i++)); do
    M=$((WEIGHTS[i] * DECIMAL_FACTOR / TOTAL_WEIGHT))
    # Ensure at least 0.05 per outcome
    [[ "$M" -lt $((5 * DECIMAL_FACTOR / 100)) ]] && M=$((5 * DECIMAL_FACTOR / 100))
    MID_VALS+=("$M")
    CURRENT_SUM=$((CURRENT_SUM + M))
  done

  # Adjust the last one to perfectly sum to 1.0
  ADJUSTMENT=$((DECIMAL_FACTOR - CURRENT_SUM))
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
rm "$TMP_TITLES"
rm "$TMP_SCALAR"
echo "Finished."
