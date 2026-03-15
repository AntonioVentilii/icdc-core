#!/bin/bash
set -euo pipefail

# Scenario 3: User A places a limit order, User B places a market order to match it.

log "--- Scenario: Limit vs Market Match ---"

TIMESTAMP=$(date +%s)
DEPOSIT_AMOUNT=1000000000 # 10 ICP

# 1. Setup Collateral
setup_icp_collateral "default" "$DEPOSIT_AMOUNT"
setup_icp_collateral "secondary" "$DEPOSIT_AMOUNT"

# 2. Register Series
log "Creating a new Binary Series..."
BINARY_SERIES_ID=$(register_series_usd "Limit/Market Test" "variant { Binary }" "null" "LM_MATCH_${TIMESTAMP}" | grep -oE '"[a-f0-9]{64}"' | head -n 1 | tr -d '"')

# 3. User A: Submit Limit Order (Maker)
log "User A: Placing Limit Buy order (Maker)..."
ORDER_ID_A="MAKER_LIMIT_${TIMESTAMP}"
dfx identity use default
dfx canister call clearing submit_limit_order "(record { 
    qty = 5 : int; 
    outcome_id = null; 
    series_id = \"$BINARY_SERIES_ID\"; 
    side = variant { Buy }; 
    order_id = \"$ORDER_ID_A\"; 
    price = record { decimal = record { value = 500000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

# 4. User B: Submit Market Order (Taker)
log "User B: Placing Market Sell order (Taker) matching User A..."
TRADE_ID_B="TAKER_TRADE_${TIMESTAMP}"
dfx identity use secondary
dfx canister call clearing submit_market_order "(record { 
    trade_id = \"$TRADE_ID_B\"; 
    matching_order_id = \"$ORDER_ID_A\"; 
})" >/dev/null

# 5. Verification
log "Verifying trade execution and positions..."
dfx identity use default
POSITIONS_A=$(dfx canister call clearing get_user_positions)
log "User A Positions: $POSITIONS_A"

dfx identity use secondary
POSITIONS_B=$(dfx canister call clearing get_user_positions)
log "User B Positions: $POSITIONS_B"

if [[ "$POSITIONS_A" == *"qty = 5"* ]] && [[ "$POSITIONS_B" == *"qty = -5"* ]]; then
  success "Market order matched against limit order successfully!"
else
  error "Position mismatch. Expected A long 5 and B short 5."
fi

success "Limit vs Market Match Scenario completed successfully!"
