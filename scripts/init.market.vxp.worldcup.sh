#!/usr/bin/env bash

# Seeds VXP (ViciXp domain) liquidity on active markets using a CURATED mid
# taken from a market deck's `consensus` value, instead of the random mid used
# by init.market.vxp.sh. Built for Vici's World Cup deck
# (vici-app/scripts/data/markets.deck-2026.json), where every row carries a
# 0-1 `consensus` probability.
#
# Usage:
#   ./scripts/init.market.vxp.worldcup.sh <markets-deck-json> [--local|--staging|--ic]
#   ./scripts/init.market.vxp.worldcup.sh ../vici-app/scripts/data/markets.deck-2026.json --local
#
# The deck is matched to on-chain series BY TITLE: a scalar market whose title
# appears in the deck is quoted around its consensus; markets absent from the
# deck (or categorical ones) fall back to the random ladder. Shared logic lives
# in init.market.common.sh.

# Capture the deck file (first positional) before utils.sh consumes "$@".
DECK_FILE="${1:-}"
if [[ -z "$DECK_FILE" || "$DECK_FILE" == -* || "$DECK_FILE" == "local" || "$DECK_FILE" == "staging" || "$DECK_FILE" == "ic" ]]; then
  echo "Usage: $0 <markets-deck-json> [--local|--staging|--ic]" >&2
  echo "  deck rows must carry a 0-1 \`consensus\`; see vici-app/scripts/data/markets.deck-2026.json" >&2
  exit 1
fi
shift

source "$(dirname "$0")/utils.sh" "$@"
source "$(dirname "$0")/init.common.sh"
source "$(dirname "$0")/init.market.common.sh"

if [[ ! -f "$DECK_FILE" ]]; then
  echo "Error: deck file '$DECK_FILE' not found." >&2
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "Error: jq is required to read the deck file." >&2
  exit 1
fi

# title -> consensus (0-1), loaded from the deck.
declare -A CONSENSUS_BY_TITLE

load_consensus_map() {
  local file=$1 c t
  while IFS=$'\t' read -r c t; do
    [[ -z "$t" ]] && continue
    CONSENSUS_BY_TITLE["$t"]="$c"
  done < <(jq -r '.[] | select(.consensus != null) | "\(.consensus)\t\(.title)"' "$file")
}

# Convert a 0-1 consensus probability to a mid in USD_DECIMALS minor units
# (0.18 => 1800 @ 4dp), rounded to the nearest unit.
consensus_to_mid() {
  awk -v c="$1" -v d="$USD_DECIMALS" 'BEGIN { printf "%.0f", c * (10 ^ d) }'
}

# Seed scalar markets with a consensus-derived mid, falling back to random when
# the market is not present in the deck.
place_scalar_orders_consensus() {
  local SID TITLE c MID_VAL SPREAD_VAL matched=0 missed=0
  for SID in $SCALAR_MARKETS; do
    TITLE=$(get_title "$SID")
    c=${CONSENSUS_BY_TITLE["$TITLE"]:-}
    if [[ -z "$c" ]]; then
      MID_VAL=$(random_mid_val)
      echo "Processing Scalar Market: $TITLE ($SID) — not in deck, random mid $MID_VAL"
      missed=$((missed + 1))
    else
      MID_VAL=$(consensus_to_mid "$c")
      echo "Processing Scalar Market: $TITLE ($SID) — consensus $c => mid $MID_VAL"
      matched=$((matched + 1))
    fi
    SPREAD_VAL=$(random_spread_val)
    place_outcome_orders "$SID" "null" "$MID_VAL" "$SPREAD_VAL"
  done
  echo "Consensus mids applied to $matched market(s); $missed fell back to random."
}

resolve_maker_identity

load_consensus_map "$DECK_FILE"
echo "Loaded ${#CONSENSUS_BY_TITLE[@]} consensus entries from $DECK_FILE."

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

# --- 5. PLACE ORDERS (consensus mid for scalar; random for categorical) ---
place_scalar_orders_consensus
place_categorical_orders_random

cleanup_market_tmps
echo "Finished."
