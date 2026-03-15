#!/bin/bash
set -euo pipefail

# Scenario 2: Single user deposits collateral and places two non-matching limit orders.
# One Buy and one Sell on the same series.

log "--- Scenario: Limit Buy and Sell (Single User) ---"

TIMESTAMP=$(date +%s)
DEPOSIT_AMOUNT=1000000000 # 10 ICP

# 1. Setup Collateral
setup_icp_collateral "default" "$DEPOSIT_AMOUNT"

# 2. Register Series
log "Creating a new Binary Series..."
BINARY_SERIES_ID=$(register_series_usd "Buy/Sell Test" "variant { Binary }" "null" "SINGLE_BS_${TIMESTAMP}" | grep -oE '"[a-f0-9]{64}"' | head -n 1 | tr -d '"')

if [ -z "$BINARY_SERIES_ID" ]; then
  error "Failed to create Binary Series in single user test"
fi

# 3. Place Buy Order
log "User A: Placing Buy limit order..."
ORDER_ID_A_BUY="A_BUY_${TIMESTAMP}"
dfx identity use default
dfx canister call clearing submit_limit_order "(record { 
    qty = 10 : int; 
    outcome_id = null; 
    series_id = \"$BINARY_SERIES_ID\"; 
    side = variant { Buy }; 
    order_id = \"$ORDER_ID_A_BUY\"; 
    price = record { decimal = record { value = 400000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

# 4. Place Sell Order
log "User A: Placing Sell limit order (higher price)..."
ORDER_ID_A_SELL="A_SELL_${TIMESTAMP}"
dfx canister call clearing submit_limit_order "(record { 
    qty = 5 : int; 
    outcome_id = null; 
    series_id = \"$BINARY_SERIES_ID\"; 
    side = variant { Sell }; 
    order_id = \"$ORDER_ID_A_SELL\"; 
    price = record { decimal = record { value = 600000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

# 5. Verification
log "Verifying orders are present and no matches occurred..."
ORDERS=$(dfx canister call clearing get_orders)
log "Active Orders: $ORDERS"

if [[ "$ORDERS" == *"$ORDER_ID_A_BUY"* ]] && [[ "$ORDERS" == *"$ORDER_ID_A_SELL"* ]]; then
  success "Both Buy and Sell limit orders are active."
else
  error "Orders not found in book."
fi

POSITIONS=$(dfx canister call clearing get_user_positions)
if [[ "$POSITIONS" == *"series_id = \"$BINARY_SERIES_ID\""* ]]; then
  error "Position found prematurely. Trade should not have matched."
else
  success "No positions created, as expected."
fi

success "Limit Buy and Sell Scenario completed successfully!"
