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

# --- ASSERTIONS ---
assert_eq() {
  local actual="$1"
  local expected="$2"
  local label="$3"
  if [ "$actual" != "$expected" ]; then
    error "ASSERT FAILED ($label): expected $expected, got $actual"
  fi
  log "  ✅ $label: $actual == $expected"
}

assert_gt() {
  local actual="$1"
  local threshold="$2"
  local label="$3"
  if [ "$actual" -le "$threshold" ]; then
    error "ASSERT FAILED ($label): expected > $threshold, got $actual"
  fi
  log "  ✅ $label: $actual > $threshold"
}

# --- IDENTITY HANDLING ---
use_identity() {
  dfx identity use "$1" >/dev/null 2>&1
}

setup_identities() {
  dfx identity use default
  PRINCIPAL_A="$(dfx identity get-principal)"
  dfx identity get-principal --identity secondary &>/dev/null || dfx identity new secondary --storage-mode=plaintext
  PRINCIPAL_B="$(dfx identity get-principal --identity secondary)"

  export PRINCIPAL_A
  export PRINCIPAL_B
  log "Identities: User A ($PRINCIPAL_A), User B ($PRINCIPAL_B)"
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

get_fund_balance() {
  local fund_key="$1"
  local res
  res=$(dfx canister call clearing get_funds)

  echo "$res" | tr '\n' ' ' |
    grep -oE "${fund_key} = vec \{[^}]*\}" |
    grep -oE '[0-9_]+ : nat' |
    head -n1 |
    awk '{print $1}' |
    tr -d '_'
}

get_equity() {
  local target_identity="$1"
  local current_identity
  current_identity=$(dfx identity whoami)

  use_identity "$target_identity"

  local res
  res=$(dfx canister call clearing get_account_state "(record { refresh = null })")

  use_identity "$current_identity"

  echo "$res" | grep -oE 'reserved_margin_usd = [0-9_]+' | awk '{print $3}' | tr -d '_'
}

# --- SETUP UTILS ---

# Generic collateral setup
setup_collateral() {
  local identity="$1"
  local amount="$2"
  local asset_id="$3"
  local ledger_id="$4"
  local domain="${5:-variant { Settlement }}"
  local clearing_id
  local timestamp

  clearing_id=$(dfx canister id clearing)
  timestamp=$(date +%s%N)

  log "Setting up $amount $asset_id collateral for $identity (Ledger: $ledger_id, Domain: $domain)..."
  use_identity "$identity"

  dfx canister call "$ledger_id" icrc2_approve "(record { 
        amount = 1000000000000 : nat; 
        spender = record { owner = principal \"$clearing_id\" } 
    })" >/dev/null

  dfx canister call clearing deposit_collateral "(record { 
        deposit_id = \"DEP_${asset_id}_${timestamp}\"; 
        asset_id = \"$asset_id\"; 
        amount = $amount : nat;
        domain = opt $domain;
    })" >/dev/null
}

setup_icp_collateral() {
  local id
  id=$(dfx canister id icp_ledger 2>/dev/null || echo "$ICP_LEDGER_CANISTER")
  setup_collateral "$1" "$2" "ICP" "$id" "$3"
}

setup_ckusdc_collateral() {
  local id
  id=$(dfx canister id ledger 2>/dev/null || echo "$VUSD_LEDGER")
  setup_collateral "$1" "$2" "ckUSDC" "$id" "$3"
}

register_series_usd() {
  local title="$1"
  local payoff_type="$2"
  local strike="$3"
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
        balance_domain = variant { Settlement };
    })"
}

# --- GLOBAL SETUP ---
init_integration_env() {
  log "Initializing global integration environment..."

  # Ensure network is set
  NETWORK=${NETWORK:-local}
  export NETWORK

  # Source common configs (canisters, symbols)
  source "$(dirname "$0")/init.common.sh"

  # Setup identities
  setup_identities

  # Register Assets if local
  if [[ "$NETWORK" == "local" ]]; then
    log "Local environment detected. Ensuring Assets are registered..."

    # ICP
    ICP_ID=$(dfx canister id icp_ledger 2>/dev/null || echo "$ICP_LEDGER_CANISTER")
    log "  Registering ICP ($ICP_ID)..."
    dfx canister call clearing register_icrc_asset "(record { 
            asset_id = \"ICP\"; 
            ledger_id = principal \"$ICP_ID\";
            haircut_bps = 0 : nat16;
            oracle_id = null;
            is_enabled = true;
        })" 2>/dev/null || true

    dfx canister call clearing update_asset_price "(record { 
            asset_id = \"ICP\"; 
            price = record { decimal = record { value = 10000000 : nat; decimals = $USD_DECIMALS : nat8 }; oracle_id = null; timestamp = null };
        })" >/dev/null

    # ckUSDC (mapped to 'ledger' canister in dfx.json)
    CKUSDC_ID=$(dfx canister id ledger 2>/dev/null || echo "$VUSD_LEDGER")
    if [[ -n "$CKUSDC_ID" ]]; then
      log "  Registering ckUSDC ($CKUSDC_ID)..."
      dfx canister call clearing register_icrc_asset "(record { 
                asset_id = \"ckUSDC\"; 
                ledger_id = principal \"$CKUSDC_ID\";
                haircut_bps = 0 : nat16;
                oracle_id = null;
                is_enabled = true;
            })" 2>/dev/null || true

      dfx canister call clearing update_asset_price "(record { 
                asset_id = \"ckUSDC\"; 
                price = record { decimal = record { value = 1000000 : nat; decimals = $USD_DECIMALS : nat8 }; oracle_id = null; timestamp = null };
            })" >/dev/null
    fi
  fi
}
