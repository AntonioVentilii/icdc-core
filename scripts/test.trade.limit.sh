#!/bin/bash

print_margin_account() {
  dfx canister call clearing get_margin_account "(record { refresh = null })"
}

# Snapshot BEFORE settlement (last step before settle_series)
print_settlement_snapshot() {
  local when="$1"
  echo "📸 Margin accounts $when:"
  dfx identity use default
  print_margin_account
  dfx identity use secondary
  print_margin_account
  dfx identity use default
}

get_balance() {
  local target_identity="$1"
  local current_identity
  current_identity=$(dfx identity whoami)

  dfx identity use "$target_identity" >/dev/null 2>&1
  local res
  res=$(dfx canister call clearing get_margin_account "(record { refresh = null })")

  # Restore original identity
  dfx identity use "$current_identity" >/dev/null 2>&1

  # Extract the last : nat occurrences (the balance, not required_margin)
  echo "$res" | grep -oE '[0-9_]+ : nat' | tail -n 1 | awk '{print $1}' | tr -d '_'
}

# --- CONFIGURATION ---
ALLOWANCE=10000000000      # 100 ICP
DEPOSIT_AMOUNT=1000000000  # 10 ICP
TRADE_QTY=10               # 10 units
TRADE_PRICE=55000000       # 0.55 ICP
SETTLEMENT_PRICE=100000000 # 1 ICP
LEDGER_FEE=10000           # 0.0001 ICP, adjust based on actual fee structure

# Init
TIMESTAMP=$(date +%s)

# Canister IDs
CLEARING=$(dfx canister id clearing)
ICP_LEDGER=$(dfx canister id icp_ledger)
REGISTRY=$(dfx canister id registry)

# Initialize clearing canister with registry ID
echo "🚀 Setting registry canister ID to $REGISTRY in clearing canister $CLEARING..."
dfx canister call clearing set_registry_canister "(principal \"$REGISTRY\")"

# Inititalize with default identity
dfx identity use default
PRINCIPAL="$(dfx identity get-principal)"
echo "🚀 Default identity principal: $PRINCIPAL"

# Set secondary identity
dfx identity get-principal --identity secondary 2>/dev/null || dfx identity new secondary --storage-mode=plaintext
SECONDARY="$(dfx identity get-principal --identity secondary)"
echo "🚀 Secondary identity created with principal: $SECONDARY"

# Send test tokens to default identity
echo "🚀 Sending test tokens to default identity ($PRINCIPAL)..."
./scripts/send.tokens.sh "$PRINCIPAL" 50

# Send test tokens to secondary identity
echo "🚀 Sending test tokens to secondary identity ($SECONDARY)..."
./scripts/send.tokens.sh "$SECONDARY" 50

# Start balances
BAL_START_DEFAULT=$(get_balance "default")
BAL_START_SECONDARY=$(get_balance "secondary")

# Set allowance for the default identity
echo "🚀 Setting allowance for default identity to the clearing canister $CLEARING..."
dfx canister call icp_ledger icrc2_approve "(
  record {
    fee = null;
    memo = null;
    from_subaccount = null;
    created_at_time = null;
    amount = $ALLOWANCE : nat;
    expected_allowance = null;
    expires_at = null;
    spender = record {
      owner = principal \"$CLEARING\";
      subaccount = null;
    };
  }
)"

## Wait 10 seconds to ensure the approval is processed before attempting to deposit collateral
#echo "⏳ Waiting 10 seconds for the approval to be processed..."
#sleep 10

# Deposit collateral for the default identity
echo "🚀 Depositing collateral for default identity..."
dfx canister call clearing deposit_collateral "(
  record {
    deposit_id = \"DEPOSIT_TEST_${TIMESTAMP}\";
    asset = variant { Icrc = principal \"$ICP_LEDGER\" };
    amount = $DEPOSIT_AMOUNT : nat;
  },
)"
print_margin_account

# Switch to secondary identity
dfx identity use secondary

# Set allowance for the secondary identity to the clearing canister
echo "🚀 Setting allowance for secondary identity to the clearing canister $CLEARING..."
dfx canister call icp_ledger icrc2_approve "(
  record {
    fee = null;
    memo = null;
    from_subaccount = null;
    created_at_time = null;
    amount = $ALLOWANCE : nat;
    expected_allowance = null;
    expires_at = null;
    spender = record {
      owner = principal \"$CLEARING\";
      subaccount = null;
    };
  }
)"

# Deposit collateral for the secondary identity
echo "🚀 Depositing collateral for secondary identity..."
dfx canister call clearing deposit_collateral "(
  record {
    deposit_id = \"DEPOSIT_TEST_${TIMESTAMP}\";
    asset = variant { Icrc = principal \"$ICP_LEDGER\" };
    amount = $DEPOSIT_AMOUNT : nat;
  },
)"
dfx canister call clearing get_margin_account "(record { refresh = null })"

# Switch back to default identity
dfx identity use default

# Register oracle
echo "🚀 Registering oracle VICI_ORACLE_V1..."
dfx canister call registry add_oracle "(
  record {
    oracle_id = \"VICI_ORACLE_V1\";
    metadata = record {
      name = \"Vici Oracle\";
      description = opt \"Automated test oracle\";
      website = null;
    };
    authorized_principals = vec { principal \"$PRINCIPAL\" };
  }
)"

