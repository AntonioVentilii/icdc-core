#!/bin/bash
set -euo pipefail

# Sourcing centralized utilities
source ./scripts/test.integration.helper.sh

# --- CONFIGURATION ---
DEPOSIT_AMOUNT=2000000000 # 20 ICP
TIMESTAMP=$(date +%s)

# Canister IDs
CLEARING=$(dfx canister id clearing 2>/dev/null) || error "Clearing canister not found. Did you deploy?"
REGISTRY=$(dfx canister id registry 2>/dev/null) || error "Registry canister not found. Did you deploy?"
ICP_LEDGER=$(dfx canister id icp_ledger 2>/dev/null) || error "ICP Ledger canister not found. Did you deploy?"

log "Starting Production-Ready Integration Suite..."
log "Canisters: Clearing=$CLEARING, Registry=$REGISTRY, Ledger=$ICP_LEDGER"

# --- 1. INITIALIZATION ---
log "Initializing clearing config..."
dfx canister call clearing set_registry_canister "(principal \"$REGISTRY\")" >/dev/null

# Get principals for config
dfx identity use default
PRINCIPAL="$(dfx identity get-principal)"
dfx identity get-principal --identity secondary &>/dev/null || dfx identity new secondary --storage-mode=plaintext
SECONDARY="$(dfx identity get-principal --identity secondary)"

# Using explicit Candid syntax for nat16 to avoid subtyping errors
# Wrap in (val:type) for extra safety with dfx parser
dfx canister call clearing update_config "(record { 
    insurance_fund_fee_ratio = (10 : nat16); 
    protocol_fee_ratio = (5 : nat16);
    signer_canister = principal \"$PRINCIPAL\"; 
    evm_rpc = principal \"aaaaa-aa\" 
})" || error "Failed to update clearing config"

# --- 2. ASSET SETUP ---
log "Configuring ICP & vUSD collateral assets..."
dfx canister call clearing update_collateral_asset "(record { config = record { asset_id = \"ICP\"; asset = variant { Icrc = principal \"$ICP_LEDGER\" }; symbol = \"ICP\"; decimals = 8; price_usd = record { value = 10000000; decimals = 6 }; haircut_bps = 0; is_enabled = true; } })" >/dev/null
dfx canister call clearing update_collateral_asset "(record { config = record { asset_id = \"vUSD\"; asset = variant { Icrc = principal \"aaaaa-aa\" }; symbol = \"vUSD\"; decimals = 6; price_usd = record { value = 1000000; decimals = 6 }; haircut_bps = 0; is_enabled = true; } })" >/dev/null

# --- 3. IDENTITY SEEDING ---
log "Seeding identities with test tokens..."
./scripts/send.tokens.sh "$PRINCIPAL" 100 >/dev/null
./scripts/send.tokens.sh "$SECONDARY" 100 >/dev/null

setup_icp_collateral "default" "$DEPOSIT_AMOUNT"
setup_icp_collateral "secondary" "$DEPOSIT_AMOUNT"

# --- 4. REGISTRY SETUP (Oracle & Series) ---
log "Registering Oracle and varied Series types..."
dfx identity use default
dfx canister call registry add_authorized_creators "(vec { principal \"$PRINCIPAL\" })" >/dev/null
# Ignore "OracleAlreadyExists"
dfx canister call registry add_oracle "(record { oracle_id = \"TRADE_ORACLE\"; metadata = record { name = \"Test Oracle\"; description = opt record { plain = \"Oracle\"; markdown = null; html = null }; website = null }; authorized_principals = vec { principal \"$PRINCIPAL\" } })" 2>/dev/null || true

# Register Series: Binary
BINARY_SERIES=$(register_series_usd "Binary Test" "variant { Binary }" "null" "INT_BIN_${TIMESTAMP}" | grep -oE '"[a-f0-9]{64}"' | head -n 1 | tr -d '"')

# Register Series: Put (Strike $1.00)
# Price record requires 'decimal' field
PUT_SERIES=$(register_series_usd "Put Test" "variant { Put }" "opt record { decimal = record { value = 1000000; decimals = 6 } }" "INT_PUT_${TIMESTAMP}" | grep -oE '"[a-f0-9]{64}"' | head -n 1 | tr -d '"')

# Register Series: Call (Strike $50,000) - e.g. BTC Call
CALL_SERIES=$(register_series_usd "BTC Call Test" "variant { Call }" "opt record { decimal = record { value = 50000000000; decimals = 6 } }" "INT_CALL_${TIMESTAMP}" | grep -oE '"[a-f0-9]{64}"' | head -n 1 | tr -d '"')

if [ -z "$BINARY_SERIES" ] || [ -z "$PUT_SERIES" ] || [ -z "$CALL_SERIES" ]; then
  error "Failed to register series"
fi

log "Registered Series: BINARY=$BINARY_SERIES, PUT=$PUT_SERIES, CALL=$CALL_SERIES"

