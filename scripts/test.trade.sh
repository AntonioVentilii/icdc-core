#!/bin/bash

# Canister IDs
CLEARING="lqy7q-dh777-77777-aaaaq-cai"

# Set secondary identity
dfx identity get-principal --identity secondary 2>/dev/null || dfx identity new secondary --storage-mode=plaintext
dfx identity use secondary
SECONDARY="$(dfx identity get-principal --identity secondary)"
echo "🚀 Secondary identity created with principal: $SECONDARY"

# Send test tokens to default identity
dfx identity use default
PRINCIPAL="$(dfx identity get-principal)"
echo "🚀 Sending test tokens to default identity ($PRINCIPAL)..."
./scripts/send.tokens.sh "$PRINCIPAL" 20

# Send test tokens to secondary identity
echo "🚀 Sending test tokens to secondary identity ($SECONDARY)..."
./scripts/send.tokens.sh "$SECONDARY" 20

# Set allowance for the default identity to the clearing canister
echo "🚀 Setting allowance for default identity to the clearing canister ($CLEARING)..."
dfx canister call icp_ledger icrc2_approve "(record { fee = null; memo = null; from_subaccount = null; created_at_time = null; amount = 1_000_000_000 : nat; expected_allowance = null; expires_at = null; spender = record { owner = principal \"$CLEARING\"; subaccount = null; }; })"

# Set allowance for the secondary identity to the clearing canister
dfx identity use secondary
echo "🚀 Setting allowance for secondary identity to the clearing canister ($CLEARING)..."
dfx canister call icp_ledger icrc2_approve "(record { fee = null; memo = null; from_subaccount = null; created_at_time = null; amount = 1_000_000_000 : nat; expected_allowance = null; expires_at = null; spender = record { owner = principal \"$CLEARING\"; subaccount = null; }; })"

# Switch back to default identity
dfx identity use default
