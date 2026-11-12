#!/usr/bin/env bash

# Shared helpers for the init.market.*.sh liquidity seeders.
#
# Source AFTER scripts/utils.sh (sets NETWORK) and scripts/init.common.sh
# (sets REGISTRY_CANISTER, CLEARING_CANISTER, USD_DECIMALS, token configs).
#
# These seeders act as a single market-maker principal: they fetch the active
# series from the registry, deposit collateral into clearing, then post a
# two-sided limit-order ladder on each market. The only thing that varies
# between seeders is the collateral token (VXP vs ICP/TESTICP/...) and how the
# per-market mid price is chosen (random vs. a curated `consensus`).

# --- ORDER / LADDER CONFIGURATION (env-overridable) ---
NUM_ORDERS_PER_SIDE=${NUM_ORDERS_PER_SIDE:-3}
ORDER_VALUE_USD=${ORDER_VALUE_USD:-1}
# `qty` in `clearing.submit_limit_order` is the order quantity (contracts/units),
# not the collateral amount. Collateral is deposited separately as margin.
ORDER_QTY=${ORDER_QTY:-500}
MID_MIN=${MID_MIN:-10}
MID_MAX=${MID_MAX:-90}
SPREAD_MIN=${SPREAD_MIN:-2}
SPREAD_MAX=${SPREAD_MAX:-8}
WIGGLE_ROOM=${WIGGLE_ROOM:-1.3}

# Resolve and echo the calling dfx identity. Sets MY_IDENTITY / MY_PRINCIPAL /
# MY_ACCOUNT_ID as globals for the caller.
resolve_maker_identity() {
  MY_IDENTITY=$(dfx identity whoami)
  MY_PRINCIPAL=$(dfx identity get-principal)
  MY_ACCOUNT_ID=$(dfx ledger account-id --of-principal "$MY_PRINCIPAL" 2>/dev/null || echo "")
  echo "My Identity: $MY_IDENTITY"
  echo "My Principal: $MY_PRINCIPAL"
  echo "My Account ID: $MY_ACCOUNT_ID"
}

# Place a ladder of NUM_ORDERS_PER_SIDE buy + sell limit orders around MID_VAL.
# Usage: place_outcome_orders SERIES_ID OUTCOME_ID MID_VAL SPREAD_VAL
#   OUTCOME_ID is "null" for scalar (Binary/Call/Put) markets, else the id.
#   MID_VAL / SPREAD_VAL are in USD_DECIMALS minor units (0.5 => 5000 @ 4dp).
place_outcome_orders() {
  local SID=$1
  local OID=$2
  local MID_VAL=$3
  local SPREAD_VAL=$4

  echo "    Placing orders for Outcome: ${OID:-Binary} (Mid: $MID_VAL, Spread: $SPREAD_VAL)"

  # 1% steps in the same fixed-point units as `MID_VAL`.
  # With USD_DECIMALS=4, 1.0% = 0.01 USD => 0.01 * 10^4 = 100 minor units.
  local STEP_1P=$((10 ** (USD_DECIMALS - 2)))

  # We interpret `SPREAD_VAL` as the OUTER offset from `MID_VAL`.
  # For 3 levels per side and a spread of X, offsets become: (X-2%), (X-1%), (X-0%).
  for i in $(seq 1 "$NUM_ORDERS_PER_SIDE"); do
    local OFFSET=$((SPREAD_VAL - (NUM_ORDERS_PER_SIDE - i) * STEP_1P))
    [[ "$OFFSET" -lt 0 ]] && OFFSET=0

    local BID_VAL=$((MID_VAL - OFFSET))
    local ASK_VAL=$((MID_VAL + OFFSET))

    # Clamp BOTH sides into the [0.01, 0.99] tick range (in USD_DECIMALS minor
    # units). A consensus-derived mid near 0 or 1 can otherwise push a bid above
    # MAX_TICK or an ask below MIN_TICK, which submit_limit_order rejects and
    # (under set -e) would abort the run.
    local MIN_TICK=$((10 ** (USD_DECIMALS - 2)))
    local MAX_TICK=$((99 * (10 ** (USD_DECIMALS - 2))))
    [[ "$BID_VAL" -lt "$MIN_TICK" ]] && BID_VAL=$MIN_TICK
    [[ "$BID_VAL" -gt "$MAX_TICK" ]] && BID_VAL=$MAX_TICK
    [[ "$ASK_VAL" -lt "$MIN_TICK" ]] && ASK_VAL=$MIN_TICK
    [[ "$ASK_VAL" -gt "$MAX_TICK" ]] && ASK_VAL=$MAX_TICK

    local OBID
    OBID=$(openssl rand -hex 8)
    local OASK
    OASK=$(openssl rand -hex 8)
    local QTY=$ORDER_QTY

    local OARG="$OID"
    if [[ "$OID" != "null" ]]; then OARG="opt \"$OID\""; fi

    echo "      Level $i: Buy @ $BID_VAL, Sell @ $ASK_VAL"
    dfx canister call clearing submit_limit_order "(record { qty = $QTY : int; outcome_id = $OARG; series_id = \"$SID\"; side = variant { Buy }; order_id = \"$OBID\"; price = record { decimal = record { value = $BID_VAL : nat; decimals = $USD_DECIMALS : nat8 }; oracle_id = null; timestamp = null }; })" --network "$NETWORK"
    dfx canister call clearing submit_limit_order "(record { qty = $QTY : int; outcome_id = $OARG; series_id = \"$SID\"; side = variant { Sell }; order_id = \"$OASK\"; price = record { decimal = record { value = $ASK_VAL : nat; decimals = $USD_DECIMALS : nat8 }; oracle_id = null; timestamp = null }; })" --network "$NETWORK"
  done
}

