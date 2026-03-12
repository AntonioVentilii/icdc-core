#!/usr/bin/env bash

source "$(dirname "$0")/utils.sh" "$@"

# --- CONFIGURATION ---
TEST_ICP_LEDGER="xafvr-biaaa-aaaai-aql5q-cai"
HAIRCUT_BPS=1000     # 10%
PRICE_USD_E6=2000000 # 2 USD (6 decimals)

# Canister IDs
CLEARING_CANISTER=$(dfx canister id clearing --network "$NETWORK" 2>/dev/null)
REGISTRY_CANISTER=$(dfx canister id registry --network "$NETWORK" 2>/dev/null)

if [[ -z "$CLEARING_CANISTER" ]]; then
  echo "Error: Could not find clearing canister ID for network $NETWORK. Is it deployed?"
  exit 1
fi

if [[ -z "$REGISTRY_CANISTER" ]]; then
  if [[ "$NETWORK" == "staging" ]]; then
    REGISTRY_CANISTER="g5pxl-pyaaa-aaaaj-qqhoq-cai"
  else
    echo "Error: Could not find registry canister ID for network $NETWORK."
    exit 1
  fi
fi

echo "Initializing ICDC Clearing Framework on network: $NETWORK"
echo "Clearing: $CLEARING_CANISTER"
echo "Registry: $REGISTRY_CANISTER"

# 1. Set Registry Canister
echo "Setting registry canister..."
dfx canister call clearing set_registry_canister "(principal \"$REGISTRY_CANISTER\")" --network "$NETWORK"

# 2. Update Collateral Asset (TESTICP)
echo "Configuring TESTICP collateral asset..."
dfx canister call clearing update_collateral_asset "(record { 
    config = record { 
        asset_id = \"TESTICP\"; 
        asset = variant { Icrc = principal \"$TEST_ICP_LEDGER\" }; 
        symbol = \"TESTICP\"; 
        decimals = 8 : nat8; 
        is_enabled = true; 
        oracle_id = null;
    } 
})" --network "$NETWORK"

# 3. Update Asset Metrics (TESTICP)
echo "Setting metrics for TESTICP (10% haircut, \$2.00 price)..."
dfx canister call clearing update_asset_metrics "(record { 
    asset_id = \"TESTICP\"; 
    metrics = record { 
        haircut_bps = $HAIRCUT_BPS : nat16; 
        price_usd = record { value = $PRICE_USD_E6 : nat; decimals = 6 : nat8 };
        latest_transfer_fee = null;
        insurance_fee_ratio = null;
        last_updated_ns = null;
        protocol_fee_ratio = null;
    } 
})" --network "$NETWORK"

echo "ICDC Clearing Framework initialized successfully."
