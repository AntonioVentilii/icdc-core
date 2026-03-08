#!/bin/bash

# --- COLORS ---
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# --- LOGGING ---
log() { echo -e "${BLUE}LOG:${NC} $1"; }
warn() { echo -e "${YELLOW}WARN:${NC} $1"; }
error() {
  echo -e "${RED}ERROR:${NC} $1"
  exit 1
}
success() { echo -e "${GREEN}SUCCESS:${NC} $1"; }

# --- IDENTITY HANDLING ---
use_identity() {
  dfx identity use "$1" >/dev/null 2>&1
}

# --- ACCOUNT QUERIES ---
get_usd_balance() {
  local target_identity="$1"
  local current_identity
  current_identity=$(dfx identity whoami)

  use_identity "$target_identity"

  local res
  res=$(dfx canister call clearing get_account_state "(record { refresh = null })")

  use_identity "$current_identity"

  echo "$res" | grep -oE 'cash_balance_usd = -?[0-9_]+' | awk '{print $3}' | tr -d '_'
}

# --- SETUP UTILS ---
setup_icp_collateral() {
  local identity="$1"
  local amount="$2"
  local clearing_id
  local timestamp

  clearing_id=$(dfx canister id clearing)
  timestamp=$(date +%s%N)

  log "Setting up $amount ICP collateral for $identity..."
  use_identity "$identity"

  dfx canister call icp_ledger icrc2_approve "(record { 
        amount = 10000000000 : nat; 
        spender = record { owner = principal \"$clearing_id\" } 
    })" >/dev/null

  dfx canister call clearing deposit_collateral "(record { 
        deposit_id = \"DEP_${timestamp}\"; 
        asset_id = \"ICP\"; 
        amount = $amount : nat 
    })" >/dev/null
}

register_series_usd() {
  local title="$1"
  local payoff_type="$2" # variant { Binary }
  local strike="$3"      # e.g. "opt record { decimal = record { value = 1000000; decimals = 6 } }"
  local underlying="$4"

  dfx canister call registry add_series "(record {
        title = \"$title\";
        strike = $strike;
        payoff_type = $payoff_type;
        payout_unit = variant { Fiat = variant { Usd } };
        underlying = \"$underlying\";
        expiry_ns = 2_000_000_000_000_000_000 : nat64;
        oracle_source = \"TRADE_ORACLE\";
        description = record { plain = \"$title\"; markdown = null; html = null };
        price_precision = 8;
    })"
}