# Fetch the first 100 active series and parse them into temp files. Sets globals:
#   TMP_TITLES TMP_SCALAR TMP_CAT_INFO  (temp file paths)
#   NUM_SCALAR NUM_CATEGORICAL_OUTCOMES TOTAL_UNITS
#   SCALAR_MARKETS  (newline-separated series ids of Binary/Call/Put markets)
#   CAT_SERIES_IDS  (newline-separated series ids of Categorical markets)
# Returns 1 (after cleanup) when there are no active markets.
fetch_and_parse_markets() {
  echo "Fetching active markets from Registry..."
  local NOW_NS
  NOW_NS=$(date +%s%N)

  local ALL_SERIES
  ALL_SERIES=$(dfx canister call registry list_series "(record { limit = opt 100 : opt nat64; cursor = null })" --network "$NETWORK") || {
    echo "Failed to fetch series from Registry"
    exit 1
  }

  # Insert record boundaries so awk can keep title/series_id/payoff together.
  local PARSABLE_DATA="${ALL_SERIES//record \{/$'\n---RECORD---\n'}"

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

  NUM_SCALAR=$(wc -l <"$TMP_SCALAR" | tr -d '[:space:]')
  NUM_CATEGORICAL_OUTCOMES=$(wc -l <"$TMP_CAT_INFO" | tr -d '[:space:]')
  TOTAL_UNITS=$((NUM_SCALAR + NUM_CATEGORICAL_OUTCOMES))

  if [[ "$TOTAL_UNITS" -eq 0 ]]; then
    echo "No active markets found in Registry."
    cleanup_market_tmps
    return 1
  fi

  echo "Found $NUM_SCALAR scalar markets and $NUM_CATEGORICAL_OUTCOMES categorical outcomes."

  SCALAR_MARKETS=$(sort -u "$TMP_SCALAR" 2>/dev/null || true)
  CAT_SERIES_IDS=$(awk '{print $1}' <"$TMP_CAT_INFO" | sort -u)
}

# Look up a market title by series id (from the parsed TMP_TITLES).
get_title() {
  local SID=$1
  grep "^$SID|" "$TMP_TITLES" | head -n1 | cut -d'|' -f2- || echo "Unknown Market"
}

cleanup_market_tmps() {
  rm -f "$TMP_CAT_INFO" "$TMP_TITLES" "$TMP_SCALAR"
}

# Echo the collateral required (in ledger base units) to back every ladder.
# Usage: compute_required_base_units DECIMALS
# Usage: compute_required_base_units DECIMALS [UNITS]
#   UNITS defaults to TOTAL_UNITS (all active markets); pass a smaller count when
#   only seeding a subset (e.g. deck-only) so the collateral isn't over-sized.
compute_required_base_units() {
  local decimals=$1
  local units=${2:-$TOTAL_UNITS}
  # For binary/categorical products total system margin per contract is ~1 USD,
  # so we scale by ORDER_QTY (contracts/units per order) across all ladders.
  local req_tokens
  req_tokens=$(echo "$units * $NUM_ORDERS_PER_SIDE * 2 * $ORDER_QTY * $ORDER_VALUE_USD * $WIGGLE_ROOM" | bc)
  echo "scale=0; $req_tokens * (10^$decimals) / 1" | bc | cut -d'.' -f1
}

# Echo a principal's ICRC-1 balance in base units (0 on failure).
read_ledger_balance() {
  local ledger=$1
  local principal=$2
  local res
  if ! res=$(dfx canister call "$ledger" icrc1_balance_of "(record { owner = principal \"$principal\" })" --network "$NETWORK" 2>/dev/null); then
    echo "Warning: balance query failed for ledger $ledger; treating balance as 0." >&2
    echo 0
    return
  fi
  echo "$res" | grep -oE '[0-9_]+ : nat' | head -n1 | awk '{print $1}' | tr -d '_'
}

