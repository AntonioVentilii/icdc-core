#!/bin/bash
set -euo pipefail

# This scenario simulates a complete multi-user journey with limit order matching.
# It assumes global environment is already initialized.

log "--- Scenario: Multi-User Flow ---"

TIMESTAMP=$(date +%s)
DEPOSIT_AMOUNT=1000000000 # 10 ICP

# 1. Register Series
log "Creating a new Binary Series for flow test..."
BINARY_SERIES_ID=$(register_series_usd "Flow Test Binary" "variant { Binary }" "null" "FLOW_TEST_${TIMESTAMP}" | grep -oE '"[a-f0-9]{64}"' | head -n 1 | tr -d '"')

if [ -z "$BINARY_SERIES_ID" ]; then
  error "Failed to create Binary Series in Multi-User Flow"
fi
log "  Created Series: $BINARY_SERIES_ID"

# 2. Setup Collateral
setup_icp_collateral "default" "$DEPOSIT_AMOUNT"
setup_icp_collateral "secondary" "$DEPOSIT_AMOUNT"

# 3. User A: Placing orders
log "User A: Placing orders..."
ORDER_ID_A_BUY="A_BUY_${TIMESTAMP}"
dfx canister call clearing submit_limit_order "(record { 
    qty = 10 : int; 
    outcome_id = null; 
    series_id = \"$BINARY_SERIES_ID\"; 
    side = variant { Buy }; 
    order_id = \"$ORDER_ID_A_BUY\"; 
    price = record { decimal = record { value = 500000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

ORDER_ID_A_SELL="A_SELL_${TIMESTAMP}"
dfx canister call clearing submit_limit_order "(record { 
    qty = 10 : int; 
    outcome_id = null; 
    series_id = \"$BINARY_SERIES_ID\"; 
    side = variant { Sell }; 
    order_id = \"$ORDER_ID_A_SELL\"; 
    price = record { decimal = record { value = 600000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

# 4. User B: Matching User A's Sell Order
log "User B: Matching User A's Sell Order..."
ORDER_ID_B_BUY="B_BUY_${TIMESTAMP}"
dfx identity use secondary
dfx canister call clearing submit_limit_order "(record { 
    qty = 10 : int; 
    outcome_id = null; 
    series_id = \"$BINARY_SERIES_ID\"; 
    side = variant { Buy }; 
    order_id = \"$ORDER_ID_B_BUY\"; 
    price = record { decimal = record { value = 600000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

# 5. Verification
log "Verifying trade matching and positions..."
dfx identity use default
POSITIONS_A=$(dfx canister call clearing get_user_positions)
log "User A Positions: $POSITIONS_A"

dfx identity use secondary
POSITIONS_B=$(dfx canister call clearing get_user_positions)
log "User B Positions: $POSITIONS_B"

if [[ "$POSITIONS_A" == *"qty = -10"* ]] && [[ "$POSITIONS_B" == *"qty = 10"* ]]; then
  success "Trade matched successfully! A is short 10, B is long 10."
else
  error "Position mismatch. Expected A short 10 and B long 10."
fi

success "Multi-User Flow Scenario completed successfully!"
