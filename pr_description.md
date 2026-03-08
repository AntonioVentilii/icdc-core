# PR Description: vUSD Integration & USD-Based Settlement Architecture

This PR introduces the `vUSD` (Virtual USD) as the canonical internal accounting token for the ICDC core system. It represents a significant architectural shift from asset-specific settlement to a unified USD-denominated accounting model, enabling more complex multi-asset collateral management and cleaner derivative payoffs.

## Motivation

The previous architecture tied derivative settlement directly to a specific "Settlement Asset" on-chain. This limited flexibility and created complexity in cross-margin accounting.

By introducing `vUSD`:

- **Unification**: All internal accounting (PnL, margin, equity) is now denominated in USD (6 decimals).
- **Decoupling**: The Registry now uses `PayoutUnit` to describe the economic payoff, independent of the Transfer Rail (Asset).
- **Flexibility**: Users can deposit various collateral assets (ICP, ckUSDC, etc.), which are valued in USD to back their positions.
- **Safety**: The "Upfront Collateral" model for fully-collateralised series ensures that the system is always solvent and payoffs are pre-funded.

## Changes

### 1. Unified Internal Accounting (`clearing`)

- **AccountState refactor**: Replaced `MarginAccount` and decoupled `LEDGER_BALANCES`/`MARGIN_ACCOUNTS` with a unified `AccountState`.
- **Global Risk State**: Replaced per-asset `reserved_balances` and `required_margin` with a **unified `reserved_margin_usd`**. This enables a robust cross-margin model where all collateral (adjusted for haircuts) backs the total activity.
- **Cash Balance**: Introduced `cash_balance_usd` (i128) to track realized PnL and credits/debits.
- **Upfront Collateral Model**: Modified `internal_execute_trade` to deduct the full margin cost upfront from the user's `cash_balance_usd`.
- **USD-based Equity**: Implemented `calculate_equity_usd` which aggregates the USD value of all collateral (with haircuts) plus the internal cash balance.
- **Atomicity Audit**: Verified that all state-modifying sequences in trade, orders, and withdrawals are **synchronous** (no `await` points between checking and updating state), ensuring on-chain transactional integrity.

### 2. Settlement API Rewrite

- **Internal Movement**: `settle_series` no longer performs external ledger transfers. It updates internal `cash_balance_usd` for all participants based on the settlement payoff.
- **Global Solvency Checks**: Added an aggregate system check in `settle_series` that verifies total payoffs against total system equity (sum of all accounts) before processing.
- **Internal Fee Distribution**: Implemented an internal revenue model. Settlement fees are now distributed to the existing `TREASURY` and `INSURANCE_FUND` stores, using the `vUSD` key to track internal USD revenue. **Now supports both Insurance and Protocol (Treasury) fees.**
- **Solvency validation**: Added `check_settlement_solvency` to ensure the system is always backed by sufficient aggregate equity before processing payouts.
- **Idiomatic Settlement Architecture**: `settle_series` now uses a chunked, resumable processing model to ensure scalability and avoid instruction limits while maintaining on-chain atomicity for individual participants.
- **Centralized Fee Logic**: Moved all fee calculations to a dedicated `payoffs::fees` utility module for better maintainability and idiomatic reuse.

### 3. vUSD Minter Canister

- **[NEW] `minter` crate**: A dedicated canister that manages the `vUSD` ledger's minting/burning.
- **Authorized callers**: Only the `clearing` canister (or controllers) can request mints, ensuring tight control over the internal USD supply.

### 4. Registry & Shared Types

- **PayoutUnit Integration**: Decoupled `Series` from `SettlementAsset`. It now references a `PayoutUnit` (e.g., USD) which maps to the internal accounting unit.
- **Constants**: Updated `VUSD_LEDGER` and added `USD_DECIMALS` (6).

### 5. API Transparency & Build Improvements

- **Explicit Exports**: Updated `clearing/src/lib.rs` to explicitly re-export all API sub-modules. This ensures that the `export_candid!` macro correctly identifies all handlers for the Candid interface.
- **Cargo Cleanup**: Address redundant imports and added missing `serde` derives for ICRC-1 types in the `minter` crate.

## Tests

### Integration Test: `test.trade.sh`

Successfully verified the full trade life cycle with the new architecture:

1.  **Deposits**: Participants deposited `ICP` collateral.
2.  **Trade execution**: A trade of 10 qty @ 0.55 USD was executed.
    - Buyer paid **5.50 USD** upfront.
    - Seller paid **4.50 USD** upfront.
3.  **Settlement**: Series settled at 1.00 USD.
    - Settlement payout distributed **10.00 USD** to the buyer and **0.00 USD** to the seller.
4.  **Final Verification**:
    - Buyer Net Delta: **+4.49 USD** (after 0.1% insurance fee).
    - Seller Net Delta: **-4.50 USD**.

**Test Output:**

```bash
Default Delta:   4490000 (Expected: 4490000)
Secondary Delta: -4500000 (Expected: -4500000)
✅ TRADE TEST PASSED!
```

---

> [!IMPORTANT]
> This is a **breaking change**. Canisters and clients must update their Candid bindings to reflect the new `AccountState` and internal accounting endpoints.
