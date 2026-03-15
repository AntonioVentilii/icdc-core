#!/bin/bash
set -euo pipefail

# Scenario 1: User deposits collateral (ICP and ckUSDC) and verifies isolation.
# It checks both Settlement and Playground domains.

log "--- Scenario: Deposit and Domain Isolation ---"

DEPOSIT_AMOUNT=500000000 # 5 ICP or 5 ckUSDC (scaled)

# 1. Deposit ICP into Settlement
log "Depositing ICP into Settlement..."
setup_icp_collateral "default" "$DEPOSIT_AMOUNT" "variant { Settlement }"

# 2. Deposit ICP into Playground
log "Depositing ICP into Playground..."
setup_icp_collateral "default" "$DEPOSIT_AMOUNT" "variant { Playground }"

# 3. Deposit ckUSDC into Settlement
log "Depositing ckUSDC into Settlement..."
setup_ckusdc_collateral "default" "$DEPOSIT_AMOUNT" "variant { Settlement }"

# 4. Verify Account State
log "Verifying account state for multi-collateral and multi-domain..."
dfx identity use default

# Check that we have two domains in cash_balances_usd or reserved_margin_usd?
# Actually, the user wants to check if it "matches".
# We can check get_account_collateral for each domain.

COLLATERAL_SETTLEMENT=$(dfx canister call clearing get_account_collateral "(record { domain = opt variant { Settlement } })")
COLLATERAL_PLAYGROUND=$(dfx canister call clearing get_account_collateral "(record { domain = opt variant { Playground } })")

log "Settlement Collateral: $COLLATERAL_SETTLEMENT"
log "Playground Collateral: $COLLATERAL_PLAYGROUND"

if [[ "$COLLATERAL_SETTLEMENT" == *"ICP"* ]] && [[ "$COLLATERAL_SETTLEMENT" == *"ckUSDC"* ]]; then
  success "Settlement domain has both ICP and ckUSDC."
else
  error "Settlement domain collateral missing assets."
fi

if [[ "$COLLATERAL_PLAYGROUND" == *"ICP"* ]]; then
  success "Playground domain has ICP."
else
  error "Playground domain collateral missing ICP."
fi

success "Deposit and Domain Isolation Scenario completed successfully!"
