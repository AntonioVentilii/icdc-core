#!/bin/bash
set -euo pipefail

# This scenario focuses on basic matched trades for Binary and Put options.
# It assumes global environment is already initialized.

log "--- Scenario: Basic Matched Trades ---"

TIMESTAMP=$(date +%s)
DEPOSIT_AMOUNT=2000000000 # 20 ICP

# 1. Setup Collateral
setup_icp_collateral "default" "$DEPOSIT_AMOUNT"
setup_icp_collateral "secondary" "$DEPOSIT_AMOUNT"

# 2. Register Series
log "Registering Binary and Put Series..."
BINARY_SERIES=$(register_series_usd "Binary Test" "variant { Binary }" "null" "BASIC_BIN_${TIMESTAMP}" | grep -oE '"[a-f0-9]{64}"' | head -n 1 | tr -d '"')
PUT_SERIES=$(register_series_usd "Put Test" "variant { Put }" "opt record { decimal = record { value = 1000000; decimals = 6 } }" "BASIC_PUT_${TIMESTAMP}" | grep -oE '"[a-f0-9]{64}"' | head -n 1 | tr -d '"')

if [ -z "$BINARY_SERIES" ] || [ -z "$PUT_SERIES" ]; then
  error "Failed to register series in Basic Matched Scenario"
fi

# 3. Execution: Binary Scenario
log "Matching Binary Trade (10 @ 0.55)..."
dfx identity use default
dfx canister call clearing submit_matched_trade "(record { 
    trade_id = \"T1_${TIMESTAMP}\"; 
    series_id = \"$BINARY_SERIES\"; 
    buyer = principal \"$PRINCIPAL_A\"; 
    seller = principal \"$PRINCIPAL_B\"; 
    qty = 10 : int; 
    price = record { decimal = record { value = 550000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

# 4. Execution: Put Scenario
log "Matching Put Trade (5 @ 0.20)..."
dfx canister call clearing submit_matched_trade "(record { 
    trade_id = \"T2_${TIMESTAMP}\"; 
    series_id = \"$PUT_SERIES\"; 
    buyer = principal \"$PRINCIPAL_B\"; 
    seller = principal \"$PRINCIPAL_A\"; 
    qty = 5 : int; 
    price = record { decimal = record { value = 200000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

# 5. Settlement
log "Settling series..."
dfx canister call clearing settle_series "(record { 
    series_id = \"$BINARY_SERIES\"; 
    settlement_price = record { decimal = record { value = 1000000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

dfx canister call clearing settle_series "(record { 
    series_id = \"$PUT_SERIES\"; 
    settlement_price = record { decimal = record { value = 800000 : nat; decimals = 6 : nat8 }; oracle_id = null; timestamp = null }; 
})" >/dev/null

# 6. Verification
log "Verifying final balances..."
BAL_A=$(get_usd_balance "default")
BAL_B=$(get_usd_balance "secondary")

# Expected balances based on pnl breakdown in original test.integration.sh
# Default: -515,000
# Secondary: -4,501,500
assert_eq "$BAL_A" "-515000" "User A cash_balance_usd"
assert_eq "$BAL_B" "-4501500" "User B cash_balance_usd"

success "Basic Matched Trades Scenario completed successfully!"
