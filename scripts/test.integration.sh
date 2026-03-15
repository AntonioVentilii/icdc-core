#!/bin/bash
set -euo pipefail

# --- MASTER INTEGRATION RUNNER ---
# This script orchestrates the entire integration testing suite.

# 1. Load Utilities and Initialize Environment
SCRIPT_DIR="$(dirname "$0")"
source "$SCRIPT_DIR/test.integration.utils.sh"

log "================================================="
log "🚀 Starting Master Integration Suite"
log "================================================="

init_integration_env

# 2. Global Setup (Initialize config, Oracle, etc.)
log "Performing global configuration setup..."
dfx identity use default
dfx canister call clearing update_config "(record { 
    insurance_fund_fee_ratio = (10 : nat16); 
    protocol_fee_ratio = (5 : nat16);
    signer_canister = principal \"$PRINCIPAL_A\"; 
    evm_rpc = principal \"aaaaa-aa\" 
})" || error "Failed to update clearing config"

dfx canister call registry add_authorized_creators "(vec { principal \"$PRINCIPAL_A\" })" >/dev/null
dfx canister call registry add_oracle "(record { 
    oracle_id = \"TRADE_ORACLE\"; 
    metadata = record { 
        name = \"Test Oracle\"; 
        description = opt record { plain = \"Oracle\"; markdown = null; html = null }; 
        website = null 
    }; 
    authorized_principals = vec { principal \"$PRINCIPAL_A\" } 
})" 2>/dev/null || true

# 3. Execute Scenarios
SCENARIOS_DIR="$SCRIPT_DIR/tests"
for scenario in "$SCENARIOS_DIR"/scenario_*.sh; do
  if [[ -f "$scenario" ]]; then
    log ""
    log "-------------------------------------------------"
    log "Running Scenario: $(basename "$scenario")"
    log "-------------------------------------------------"
    source "$scenario" || error "Scenario $(basename "$scenario") failed!"
  fi
done

log ""
log "================================================="
success "All integration scenarios passed successfully!"
log "================================================="
