# ICDC Core Project - Status & Roadmap

## ✅ Completed (Done)

### Architecture & Foundation

- [x] **Multi-Canister Structure**: Separated Registry and Clearing canisters for scalability and neutrality.
- [x] **Shared Type Library**: Centralised `shared` crate for common data structures and constants.
- [x] **Stable Memory Persistence**: Integrated `ic-stable-structures` in all canisters for safe upgrades.
- [x] **Advanced Pagination**: Implemented cursor-based pagination for Registry queries.

### Clearing Engine

- [x] **Idempotency Pattern**: Implemented "Plan-Execute-Finalise" 3-step logic for safe async transfers.
- [x] **Multi-Asset Accounts**: `MarginAccount` supports multiple assets (ledger-based).
- [x] **Trade Execution**: Matched trade submission with margin validation and internal position updates.
- [x] **New Query Endpoints**: Added `get_orders` and `get_trade_history` for user observability.
- [x] **Settlement Infrastructure**: Robust, resumable settlement logic for expiring series.
- [x] **Event Logging**: In-memory event log for all significant state changes.

### Series Registry

- [x] **Series Lifecycle**: Functions for adding, retrieving, and listing derivative series.
- [x] **Deterministic Identifiers**: Canonical `SeriesId` generation to prevent duplicates.
- [x] **Oracle Metadata**: Support for tracking authorized oracles per series.

---

## 🚧 In Progress / Missing (High Priority)

### Access Control & Security

- [x] **Authorized Creators**: Restrict `add_series` to specific principals or roles.
- [ ] **Authorized Exchanges**: Gated `submit_matched_trade` so only trusted exchange canisters can submit trades.
- [ ] **Cryptographic Signatures**: Implement signing for `PositionProof` in `freeze_position_for_transfer` (currently using empty bytes).
- [x] **Pre-check Solvency**: Add comprehensive solvency checks in `settle_series` before starting transfers.

### Margin Logic

- [ ] **Multi-Asset Margin Map**: Refactor `required_margin` from a single `u128` to a per-asset requirement.
- [ ] **Portfolio Margin**: Implement basic portfolio netting across different series of the same underlying.

### Automation

- [ ] **Registry Sync**: Add timers to Clearing canisters to auto-register new series from the Registry.
- [ ] **Balance Refreshers**: Timers to periodically pull/verify balances from external ledgers.

### Tech Debt

- [ ] **Code Cleanup**: Refactor and document complex functions, especially in `settle_series`.
- [ ] **Testing**: Expand unit and integration tests, especially for edge cases in settlement and margin calculations.
- [ ] **Documentation**: Add comprehensive docstrings and external documentation for all public APIs and complex internal logic.
- [ ] **Unbounded Vectors**: Replace all `Vec` with bounded collections to prevent DoS from unbounded growth (e.g., in `EVENTS`).
- [ ] **Pagination for Results**: Implement pagination for `get_orders` and `get_trade_history` to handle large datasets.

---

## 💡 Suggestions & Future Improvements (Roadmap)

### 1. Observability

- [x] **Prometheus Metrics**: Export internal state (open interest, total collateral locked, trade frequency) for monitoring.
- [ ] **Structured Event Sharding**: Move the `EVENTS` log from memory-only to a dedicated archive canister once it reaches a certain size.

### 2. Risk Management

- **[SUGGESTION] Insurance Fund**: Allocate a small fee from settlements to a clearing-wide insurance fund to cover bankruptcies during liquidation.
- **[SUGGESTION] Risk Waterfall**: Implement a structured "risk waterfall" including default funds and mutualised risk sharing.
- **[SUGGESTION] Liquidation Engine**: Implement an automated "backstop" liquidator for accounts that fall below maintenance margin.

### 4. Advanced Collateral Management

- **[SUGGESTION] Third-party Blocking**: Allow authorized external canisters (e.g., Auction engines, Governance, or Bridges) to use the `block_collateral` / `unblock` primitives to lock user funds for specialized workflows.
- **[SUGGESTION] Order-less Liquidity Locking**: Let users manually lock a portion of their collateral as "guaranteed liquidity" to earn a fee share or premium from the clearing engine's insurance fund.
- **[SUGGESTION] Cross-chain Proofs**: Issue signed proofs of blocked collateral that can be verified on other chains/canisters for atomic swaps.

### 5. Delegated Authority

- **[SUGGESTION] User-Authorized Agents**: Implement a "Session Key" or "Agent" pattern where a user can authorize another principal to call `submit_limit_order_for`, `cancel_limit_order_for`, or `block_collateral_for` on their behalf.
- **[SUGGESTION] Scoped Permissions**: Allow users to restrict agents to specific series, maximum order sizes, or maximum total blocked collateral.
