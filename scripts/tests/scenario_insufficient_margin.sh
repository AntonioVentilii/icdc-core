#!/bin/bash
set -euo pipefail

# Scenario 5 & 6: Negative tests for margin validation.
# Verifies that users cannot place limit or market orders exceeding their collateral.

log "--- Scenario: Insufficient Margin Validation ---"

TIMESTAMP=$(date +%s)
SMALL_DEPOSIT=100000000 # 1 ICP
HUGE_QTY=1000           # 1000 units @ 0.5 price = 500 USD margin req (at 1.0 payoff)

# 1. Setup minimal collateral
log "User A: Setting up small collateral (1 ICP)..."
setup_icp_collateral "default" "$SMALL_DEPOSIT"

# 2. Register Series
log "Creating a Binary Series for margin tests..."
MARGIN_SERIES_ID=$(register_series_usd "Margin Test Binary" "variant { Binary }" "null" "MARGIN_NEG_${TIMESTAMP}" | grep -oE '"[a-f0-9]{64}"' | head -n 1 | tr -d '"')

# 3. Test Case 5: Limit Order Insufficient Margin
log "User A: Attempting to place HUGE limit order (Maker)..."
dfx identity use default
RES_LIMIT=$(dfx canister call clearing submit_limit_order "(record { 
    qty = $HUGE_QTY : int; 
    outcome_id = null; 
    series_id = \"$MARGIN_SERIES_ID\"; 
    side = variant { Buy }; 
    order_id = \"NEG_LIMIT_${TIMESTAMP}\"; 
    price = record { decimal = record { value = 500000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})")

if [[ "$HUGE_QTY" -gt 10 ]] && [[ "$RES_LIMIT" == *"InsufficientMargin"* ]]; then
  success "Limit order rejected as expected with InsufficientMargin error."
else
  error "Limit order should have been rejected! Response: $RES_LIMIT"
fi

# 4. Test Case 6: Market Order Insufficient Margin
log "User B: Setting up small collateral (1 ICP)..."
setup_icp_collateral "secondary" "$SMALL_DEPOSIT"

log "User A: Placing small valid limit order..."
VALID_ORDER_ID="MAKER_VALID_${TIMESTAMP}"
dfx identity use default
dfx canister call clearing submit_limit_order "(record { 
    qty = 5 : int; 
    outcome_id = null; 
    series_id = \"$MARGIN_SERIES_ID\"; 
    side = variant { Buy }; 
    order_id = \"$VALID_ORDER_ID\"; 
    price = record { decimal = record { value = 500000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

log "User B: User A just placed a buy order. User B will try to sell matching it, but let's assume B has no margin for it (needs more than 1 ICP)..."
# Since binary payoff is 1.0, 5 units @ 0.5 price needs 2.5 USD margin. 1 ICP is ~10 USD, so 5 units is actually valid.
# Let's try 100 units match.

HUGE_ORDER_ID="HUGE_MAKER_${TIMESTAMP}"
dfx identity use default
dfx canister call clearing submit_limit_order "(record { 
    qty = 200 : int; 
    outcome_id = null; 
    series_id = \"$MARGIN_SERIES_ID\"; 
    side = variant { Buy }; 
    order_id = \"$HUGE_ORDER_ID\"; 
    price = record { decimal = record { value = 500000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

log "User B: Attempting to match HUGE order (Taker)..."
dfx identity use secondary
RES_MARKET=$(dfx canister call clearing submit_market_order "(record { 
    trade_id = \"NEG_MARKET_${TIMESTAMP}\"; 
    matching_order_id = \"$HUGE_ORDER_ID\"; 
})")

if [[ "$RES_MARKET" == *"InsufficientMargin"* ]]; then
  success "Market order rejected as expected with InsufficientMargin error."
else
  error "Market order should have been rejected! Response: $RES_MARKET"
fi

success "Insufficient Margin Validation Scenario completed successfully!"
