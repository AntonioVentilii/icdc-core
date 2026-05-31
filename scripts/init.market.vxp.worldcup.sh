#!/usr/bin/env bash

# Seeds VXP (ViciXp domain) liquidity on the markets in a curated deck, quoting
# each around its `consensus` value (0-1) instead of the random mid used by
# init.market.vxp.sh. Built for Vici's World Cup deck
# (vici-app/scripts/data/markets.deck-2026.json).
#
# DECK-ONLY: only scalar markets whose title appears in the deck are seeded.
# Every other registry market (and all categorical markets) is left untouched,
# and the required collateral is sized to just the matched markets. Use
# init.market.vxp.sh if you want to seed the whole registry with random mids.
#
# Usage:
#   ./scripts/init.market.vxp.worldcup.sh <markets-deck-json> [--local|--staging|--ic]
#   ./scripts/init.market.vxp.worldcup.sh ../vici-app/scripts/data/markets.deck-2026.json --local
#
# Shared logic lives in init.market.common.sh.

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

resolve_maker_identity

load_consensus_map "$DECK_FILE"
echo "Loaded ${#CONSENSUS_BY_TITLE[@]} consensus entries from $DECK_FILE."

# --- 1. FETCH + PARSE ACTIVE MARKETS ---
fetch_and_parse_markets || exit 0

# --- 2. SELECT DECK MARKETS (scalar, matched by title) ---
MATCHED_SIDS=()
for SID in $SCALAR_MARKETS; do
  TITLE=$(get_title "$SID")
  [[ -n "${CONSENSUS_BY_TITLE["$TITLE"]:-}" ]] && MATCHED_SIDS+=("$SID")
done
NUM_MATCHED=${#MATCHED_SIDS[@]}
echo "Matched $NUM_MATCHED of $NUM_SCALAR scalar markets to the deck (deck-only; other markets left untouched)."

if [[ "$NUM_MATCHED" -eq 0 ]]; then
  echo "No deck markets present in the registry — nothing to seed. (Deploy the deck first.)"
  cleanup_market_tmps
  exit 0
fi

# --- 3. REQUIRED COLLATERAL (sized to the matched markets only) ---
REQ_BASE_UNITS=$(compute_required_base_units "$VICI_XP_DECIMALS" "$NUM_MATCHED")
echo "Required $VICI_XP_SYMBOL: $REQ_BASE_UNITS ledger base units"

# --- 4. BALANCE CHECK (no faucet for VXP) ---
echo "Checking balance..."
CUR_BAL_BASE=$(read_ledger_balance "$VICI_XP_LEDGER" "$MY_PRINCIPAL")
echo "Current balance: $CUR_BAL_BASE base units"

if [[ "$CUR_BAL_BASE" -lt "$REQ_BASE_UNITS" ]]; then
  echo "Error: Current balance ($CUR_BAL_BASE base units) is less than required ($REQ_BASE_UNITS base units). Please ensure you have sufficient $VICI_XP_SYMBOL tokens."
  cleanup_market_tmps
  exit 1
fi
echo "Balance sufficient ($CUR_BAL_BASE base units)."

# --- 5. DEPOSIT COLLATERAL ---
deposit_all_collateral "$VICI_XP_SYMBOL" "$VICI_XP_LEDGER" "$VICI_XP_TRANSFER_FEE" "opt variant { ViciXp }" "$CUR_BAL_BASE"

# --- 6. PLACE ORDERS (consensus mid, deck markets only) ---
for SID in "${MATCHED_SIDS[@]}"; do
  TITLE=$(get_title "$SID")
  CONS=${CONSENSUS_BY_TITLE["$TITLE"]}
  MID_VAL=$(consensus_to_mid "$CONS")
  SPREAD_VAL=$(random_spread_val)
  echo "Processing Scalar Market: $TITLE ($SID) — consensus $CONS => mid $MID_VAL"
  place_outcome_orders "$SID" "null" "$MID_VAL" "$SPREAD_VAL"
done

cleanup_market_tmps
echo "Finished."
