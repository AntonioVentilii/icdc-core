# vUSD Internal Ledger Verification

Verified on: 2026-03-16

## Summary

The `vUSD` token serves as the canonical internal accounting unit for the ICDC ecosystem. This document summarizes the verified properties and guarantees of the `vUSD` asset.

## Verified Properties

### 1. Internal Ledger Mapping

- **Finding**: `vUSD` deposits are directly credited to the user's `cash_balance_usd` in the `AccountState`.
- **Code Reference**: [collateral/api.rs](file:///Users/antonio.ventilii/projects/icdc-core/src/clearing/src/api/collateral/api.rs) (Phase C of `deposit_collateral`).
- **Implication**: `vUSD` is not treated as a volatile collateral asset but as realized cash.

### 2. Collateral Restrictions

- **Finding**: While other assets like `ICP` are subject to haircuts (e.g., 2% in tests), `vUSD` (mapped to `cash_balance_usd`) is included in equity calculations at 1:1 value.
- **Code Reference**: [margin.rs](file:///Users/antonio.ventilii/projects/icdc-core/src/clearing/src/types/margin.rs) (`calculate_raw_equity_i128`).
- **Implication**: `vUSD` acts as the risk-free base currency of the clearing system.

### 3. Controller Guarantee

- **Finding**: The `clearing` canister must be a controller of the `vUSD` Ledger to ensure it can manage the accounting unit of the system.
- **Action**: Updated `scripts/init.clearing.sh` to explicitly add the `clearing` canister as a controller if not already present.

## Conclusion

The current implementation aligns with the architectural goal of using `vUSD` as a "virtual" internal ledger for all realized PnL and cash flows.

### 4. Pay-per-trade Margin Model & Equity

- **Finding**: In the "Pay-per-trade" model, required margin is directly deducted from the `vUSD` cash balance (`cash_balance_usd`) and added to the `reserved_margins_usd`.
- **Systematic Fix**: The `total_equity_usd` calculation in `margin.rs` (`calculate_raw_equity_i128`) was updated to explicitly sum `cash_balance_usd` + `reserved_margin_usd`. This ensures that margin deductions do not artificially reduce the user's Total Equity (preventing double-counting / false insolvencies).
- **Test Coverage**: Verified in `tests/it/vusd.rs::complex_worth_power_check` which asserts that total equity remains cleanly isolated to $197M after a trade deduction of $50M.
