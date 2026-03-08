#!/bin/bash

# --- HELPER FUNCTIONS ---

get_usd_balance() {
  local target_identity="$1"
  local current_identity
  current_identity=$(dfx identity whoami)

  dfx identity use "$target_identity" >/dev/null 2>&1
  local res
  res=$(dfx canister call clearing get_account_state "(record { refresh = null })")

  # Restore original identity
  dfx identity use "$current_identity" >/dev/null 2>&1

  # Extract cash_balance_usd
  echo "$res" | grep -oE 'cash_balance_usd = -?[0-9_]+' | awk '{print $3}' | tr -d '_'
}

print_account_state() {
  dfx canister call clearing get_account_state "(record { refresh = null })"
}

# --- CONFIGURATION ---
ALLOWANCE=10000000000     # 100 ICP
DEPOSIT_AMOUNT=1000000000 # 10 ICP
TRADE_QTY=10              # 10 units
TRADE_PRICE=550000        # 0.55 USD (e6)
SETTLEMENT_PRICE=1000000  # 1.00 USD (e6)

# Init
# Wait a bit to ensure unique series ID
sleep 1
TIMESTAMP=$(date +%s)

# Canister IDs
CLEARING=$(dfx canister id clearing)
ICP_LEDGER=$(dfx canister id icp_ledger)
REGISTRY=$(dfx canister id registry)

# Inititalize with default identity
dfx identity use default
PRINCIPAL="$(dfx identity get-principal)"
echo "🚀 Default identity principal: $PRINCIPAL"

# Set secondary identity
dfx identity get-principal --identity secondary 2>/dev/null || dfx identity new secondary --storage-mode=plaintext
SECONDARY="$(dfx identity get-principal --identity secondary)"
echo "🚀 Secondary identity created with principal: $SECONDARY"

# 1. INITIALIZE CLEARING & REGISTRY CONFIG
echo "🚀 Initializing clearing canister..."
dfx canister call clearing set_registry_canister "(principal \"$REGISTRY\")"
dfx canister call clearing update_config "(record { 
    insurance_fund_fee_ratio = 10; 
    signer_canister = principal \"$PRINCIPAL\"; 
    evm_rpc = principal \"aaaaa-aa\" 
})"

# 2. CONFIGURE COLLATERAL ASSETS
echo "🚀 Configuring ICP as collateral asset..."
dfx canister call clearing update_collateral_asset "(
  record {
    config = record {
      asset_id = \"ICP\";
      asset = variant { Icrc = principal \"$ICP_LEDGER\" };
      symbol = \"ICP\";
      decimals = 8;
      price_usd = record { value = 10000000; decimals = 6 }; // 10.00 USD
      haircut_bps = 0; // No haircut
      is_enabled = true;
    }
  }
)"

# Configure vUSD as local Cash asset
echo "🚀 Configuring vUSD as collateral asset..."
dfx canister call clearing update_collateral_asset "(
  record {
    config = record {
      asset_id = \"vUSD\";
      asset = variant { Icrc = principal \"aaaaa-aa\" }; // Placeholder if not used via ledger
      symbol = \"vUSD\";
      decimals = 6;
      price_usd = record { value = 1000000; decimals = 6 }; // 1.00 USD
      haircut_bps = 0;
      is_enabled = true;
    }
  }
)"

# 3. AUTHORIZE CREATOR & REGISTER ORACLE
echo "🚀 Authorizing default identity as creator in registry..."
dfx canister call registry add_authorized_creators "(vec { principal \"$PRINCIPAL\" })"

echo "🚀 Registering Oracle..."
dfx canister call registry add_oracle "(record { oracle_id = \"TRADE_ORACLE\"; metadata = record { name = \"Trade Oracle\"; description = opt record { plain = \"Test Oracle\"; markdown = null; html = null }; website = null }; authorized_principals = vec { principal \"$PRINCIPAL\" } })"

# 4. DEPOSIT COLLATERAL
# BAL_START is USD cash balance
BAL_START_DEFAULT=$(get_usd_balance "default")
BAL_START_SECONDARY=$(get_usd_balance "secondary")

# Send test tokens to identities (for collateral)
./scripts/send.tokens.sh "$PRINCIPAL" 50
./scripts/send.tokens.sh "$SECONDARY" 50

