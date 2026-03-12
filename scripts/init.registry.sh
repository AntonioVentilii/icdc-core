#!/usr/bin/env bash

source "$(dirname "$0")/utils.sh" "$@"

# Canister ID
REGISTRY_CANISTER=$(dfx canister id registry --network "$NETWORK" 2>/dev/null)

if [[ -z "$REGISTRY_CANISTER" ]]; then
  if [[ "$NETWORK" == "staging" ]]; then
    REGISTRY_CANISTER="g5pxl-pyaaa-aaaaj-qqhoq-cai"
  else
    echo "Error: Could not find registry canister ID for network $NETWORK. Is it deployed?"
    exit 1
  fi
fi

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
