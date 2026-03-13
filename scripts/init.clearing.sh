#!/usr/bin/env bash

source "$(dirname "$0")/utils.sh" "$@"

source "$(dirname "$0")/init.common.sh"

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

# 2. Update Collateral Asset ($TESTICP_SYMBOL)
echo "Configuring $TESTICP_SYMBOL collateral asset..."
dfx canister call clearing update_collateral_asset "(record { 
    config = record { 
        asset_id = \"$TESTICP_SYMBOL\"; 
        asset = variant { Icrc = principal \"$TEST_ICP_LEDGER\" }; 
        symbol = \"$TESTICP_SYMBOL\"; 
        decimals = $TEST_ICP_DECIMALS : nat8; 
        is_enabled = true; 
        oracle_id = null;
    } 
})" --network "$NETWORK"

# 3. Update Asset Metrics ($TESTICP_SYMBOL)
echo "Setting metrics for $TESTICP_SYMBOL ($((TEST_ICP_HAIRCUT_BPS / 100))% haircut, \$$(echo "scale=2; $TEST_ICP_PRICE_E6 / 1000000" | bc) price)..."
dfx canister call clearing update_asset_metrics "(record { 
    asset_id = \"$TESTICP_SYMBOL\"; 
    metrics = record { 
        haircut_bps = $TEST_ICP_HAIRCUT_BPS : nat16; 
        price_usd = record { value = $TEST_ICP_PRICE_E6 : nat; decimals = $USD_DECIMALS : nat8 };
        latest_transfer_fee = null;
        insurance_fee_ratio = null;
        last_updated_ns = null;
        protocol_fee_ratio = null;
    } 
})" --network "$NETWORK"

# 4. Update Collateral Asset ($TICRC1_SYMBOL)
echo "Configuring $TICRC1_SYMBOL collateral asset..."
dfx canister call clearing update_collateral_asset "(record { 
    config = record { 
        asset_id = \"$TICRC1_SYMBOL\"; 
        asset = variant { Icrc = principal \"$TICRC1_LEDGER\" }; 
        symbol = \"$TICRC1_SYMBOL\"; 
        decimals = $TICRC1_DECIMALS : nat8; 
        is_enabled = true; 
        oracle_id = null;
    } 
})" --network "$NETWORK"

# 5. Update Asset Metrics ($TICRC1_SYMBOL)
echo "Setting metrics for $TICRC1_SYMBOL ($((TICRC1_HAIRCUT_BPS / 100))% haircut, \$$(echo "scale=2; $TICRC1_PRICE_E6 / 1000000" | bc) price)..."
dfx canister call clearing update_asset_metrics "(record { 
    asset_id = \"$TICRC1_SYMBOL\"; 
    metrics = record { 
        haircut_bps = $TICRC1_HAIRCUT_BPS : nat16; 
        price_usd = record { value = $TICRC1_PRICE_E6 : nat; decimals = $USD_DECIMALS : nat8 };
        latest_transfer_fee = null;
        insurance_fee_ratio = null;
        last_updated_ns = null;
        protocol_fee_ratio = null;
    } 
})" --network "$NETWORK"

echo "ICDC Clearing Framework initialized successfully."