# --- 5. EXECUTION: BINARY SCENARIO ---
log "Scenario 1: Binary Trade Matching..."
dfx identity use default
# Buy 10 @ 0.55 USD
dfx canister call clearing submit_matched_trade "(record { trade_id = \"T1_${TIMESTAMP}\"; series_id = \"$BINARY_SERIES\"; buyer = principal \"$PRINCIPAL\"; seller = principal \"$SECONDARY\"; qty = 10 : int; price = record { decimal = record { value = 550000; decimals = 6 }; timestamp = null; oracle_id = null }; })" >/dev/null

# --- 6. EXECUTION: PUT SCENARIO (Margin Enforcement) ---
log "Scenario 2: Put Trade (Short side collateralizes strike)..."
# Seller (Default) sells a Put @ 0.20 USD (Strike $1.0)
dfx canister call clearing submit_matched_trade "(record { trade_id = \"T2_${TIMESTAMP}\"; series_id = \"$PUT_SERIES\"; buyer = principal \"$SECONDARY\"; seller = principal \"$PRINCIPAL\"; qty = 5 : int; price = record { decimal = record { value = 200000; decimals = 6 }; timestamp = null; oracle_id = null }; })" >/dev/null

# --- 7. SETTLEMENT & VERIFICATION ---
log "Settling all series..."
# Settle Binary @ $1.0 (Full Win for Default)
dfx canister call clearing settle_series "(record { series_id = \"$BINARY_SERIES\"; settlement_price = record { decimal = record { value = 1000000; decimals = 6 }; timestamp = null; oracle_id = null }; })" >/dev/null

# Settle Put @ $0.80 (In-the-money for Secondary)
# Payoff for Put = max(Strike - Price, 0) = max(1.0 - 0.8, 0) = 0.20 per unit.
# Secondary (Buyer) gets 5 * 0.20 = 1.00 USD.
dfx canister call clearing settle_series "(record { series_id = \"$PUT_SERIES\"; settlement_price = record { decimal = record { value = 800000; decimals = 6 }; timestamp = null; oracle_id = null }; })" >/dev/null

# Settle Call @ $60,000 (Very In-the-money) - but we didn't trade it, just verifying registration
log "Skipping Call settlement as no trades were executed."

# --- FINAL CHECKS ---
# Under the upfront collateral model, margin cost is deducted from cash_balance_usd
# at trade time. Cash can be negative — users are backed by ICP collateral.
#
# PnL Breakdown (all values in 6-decimal USD units):
#
# T1 Binary (Default=Buyer 10@0.55):
#   Default cost:    -5,500,000  (buyer_margin = 10 * 550,000)
#   Secondary cost:  -4,500,000  (seller_margin = 10 * (1,000,000 - 550,000))
#
# T2 Put (Secondary=Buyer 5@0.20, Strike=$1.0):
#   Secondary cost:  -1,000,000  (buyer_margin = 5 * 200,000)
#   Default cost:    -5,000,000  (seller_margin = 5 * 1,000,000 strike)
#
# Settlement Binary @$1.0 (fees: 10bps insurance + 5bps protocol = 15bps):
#   Default  (Long,  qty=10): gross=10,000,000 i_fee=10,000 p_fee=5,000 net=+9,985,000
#   Secondary(Short, qty=-10): gross=0 fees=0 net=0
#
# Settlement Put @$0.80 (payoff=max(1.0-0.8,0)=0.2):
#   Secondary(Long,  qty=5):  gross=1,000,000 i_fee=1,000 p_fee=500 net=+998,500
#   Default  (Short, qty=-5): gross=0 fees=0 net=0
#
# Expected cash_balance_usd:
#   Default:   0 - 5,500,000 - 5,000,000 + 9,985,000 + 0 = -515,000
#   Secondary: 0 - 4,500,000 - 1,000,000 + 0 + 998,500   = -4,501,500

log "================================================"
log "Final Verification"
log "================================================"

BAL_DEF=$(get_usd_balance "default")
BAL_SEC=$(get_usd_balance "secondary")

log "Cash Balances:"
assert_eq "$BAL_DEF" "-515000" "Default cash_balance_usd"
assert_eq "$BAL_SEC" "-4501500" "Secondary cash_balance_usd"

# Verify all margin is released after settlement
MARGIN_DEF=$(get_equity "default")
MARGIN_SEC=$(get_equity "secondary")

log "Post-Settlement Margin:"
assert_eq "$MARGIN_DEF" "0" "Default reserved_margin_usd (released)"
assert_eq "$MARGIN_SEC" "0" "Secondary reserved_margin_usd (released)"

# Verify fee collection in system funds
# Treasury (protocol fees):  5,000 (Binary) + 500 (Put) = 5,500
# Insurance Fund:           10,000 (Binary) + 1,000 (Put) = 11,000
TREASURY_BAL=$(get_fund_balance "treasury")
INSURANCE_BAL=$(get_fund_balance "insurance_fund")

log "System Funds:"
assert_eq "$TREASURY_BAL" "5500" "Treasury vUSD balance"
assert_eq "$INSURANCE_BAL" "11000" "Insurance Fund vUSD balance"

echo ""
success "Integration suite completed successfully!"

echo ""
log "🚀 System Funds State:"
dfx canister call clearing get_funds