# icrc2_approve + deposit_collateral of (almost) the whole balance as margin.
# Usage: deposit_all_collateral SYMBOL LEDGER FEE DOMAIN_CANDID CUR_BAL_BASE
#   DOMAIN_CANDID e.g. 'opt variant { ViciXp }'
deposit_all_collateral() {
  local symbol=$1
  local ledger=$2
  local fee=$3
  local domain=$4
  local cur_bal_base=$5

  echo "Depositing collateral to Clearing..."
  local did
  did=$(openssl rand -hex 8)

  # Deduct ledger fees: one for icrc2_approve and one for icrc2_transfer_from.
  local approve_amount=$((cur_bal_base - fee))
  local deposit_amount=$((cur_bal_base - 2 * fee))
  [[ "$approve_amount" -lt 0 ]] && approve_amount=0
  [[ "$deposit_amount" -lt 0 ]] && deposit_amount=0

  echo "  Approving Clearing to spend $approve_amount base units of $symbol..."
  dfx canister call "$ledger" icrc2_approve "(record {
    amount = $approve_amount : nat;
    spender = record { owner = principal \"$CLEARING_CANISTER\" };
})" --network "$NETWORK"

  echo "  Executing deposit_collateral on Clearing..."
  dfx canister call clearing deposit_collateral "(record {
    amount = $deposit_amount : nat;
    asset_id = \"$symbol\";
    deposit_id = \"$did\";
    domain = $domain;
})" --network "$NETWORK"
}

# Echo a random mid in USD_DECIMALS minor units, in [MID_MIN, MID_MAX] percent.
random_mid_val() {
  echo $(((RANDOM % (MID_MAX - MID_MIN + 1) + MID_MIN) * (10 ** (USD_DECIMALS - 2))))
}

# Echo a random outer spread in USD_DECIMALS minor units, in [SPREAD_MIN, SPREAD_MAX] percent.
random_spread_val() {
  echo $(((RANDOM % (SPREAD_MAX - SPREAD_MIN + 1) + SPREAD_MIN) * (10 ** (USD_DECIMALS - 2))))
}

# Seed every scalar (Binary/Call/Put) market with a random mid + spread ladder.
place_scalar_orders_random() {
  local SID TITLE MID_VAL SPREAD_VAL
  for SID in $SCALAR_MARKETS; do
    TITLE=$(get_title "$SID")
    echo "Processing Scalar Market: $TITLE ($SID)"
    MID_VAL=$(random_mid_val)
    SPREAD_VAL=$(random_spread_val)
    place_outcome_orders "$SID" "null" "$MID_VAL" "$SPREAD_VAL"
  done
}

# Seed every categorical market with random per-outcome weights that sum to 1.0.
place_categorical_orders_random() {
  local SID TITLE OUTCOMES NUM_OUTCOMES SPREAD_VAL idx OID
  for SID in $CAT_SERIES_IDS; do
    TITLE=$(get_title "$SID")
    echo "Processing Categorical Market: $TITLE ($SID)"
    OUTCOMES=$(grep "^$SID " <"$TMP_CAT_INFO" | awk '{print $2}')
    NUM_OUTCOMES=$(echo "$OUTCOMES" | wc -l | xargs)

    # Generate random weights
    local WEIGHTS=()
    local TOTAL_WEIGHT=0
    local i W
    for ((i = 0; i < NUM_OUTCOMES; i++)); do
      W=$((RANDOM % 100 + 1))
      WEIGHTS+=("$W")
      TOTAL_WEIGHT=$((TOTAL_WEIGHT + W))
    done

    # Normalize so they sum to 1.0 USD
    local MID_VALS=()
    local CURRENT_SUM=0
    local DECIMAL_FACTOR=$((10 ** USD_DECIMALS))
    local M
    for ((i = 0; i < NUM_OUTCOMES; i++)); do
      M=$((WEIGHTS[i] * DECIMAL_FACTOR / TOTAL_WEIGHT))
      # Ensure at least 0.05 per outcome
      [[ "$M" -lt $((5 * DECIMAL_FACTOR / 100)) ]] && M=$((5 * DECIMAL_FACTOR / 100))
      MID_VALS+=("$M")
      CURRENT_SUM=$((CURRENT_SUM + M))
    done

    # Adjust the last one to perfectly sum to 1.0
    local ADJUSTMENT=$((DECIMAL_FACTOR - CURRENT_SUM))
    MID_VALS[NUM_OUTCOMES - 1]=$((MID_VALS[NUM_OUTCOMES - 1] + ADJUSTMENT))

    # Pick a common spread for all outcomes in this series
    SPREAD_VAL=$(random_spread_val)

    idx=0
    for OID in $OUTCOMES; do
      place_outcome_orders "$SID" "$OID" "${MID_VALS[$idx]}" "$SPREAD_VAL"
      idx=$((idx + 1))
    done
  done
}
