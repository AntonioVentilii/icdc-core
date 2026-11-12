#!/usr/bin/env bash

source "$(dirname "$0")/utils.sh" "$@"

source "$(dirname "$0")/init.common.sh"

PRINCIPAL=$(dfx identity get-principal)

echo "Initializing ICDC Registry on network: $NETWORK"
echo "Registry: $REGISTRY_CANISTER"
echo "Principal: $PRINCIPAL"

# 1. Register default engine with Creator + OracleAdmin roles
echo "Registering default engine..."
REGISTER_RESULT=$(dfx canister call registry register_engine "(record {
    name = \"Default\";
    description = null;
    icon_url = null;
    admins = vec { principal \"$PRINCIPAL\" };
    allowed_roles = vec { variant { Creator }; variant { OracleAdmin } }
})" --network "$NETWORK")

ENGINE_ID=$(echo "$REGISTER_RESULT" | grep -Eo '"[^"]*"' | head -1 | tr -d '"')
if [ -z "$ENGINE_ID" ]; then
  echo "ERROR: Failed to register engine. Response: $REGISTER_RESULT"
  exit 1
fi
echo "Registered engine: $ENGINE_ID"

# 2. Grant Creator role to deploying principal
echo "Granting Creator role..."
dfx canister call registry grant_engine_role "(record {
    engine_id = \"$ENGINE_ID\";
    grantee = principal \"$PRINCIPAL\";
    role = variant { Creator }
})" --network "$NETWORK" >/dev/null

# 3. Add Oracle
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
