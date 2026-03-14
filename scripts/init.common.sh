#!/usr/bin/env bash

# --- SHARED CONFIGURATION ---

# Canister IDs
REGISTRY_CANISTER=$(dfx canister id registry --network "$NETWORK" 2>/dev/null)
if [[ -z "$REGISTRY_CANISTER" ]]; then
  echo "Error: Could not find registry canister ID."
  exit 1
fi
export REGISTRY_CANISTER

CLEARING_CANISTER=$(dfx canister id clearing --network "$NETWORK" 2>/dev/null)
if [[ -z "$CLEARING_CANISTER" ]]; then
  echo "Error: Could not find clearing canister ID."
  exit 1
fi
export CLEARING_CANISTER

# USD Configuration
export USD_DECIMALS=6

#TODO: do i need to register this as collateral for the clearing canister? Is it not used only as ledger?
# vUSD Configuration
VUSD_LEDGER_ID=$(dfx canister id ledger --network "$NETWORK" 2>/dev/null)
if [[ -n "$VUSD_LEDGER_ID" ]]; then
  export VUSD_SYMBOL="vUSD"
  export VUSD_LEDGER="$VUSD_LEDGER_ID"
  export VUSD_DECIMALS=8
  export VUSD_HAIRCUT_BPS=0    # 0% haircut for USD stable
  export VUSD_PRICE_E6=1000000 # 1 USD (6 decimals)
fi

# TESTICP Token Configuration
export TESTICP_SYMBOL="TESTICP"
export TESTICP_LEDGER="xafvr-biaaa-aaaai-aql5q-cai"
export TESTICP_INDEX="qcuy6-bqaaa-aaaai-aqmqq-cai"
export TESTICP_DECIMALS=8
export TESTICP_HAIRCUT_BPS=1000 # 10%
export TESTICP_PRICE_E6=3000000 # 3 USD (6 decimals)

# TICRC1 Token Configuration
export TICRC1_SYMBOL="TICRC1"
export TICRC1_LEDGER="3jkp5-oyaaa-aaaaj-azwqa-cai"
export TICRC1_INDEX="qzre3-3iaaa-aaaai-aqmsa-cai"
export TICRC1_DECIMALS=8
export TICRC1_HAIRCUT_BPS=2500 # 25%
export TICRC1_PRICE_E6=500000  # 0.5 USD (6 decimals)

# Shared Minting Account
export TEST_MINTING_ACCOUNT="bnuz2-zaaaa-aaaal-arrba-cai"

# Standard Ledger Fee (10,000 e8s)
export DEFAULT_LEDGER_FEE=10000

# Faucet Canister
export FAUCET_CANISTER="nqoci-rqaaa-aaaap-qp53q-cai"

echo "Loaded common configurations for $NETWORK."
