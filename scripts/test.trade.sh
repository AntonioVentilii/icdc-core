#!/bin/bash

print_margin_account() {
  local principal="$1"
  dfx canister call clearing get_margin_account "(principal \"$principal\")"
}

print_settlement_snapshot() {
  local when="$1"
  echo "📸 Margin accounts $when:"
  print_margin_account "$PRINCIPAL"
  print_margin_account "$SECONDARY"
}

# Canister IDs
CLEARING="uxrrr-q7777-77774-qaaaq-cai"
ICP_LEDGER="ryjl3-tyaaa-aaaaa-aaaba-cai"

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
./scripts/send.tokens.sh "$PRINCIPAL" 20

# Send test tokens to secondary identity
echo "🚀 Sending test tokens to secondary identity ($SECONDARY)..."
./scripts/send.tokens.sh "$SECONDARY" 20

# Set allowance for the default identity
echo "🚀 Setting allowance for default identity to the clearing canister $CLEARING..."
dfx canister call icp_ledger icrc2_approve "(
  record {
    fee = null;
    memo = null;
    from_subaccount = null;
    created_at_time = null;
    amount = 1_000_000_000 : nat;
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
    asset = variant { Icrc = principal \"$ICP_LEDGER\" };
    amount = 200_000_000 : nat;
  },
)"
dfx canister call clearing get_margin_account "(principal \"$PRINCIPAL\")"

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
    amount = 1_000_000_000 : nat;
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
    asset = variant { Icrc = principal \"$ICP_LEDGER\" };
    amount = 200_000_000 : nat;
  },
)"
dfx canister call clearing get_margin_account "(principal \"$SECONDARY\")"

# Switch back to default identity
dfx identity use default

# Register series
TIMESTAMP=$(date +%s)
RESULT=$(dfx canister call registry add_series "(
  record {
    strike = null;
    payoff_type = variant { Binary };
    settlement_asset = variant { Icp };
    underlying = \"TEST_${TIMESTAMP}\";
    expiry = 1_782_816_000 : nat64;
    oracle_source = \"VICI_ORACLE_V1\";
  },
)")
SERIES_ID=$(echo "$RESULT" | grep -oE '"[a-f0-9]{64}"' | tr -d '"')
if [ -z "$SERIES_ID" ]; then
  echo "❌ Failed to extract Series ID"
  echo "Full response:"
  echo "$RESULT"
  exit 1
fi
echo "Series ID: $SERIES_ID"

# Submit a trade
echo "🚀 Submitting a trade from default identity to secondary identity..."
dfx canister call clearing submit_matched_trade "(
  record {
    series_id = \"$SERIES_ID\";
    buyer = principal \"$PRINCIPAL\";
    seller = principal \"$SECONDARY\";
    qty = 10 : int;
    price = 55_000_000 : nat64;
  },
)"

# Check positions
echo "🚀 Checking position for both identities..."
dfx canister call clearing get_position "(
  record {
    series_id = \"$SERIES_ID\";
    user = principal \"$PRINCIPAL\";
  },
)"
dfx canister call clearing get_position "(
  record {
    series_id = \"$SERIES_ID\";
    user = principal \"$SECONDARY\";
  },
)"

# Snapshot BEFORE settlement (last step before settle_series)
print_settlement_snapshot "AFTER"

# Settle series
echo "🚀 Settling series in favour of default identity..."
dfx canister call clearing settle_series "(
  record {
    series_id = \"$SERIES_ID\";
    settlement_price = 100_000_000 : nat64;
  },
)"

# Snapshot AFTER settlement (first step after settle_series)
print_settlement_snapshot "AFTER"

# Check positions after settlement
echo "🚀 Checking positions after settlement..."
dfx canister call clearing get_position "(
  record {
    series_id = \"$SERIES_ID\";
    user = principal \"$PRINCIPAL\";
  },
)"
dfx canister call clearing get_position "(
  record {
    series_id = \"$SERIES_ID\";
    user = principal \"$SECONDARY\";
  },
)"

# Conclusion
echo "✅ Test flow completed!"
