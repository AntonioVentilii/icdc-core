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

# 1b. Domain policies (Settlement, Playground, ViciXp)
#     Stored for admin / future enforcement; same defaults as DomainPolicy in code.
echo "Configuring balance domain policies (Settlement, Playground, ViciXp)..."
dfx canister call clearing update_domain_policy "(record {
  domain = variant { Settlement };
  policy = record {
    deposits_enabled = true;
    protocol_fee_ratio_override = null;
    label = \"Settlement\";
    withdrawals_enabled = true;
    insurance_fund_fee_ratio_override = null;
  };
})" --network "$NETWORK"

dfx canister call clearing update_domain_policy "(record {
  domain = variant { Playground };
  policy = record {
    deposits_enabled = true;
    protocol_fee_ratio_override = null;
    label = \"Playground\";
    withdrawals_enabled = true;
    insurance_fund_fee_ratio_override = null;
  };
})" --network "$NETWORK"

dfx canister call clearing update_domain_policy "(record {
  domain = variant { ViciXp };
  policy = record {
    deposits_enabled = true;
    protocol_fee_ratio_override = null;
    label = \"Vici XP\";
    withdrawals_enabled = true;
    insurance_fund_fee_ratio_override = null;
  };
})" --network "$NETWORK"

# 2. Register collateral (ICRC): allowed_balance_domains enforced on deposit/withdraw.
#    ICP → Settlement. VXP → ViciXp only. Test tokens → Playground only.
#    Tune CLEARING_DOMAINS_* and prices in init.common.sh if needed.

# 2a. ICP (Settlement only)
echo "Registering $ICP_SYMBOL collateral asset (domains: Settlement)..."
dfx canister call clearing register_icrc_asset "(record {
    asset_id = \"$ICP_SYMBOL\";
    ledger_id = principal \"$ICP_LEDGER\";
    haircut_bps = $ICP_HAIRCUT_BPS : nat16;
    oracle_id = null;
    is_enabled = true;
    allowed_balance_domains = $CLEARING_DOMAINS_SETTLEMENT;
})" --network "$NETWORK"

echo "Setting price for $ICP_SYMBOL (\$$(echo "scale=6; $ICP_PRICE_E6 / (10^$USD_DECIMALS)" | bc))..."
dfx canister call clearing update_asset_price "(record {
    asset_id = \"$ICP_SYMBOL\";
    price = record { decimal = record { value = $ICP_PRICE_E6 : nat; decimals = $USD_DECIMALS : nat8 }; oracle_id = null; timestamp = null };
})" --network "$NETWORK"

# 2b. Vici XP / VXP (ViciXp domain only)
echo "Registering $VICI_XP_SYMBOL collateral asset (domains: ViciXp; ledger $VICI_XP_LEDGER)..."
dfx canister call clearing register_icrc_asset "(record {
    asset_id = \"$VICI_XP_SYMBOL\";
    ledger_id = principal \"$VICI_XP_LEDGER\";
    haircut_bps = $VICI_XP_HAIRCUT_BPS : nat16;
    oracle_id = null;
    is_enabled = true;
    allowed_balance_domains = $CLEARING_DOMAINS_VICI_XP;
})" --network "$NETWORK"

echo "Setting price for $VICI_XP_SYMBOL (\$$(echo "scale=6; $VICI_XP_PRICE_E6 / (10^$USD_DECIMALS)" | bc))..."
dfx canister call clearing update_asset_price "(record {
    asset_id = \"$VICI_XP_SYMBOL\";
    price = record { decimal = record { value = $VICI_XP_PRICE_E6 : nat; decimals = $USD_DECIMALS : nat8 }; oracle_id = null; timestamp = null };
})" --network "$NETWORK"

# 2c. TESTICP (playground-only)
echo "Registering $TESTICP_SYMBOL collateral asset (domains: Playground)..."
dfx canister call clearing register_icrc_asset "(record { 
    asset_id = \"$TESTICP_SYMBOL\"; 
    ledger_id = principal \"$TESTICP_LEDGER\";
    haircut_bps = $TESTICP_HAIRCUT_BPS : nat16;
    oracle_id = null;
    is_enabled = false;
    allowed_balance_domains = $CLEARING_DOMAINS_PLAYGROUND;
})" --network "$NETWORK"

# 3. Update Asset Price ($TESTICP_SYMBOL)
echo "Setting price for $TESTICP_SYMBOL (\$$(echo "scale=6; $TESTICP_PRICE_E6 / (10^$USD_DECIMALS)" | bc))..."
dfx canister call clearing update_asset_price "(record { 
    asset_id = \"$TESTICP_SYMBOL\"; 
    price = record { decimal = record { value = $TESTICP_PRICE_E6 : nat; decimals = $USD_DECIMALS : nat8 }; oracle_id = null; timestamp = null };
})" --network "$NETWORK"

# 4. TICRC1 (playground-only)
echo "Registering $TICRC1_SYMBOL collateral asset (domains: Playground)..."
dfx canister call clearing register_icrc_asset "(record { 
    asset_id = \"$TICRC1_SYMBOL\"; 
    ledger_id = principal \"$TICRC1_LEDGER\";
    haircut_bps = $TICRC1_HAIRCUT_BPS : nat16;
    oracle_id = null;
    is_enabled = false;
    allowed_balance_domains = $CLEARING_DOMAINS_PLAYGROUND;
})" --network "$NETWORK"

# 5. Update Asset Price ($TICRC1_SYMBOL)
echo "Setting price for $TICRC1_SYMBOL (\$$(echo "scale=6; $TICRC1_PRICE_E6 / (10^$USD_DECIMALS)" | bc))..."
dfx canister call clearing update_asset_price "(record { 
    asset_id = \"$TICRC1_SYMBOL\"; 
    price = record { decimal = record { value = $TICRC1_PRICE_E6 : nat; decimals = $USD_DECIMALS : nat8 }; oracle_id = null; timestamp = null };
})" --network "$NETWORK"

# 6. vUSD — internal ledger only (Config.internal_ledger in clearing install args).
#    Not collateral; no register_icrc_asset. Equity uses cash_balances_usd (USD), not vUSD metrics,
#    and user vUSD token balances are not in the collateral map — so seeding ASSET_METRICS for vUSD
#    here is unnecessary. Call update_asset_metrics yourself only if ops tooling wants a row.
if [[ -n "$VUSD_LEDGER" ]]; then
  # 7. Add Clearing as controller of vUSD Ledger
  echo "Guaranteeing Clearing canister as controller of $VUSD_SYMBOL Ledger..."
  # Fetch current controllers and add clearing if not already present
  CURRENT_CONTROLLERS=$(dfx canister status "$VUSD_LEDGER" --network "$NETWORK" | grep "Controllers:" | cut -d: -f2)
  if [[ ! "$CURRENT_CONTROLLERS" == *"$CLEARING_CANISTER"* ]]; then
    dfx canister update-settings "$VUSD_LEDGER" --network "$NETWORK" --add-controller "$CLEARING_CANISTER"
  fi
fi

echo "ICDC Clearing Framework initialized successfully."
