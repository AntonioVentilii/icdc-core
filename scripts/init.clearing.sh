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

# 2. Register Collateral Asset ($TESTICP_SYMBOL)
echo "Registering $TESTICP_SYMBOL collateral asset..."
dfx canister call clearing register_icrc_asset "(record { 
    asset_id = \"$TESTICP_SYMBOL\"; 
    ledger_id = principal \"$TESTICP_LEDGER\";
    haircut_bps = $TESTICP_HAIRCUT_BPS : nat16;
    oracle_id = null;
    is_enabled = true;
})" --network "$NETWORK"

# 3. Update Asset Price ($TESTICP_SYMBOL)
echo "Setting price for $TESTICP_SYMBOL (\$$(echo "scale=2; $TESTICP_PRICE_E6 / 1000000" | bc))..."
dfx canister call clearing update_asset_price "(record { 
    asset_id = \"$TESTICP_SYMBOL\"; 
    price = record { decimal = record { value = $TESTICP_PRICE_E6 : nat; decimals = $USD_DECIMALS : nat8 }; oracle_id = null; timestamp = null };
})" --network "$NETWORK"

# 4. Register Collateral Asset ($TICRC1_SYMBOL)
echo "Registering $TICRC1_SYMBOL collateral asset..."
dfx canister call clearing register_icrc_asset "(record { 
    asset_id = \"$TICRC1_SYMBOL\"; 
    ledger_id = principal \"$TICRC1_LEDGER\";
    haircut_bps = $TICRC1_HAIRCUT_BPS : nat16;
    oracle_id = null;
    is_enabled = true;
})" --network "$NETWORK"

# 5. Update Asset Price ($TICRC1_SYMBOL)
echo "Setting price for $TICRC1_SYMBOL (\$$(echo "scale=2; $TICRC1_PRICE_E6 / 1000000" | bc))..."
dfx canister call clearing update_asset_price "(record { 
    asset_id = \"$TICRC1_SYMBOL\"; 
    price = record { decimal = record { value = $TICRC1_PRICE_E6 : nat; decimals = $USD_DECIMALS : nat8 }; oracle_id = null; timestamp = null };
})" --network "$NETWORK"

# 6. Register Collateral Asset ($VUSD_SYMBOL)
if [[ -n "$VUSD_LEDGER" ]]; then
  echo "Registering $VUSD_SYMBOL collateral asset..."
  dfx canister call clearing register_icrc_asset "(record { 
      asset_id = \"$VUSD_SYMBOL\"; 
      ledger_id = principal \"$VUSD_LEDGER\";
      haircut_bps = $VUSD_HAIRCUT_BPS : nat16;
      oracle_id = null;
      is_enabled = true;
  })" --network "$NETWORK"

  echo "Setting price for $VUSD_SYMBOL (\$$(echo "scale=2; $VUSD_PRICE_E6 / 1000000" | bc))..."
  dfx canister call clearing update_asset_price "(record { 
      asset_id = \"$VUSD_SYMBOL\"; 
      price = record { decimal = record { value = $VUSD_PRICE_E6 : nat; decimals = $USD_DECIMALS : nat8 }; oracle_id = null; timestamp = null };
  })" --network "$NETWORK"
fi

echo "ICDC Clearing Framework initialized successfully."
