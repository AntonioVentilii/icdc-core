# Clearing Engine Canister

The Clearing Engine acts as a Central Counterparty (CCP) for derivative trades. it manages risk, margin, and settlement for all registered series.

## Core Responsibilities

- **Margin Management**: Tracks user collateral balances and calculates maintenance margin requirements.
- **Trade Clearing**: Validates matched trades from exchanges and updates buyer/seller positions.
- **Settlement**: Executes a multi-phase, idempotent settlement process for expiring series.
- **Position Novation**: Enables portable positions between different clearing canisters through a 6-step protocol:
  1. **Request**: User requests a transfer to a new clearing house.
  2. **Freeze**: Source clearing house freezes the position and calculates the final margin state.
  3. **Proof**: Source issues a signed `PositionProof`.
  4. **Validate**: Target clearing house validates the signature and risk parameters.
  5. **Recreate**: Target recreates the position in its internal state.
  6. **Release**: Source releases the collateral once the novation is confirmed.

## Architecture: The Idempotency Pattern

The canister follows a strict **Plan-Execute-Finalise** pattern for all sensitive operations (deposits, withdrawals, and settlements):

1. **Phase A — Plan**: Deterministic validation, no `await` calls, state stored in stable memory.
2. **Phase B — Execute**: Asynchronous ledger interactions using idempotency keys (`created_at_time`).
3. **Phase C — Finalise**: Atomic update of internal accounts once ledger confirmations are received.

## API Overview

- `submit_matched_trade(params)`: Submits a trade for clearing.
- `deposit_collateral(params)` / `withdraw_collateral(params)`: Manages user funds.
- `settle_series(params)`: Initiates settlement for an expired series.
- `freeze_position_for_transfer(params)`: Prepares a position to be moved to another clearing house.

## Current Limitations & Roadmap

- **[MISSING] Authorized Exchanges**: Only authorized exchange principals should be able to submit trades.
- **[MISSING] Signed Proofs**: `PositionProof` signatures are currently unimplemented.
- **[PLANNED] Multi-Asset Margin**: Upgrading `required_margin` to be per-asset instead of a total value.
- **[PLANNED] Automation**: Timers for auto-syncing series and refreshing user balances.
