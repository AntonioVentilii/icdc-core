#!/usr/bin/env bash

source "$(dirname "$0")/utils.sh" "$@"

source "$(dirname "$0")/init.common.sh"

PRINCIPAL=$(dfx identity get-principal)

echo "Initializing ICDC Registry on network: $NETWORK"
echo "Registry: $REGISTRY_CANISTER"
echo "Principal: $PRINCIPAL"

# 1. Add authorized creators
echo "Authorizing principal as series creator..."
dfx canister call registry add_authorized_creators "(vec { principal \"$PRINCIPAL\" })" --network "$NETWORK" >/dev/null

# 2. Add Oracle
echo "Adding TRADE_ORACLE..."
# Ignore "OracleAlreadyExists"
dfx canister call registry add_oracle "(record { 
    oracle_id = \"TRADE_ORACLE\"; 
    metadata = record { 
        name = \"Test Oracle\"; 
        description = opt record { plain = \"Oracle\"; markdown = null; html = null }; 
        website = null 
    }; 
    authorized_principals = vec { principal \"$PRINCIPAL\" } 
})" --network "$NETWORK" 2>/dev/null || true

echo "ICDC Registry initialized successfully."
