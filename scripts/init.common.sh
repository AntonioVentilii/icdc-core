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
export USD_DECIMALS=4

# --- Balance domains for register_icrc_asset.allowed_balance_domains (Candid) ---
# Production-like collateral: Settlement only. Play/test tokens: Playground only.
# App-specific (e.g. Vici XP): ViciXp only — see docs/architecture/balance-domains.md
# Both: assets you explicitly allow in both Settlement and Playground (not vUSD — that is internal_ledger only).
export CLEARING_DOMAINS_SETTLEMENT='vec { variant { Settlement } }'
export CLEARING_DOMAINS_PLAYGROUND='vec { variant { Playground } }'
export CLEARING_DOMAINS_SETTLEMENT_AND_PLAYGROUND='vec { variant { Settlement }; variant { Playground } }'
export CLEARING_DOMAINS_VICI_XP='vec { variant { ViciXp } }'

# ICP (ICRC-1 ledger on the Internet Computer — mainnet principal)
export ICP_SYMBOL="ICP"
export ICP_LEDGER="ryjl3-tyaaa-aaaaa-aaaba-cai"
export ICP_DECIMALS=8
export ICP_HAIRCUT_BPS=500
# USD notional per whole token (USD_DECIMALS=4 → value / 10_000). Oracle can replace later.
export ICP_PRICE_E6=30000 # $3.00

# Vici XP (VXP) — ICRC ledger; default = mainnet id from vici-points/canister_ids.json. Override: VICI_XP_LEDGER=...
export VICI_XP_SYMBOL="VXP"
export VICI_XP_LEDGER="${VICI_XP_LEDGER:-s7ux4-yyaaa-aaaam-qidha-cai}"
export VICI_XP_DECIMALS=8
export VICI_XP_HAIRCUT_BPS=0  # 0% haircut for Playground-only app-specific token
export VICI_XP_PRICE_E6=10000 # $1.00

# vUSD: configured on the clearing canister via install/init `Config.internal_ledger` (build args).
# Do not use register_icrc_asset for vUSD — it is internal accounting, not user collateral.
# vUSD Configuration
VUSD_LEDGER_ID=$(dfx canister id ledger --network "$NETWORK" 2>/dev/null)
if [[ -n "$VUSD_LEDGER_ID" ]]; then
  export VUSD_SYMBOL="vUSD"
  export VUSD_LEDGER="$VUSD_LEDGER_ID"
  export VUSD_DECIMALS=8
  export VUSD_HAIRCUT_BPS=0  # 0% haircut for USD stable
  export VUSD_PRICE_E6=10000 # $1.00
fi

# TESTICP Token Configuration
export TESTICP_SYMBOL="TESTICP"
export TESTICP_LEDGER="xafvr-biaaa-aaaai-aql5q-cai"
export TESTICP_INDEX="qcuy6-bqaaa-aaaai-aqmqq-cai"
export TESTICP_DECIMALS=8
export TESTICP_HAIRCUT_BPS=1000 # 10%
export TESTICP_PRICE_E6=30000   # $3.00

# TICRC1 Token Configuration
export TICRC1_SYMBOL="TICRC1"
export TICRC1_LEDGER="3jkp5-oyaaa-aaaaj-azwqa-cai"
export TICRC1_INDEX="qzre3-3iaaa-aaaai-aqmsa-cai"
export TICRC1_DECIMALS=8
export TICRC1_HAIRCUT_BPS=2500 # 25%
export TICRC1_PRICE_E6=5000    # $0.50

# Shared Minting Account
export TEST_MINTING_ACCOUNT="bnuz2-zaaaa-aaaal-arrba-cai"

# Standard Ledger Fee (10,000 e8s)
export DEFAULT_LEDGER_FEE=10000

# Faucet Canister
export FAUCET_CANISTER="nqoci-rqaaa-aaaap-qp53q-cai"

echo "Loaded common configurations for $NETWORK."