# Register series
RESULT=$(dfx canister call registry add_series "(
  record {
    strike = null;
    payoff_type = variant { Binary };
    settlement_asset = variant { Icp };
    underlying = \"TEST_${TIMESTAMP}\";
    expiry_ns = 1_782_816_000_000_000_000 : nat64;
    oracle_source = \"VICI_ORACLE_V1\";
    title = \"Test Series\";
    description = \"Automated test series description\";
  },
)")
SERIES_ID=$(echo "$RESULT" | grep -oE '"[a-f0-9]{64}"' | head -n 1 | tr -d '"')
if [ -z "$SERIES_ID" ]; then
  echo "❌ Failed to extract Series ID"
  echo "Full response:"
  echo "$RESULT"
  exit 1
fi
echo "Series ID: $SERIES_ID"

# Submit a trade
echo "🚀 Submitting a limit order from default identity..."
dfx canister call clearing submit_limit_order "(
  record {
    order_id = \"ORDER_TEST_${TIMESTAMP}\";
    series_id = \"$SERIES_ID\";
    side = variant { Buy };
    qty = $TRADE_QTY : int;
    price = $TRADE_PRICE : nat64;
  },
)"

# Check positions
echo "🚀 Checking position for both identities..."
dfx identity use default
dfx canister call clearing get_position "(
  record {
    series_id = \"$SERIES_ID\";
  },
)"
dfx identity use secondary
dfx canister call clearing get_position "(
  record {
    series_id = \"$SERIES_ID\";
  },
)"
dfx identity use default

# Snapshot BEFORE settlement (last step before settle_series)
print_settlement_snapshot "BEFORE"

# Settle series
echo "🚀 Settling series in favour of default identity..."
dfx canister call clearing settle_series "(
  record {
    series_id = \"$SERIES_ID\";
    settlement_price = $SETTLEMENT_PRICE : nat64;
  },
)"

# Snapshot AFTER settlement (first step after settle_series)
print_settlement_snapshot "AFTER"

# Check positions after settlement
echo "🚀 Checking positions after settlement..."
dfx identity use default
dfx canister call clearing get_position "(
  record {
    series_id = \"$SERIES_ID\";
  },
)"
dfx identity use secondary
dfx canister call clearing get_position "(
  record {
    series_id = \"$SERIES_ID\";
  },
)"
dfx identity use default

# --- VERBOSE SUMMARY ---
echo "-------------------------------------------------------"
echo "📊 TEST SUMMARY & VERIFICATION"
echo "-------------------------------------------------------"

FINAL_BAL_DEFAULT=$(get_balance "default")
FINAL_BAL_SECONDARY=$(get_balance "secondary")

DELTA_DEFAULT=$((FINAL_BAL_DEFAULT - BAL_START_DEFAULT))
DELTA_SECONDARY=$((FINAL_BAL_SECONDARY - BAL_START_SECONDARY))

# Expected changes (Binary Payoff Logic)
# Winners Delta = Deposit + Profit - Fee
# Losers Delta  = Deposit - Loss - Fee (if Loss is debt collected)
PROFIT=$((TRADE_QTY * (SETTLEMENT_PRICE - TRADE_PRICE)))

# For binary Long: if Win, payout is MAX_PAYOFF. So Profit is (MAX_PAYOFF - TRADE_PRICE) * QTY = (100M - 55M) * 10 = 450M.
# For binary Short: if Loss, debt is (MAX_PAYOFF - (MAX_PAYOFF - TRADE_PRICE)) * QTY? No.
# Actually, the user's test CASE is:
# Trade 10 @ 55, Settlement 100.
# Default (Long) profit = (100 - 55) * 10 = 450M.
# Secondary (Short) loss = (100 - 55) * 10 = 450M.
EXPECTED_DELTA_DEFAULT=$((DEPOSIT_AMOUNT + PROFIT - LEDGER_FEE))
EXPECTED_DELTA_SECONDARY=$((DEPOSIT_AMOUNT - PROFIT - LEDGER_FEE))

echo "Trade:            $TRADE_QTY units @ $TRADE_PRICE e8"
echo "Settlement:       $SETTLEMENT_PRICE e8"
echo "Net Profit/Loss:  $PROFIT e8"
echo "Ledger Fee:       $LEDGER_FEE e8"
echo ""
echo "Identity  | Expected Δ Balance | Actual Δ Balance | Status"
echo "----------|-------------------|------------------|-------"

status_default="❌ FAIL"
if [ "$DELTA_DEFAULT" -eq "$EXPECTED_DELTA_DEFAULT" ]; then status_default="✅ PASS"; fi

status_secondary="❌ FAIL"
if [ "$DELTA_SECONDARY" -eq "$EXPECTED_DELTA_SECONDARY" ]; then status_secondary="✅ PASS"; fi

printf "%-9s | +%-16s | +%-15s | %s\n" "Default" "$EXPECTED_DELTA_DEFAULT" "$DELTA_DEFAULT" "$status_default"
printf "%-9s | +%-16s | +%-15s | %s\n" "Secondary" "$EXPECTED_DELTA_SECONDARY" "$DELTA_SECONDARY" "$status_secondary"

echo "-------------------------------------------------------"

if [ "$DELTA_DEFAULT" -eq "$EXPECTED_DELTA_DEFAULT" ] && [ "$DELTA_SECONDARY" -eq "$EXPECTED_DELTA_SECONDARY" ]; then
  echo "🎉 ALL EXPECTATIONS MATCH REALITY!"
  echo "✅ Test flow completed successfully."
  exit 0
else
  echo "⚠️ SOME EXPECTATIONS DID NOT MATCH."
  echo "❌ Test flow failed verification."
  # List actual deltas for debugging
  echo "Default Logic:   Expected $EXPECTED_DELTA_DEFAULT, Got $DELTA_DEFAULT"
  echo "Secondary Logic: Expected $EXPECTED_DELTA_SECONDARY, Got $DELTA_SECONDARY"
  exit 1
fi