# Collateral Setup (Default)
dfx identity use default
dfx canister call icp_ledger icrc2_approve "(record { amount = $ALLOWANCE : nat; spender = record { owner = principal \"$CLEARING\" }; })"
dfx canister call clearing deposit_collateral "(record { deposit_id = \"DEP_${TIMESTAMP}\"; asset_id = \"ICP\"; amount = $DEPOSIT_AMOUNT : nat })"

# Collateral Setup (Secondary)
dfx identity use secondary
dfx canister call icp_ledger icrc2_approve "(record { amount = $ALLOWANCE : nat; spender = record { owner = principal \"$CLEARING\" }; })"
dfx canister call clearing deposit_collateral "(record { deposit_id = \"DEP_SEC_${TIMESTAMP}\"; asset_id = \"ICP\"; amount = $DEPOSIT_AMOUNT : nat })"

# 5. REGISTER SERIES
dfx identity use default
RESULT=$(dfx canister call registry add_series "(
  record {
    title = \"Trade Test\";
    strike = null;
    payoff_type = variant { Binary };
    payout_unit = variant { Fiat = variant { Usd } };
    underlying = \"TRADE_TEST_${TIMESTAMP}\";
    expiry_ns = 2_000_000_000_000_000_000 : nat64;
    oracle_source = \"TRADE_ORACLE\";
    description = record { plain = \"Test Desc\"; markdown = null; html = null };
    price_precision = 8;
  }
)")
SERIES_ID=$(echo "$RESULT" | grep -oE '"[a-f0-9]{64}"' | head -n 1 | tr -d '"')
echo "Series ID: $SERIES_ID"

# 6. SUBMIT TRADE
echo "🚀 Submitting a trade (Default buys 10 @ 0.55 USD from Secondary)..."
dfx canister call clearing submit_matched_trade "(
  record {
    trade_id = \"T_${TIMESTAMP}\";
    series_id = \"$SERIES_ID\";
    buyer = principal \"$PRINCIPAL\";
    seller = principal \"$SECONDARY\";
    qty = $TRADE_QTY : int;
    price = record { decimal = record { value = $TRADE_PRICE; decimals = 6 }; timestamp = null; oracle_id = null };
  }
)"

# 7. SETTLE SERIES
echo "🚀 Settling series at 1.00 USD..."
dfx canister call clearing settle_series "(
  record {
    series_id = \"$SERIES_ID\";
    settlement_price = record { decimal = record { value = $SETTLEMENT_PRICE; decimals = 6 }; timestamp = null; oracle_id = null };
  }
)"

# 8. VERIFICATION
FINAL_BAL_DEFAULT=$(get_usd_balance "default")
FINAL_BAL_SECONDARY=$(get_usd_balance "secondary")

DELTA_DEFAULT=$((FINAL_BAL_DEFAULT - BAL_START_DEFAULT))
DELTA_SECONDARY=$((FINAL_BAL_SECONDARY - BAL_START_SECONDARY))

# PnL = (1.0 - 0.55) * 10 = 0.45 * 10 = 4.5 USD = 4,500,000 (e6)
PROFIT=4500000
# Note: default insurance fee is 10 bps (0.1%) of payout.
# Payout = 10 * 1.0 = 10 USD = 10,000,000.
# 0.1% fee = 10,000.
FEE=10000
EXPECTED_DELTA_DEFAULT=$((PROFIT - FEE)) # Profit minus fee
EXPECTED_DELTA_SECONDARY=$((-PROFIT))    # Loss (seller doesn't pay payout fee)

echo "Default Delta:   $DELTA_DEFAULT (Expected: $EXPECTED_DELTA_DEFAULT)"
echo "Secondary Delta: $DELTA_SECONDARY (Expected: $EXPECTED_DELTA_SECONDARY)"

if [ "$DELTA_DEFAULT" -eq "$EXPECTED_DELTA_DEFAULT" ] && [ "$DELTA_SECONDARY" -eq "$EXPECTED_DELTA_SECONDARY" ]; then
  echo "✅ TRADE TEST PASSED!"
else
  echo "❌ TRADE TEST FAILED!"
  exit 1
fi
