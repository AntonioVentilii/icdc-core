#!/bin/bash
set -euo pipefail

# Scenario 4: User A places a limit order, User B places an opposite limit order at the same price.
# This test verifies if the system supports automatic limit matching or if both stay in the book.

log "--- Scenario: Cross-Limit Placement ---"

TIMESTAMP=$(date +%s)
DEPOSIT_AMOUNT=1000000000 # 10 ICP

# 1. Setup Collateral
setup_icp_collateral "default" "$DEPOSIT_AMOUNT"
setup_icp_collateral "secondary" "$DEPOSIT_AMOUNT"

# 2. Register Series
log "Creating a new Binary Series..."
BINARY_SERIES_ID=$(register_series_usd "Cross-Limit Test" "variant { Binary }" "null" "CROSS_LIMIT_${TIMESTAMP}" | grep -oE '"[a-f0-9]{64}"' | head -n 1 | tr -d '"')

# 3. User A: Submit Limit Buy Order
log "User A: Placing Limit Buy order..."
ORDER_ID_A="LIMIT_BUY_${TIMESTAMP}"
dfx identity use default
dfx canister call clearing submit_limit_order "(record { 
    qty = 10 : int; 
    outcome_id = null; 
    series_id = \"$BINARY_SERIES_ID\"; 
    side = variant { Buy }; 
    order_id = \"$ORDER_ID_A\"; 
    price = record { decimal = record { value = 500000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

# 4. User B: Submit Limit Sell Order (Same Price)
log "User B: Placing Limit Sell order (Same Price)..."
ORDER_ID_B="LIMIT_SELL_${TIMESTAMP}"
dfx identity use secondary
dfx canister call clearing submit_limit_order "(record { 
    qty = 10 : int; 
    outcome_id = null; 
    series_id = \"$BINARY_SERIES_ID\"; 
    side = variant { Sell }; 
    order_id = \"$ORDER_ID_B\"; 
    price = record { decimal = record { value = 500000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

# 5. Verification
log "Verifying if orders matched automatically or remained in book..."
dfx identity use default
ORDERS=$(dfx canister call clearing get_orders)
POSITIONS=$(dfx canister call clearing get_user_positions)

if [[ "$POSITIONS" == *"series_id = \"$BINARY_SERIES_ID\""* ]]; then
  success "Automatic cross-limit matching occurred! Position created."
else
  warn "No automatic match. Both limit orders are likely in the book (Maker-only model)."
  if [[ "$ORDERS" == *"$ORDER_ID_A"* ]] && [[ "$ORDERS" == *"$ORDER_ID_B"* ]]; then
    success "Verified: Both crossing limit orders are sitting in the book as Makers."
  else
    error "Orders disappeared but no positions were created!"
  fi
fi

success "Cross-Limit Placement Scenario completed successfully!"
