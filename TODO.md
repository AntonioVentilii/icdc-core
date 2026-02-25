# Clearing

## High Priority
1. **Fix Multi-Asset Margin Model**: Currently `MarginAccount.required_margin` is a single `u128`. It must be per-asset (e.g., `BTreeMap<Asset, u128>`) or otherwise accommodate multi-asset collateral/settlement accurately.
2. **Access Control & Security**:
   - Guard `submit_matched_trade` so it can only be called by authorized exchanges or controllers.
   - Implement pre-check solvency in `settle_series` before initiating any transfers (verify total required from payers and pool capacity).
   - Implement signatures for `PositionProof` in `freeze_position_for_transfer`.
3. **Automation**:
   - Add a cron job (using `ic_cdk_timers`) to periodically register new supported series from the registry `list_series` method.
   - Add a cron job to "refresh" user balances from ledgers periodically.

## Improvements & Optimization
1. **Explicit Fees**: Ensure fees are consistent across all transfer types and clearly documented/logged.
2. **Security Audit**: Perform a full review of the asynchronous flows to ensure no re-entrancy or race conditions remain.
3. **Better Error Variants**: Refine `SettlementError` and `TradeError` to be more descriptive (e.g., for settlement price mismatches).

# Registry

1. **Access Control**: Restrict `add_series` to authorized principals (controllers or via a governance model).
2. **Rate Limiting**: Consider limiting the number of requests per principal to prevent spam.

# Architecture & Ideology (Reference)

### Idempotency - The Pattern (Standardised)
The 3-step pattern implemented in `settle_series`, `deposit_collateral`, and `withdraw_collateral` is the canonical way to handle async transfers:
- **Phase A — Plan**: Deterministic, no awaits, stored in stable state.
- **Phase B — Execute**: Async, resumable, uses ledger idempotency (`created_at_time`).
- **Phase C — Finalise**: Atomic state transition once all transfers are confirmed.

### Key Rules
- **Persist progress** after each successful payment to avoid double-payouts on traps.
- **Ledger-level idempotency** is mandatory.
- **Finalisation must be idempotent** and bring internal accounting into alignment with on-ledger reality.
- **Freeze canonical risk state** (margin accounts) during execution phases where possible.
