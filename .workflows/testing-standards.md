---
description: Testing standards for ICDC canisters
---

To maintain economic safety, follow these testing standards:

1. **Unit Tests (Service Level)**:
   - Place in `mod tests` block in each `service.rs` file.
   - Use mock principals and state.
   - **Mandatory coverage**:
     - All validation failure paths (e.g., `InsufficientMargin`).
     - State mutation correctness (equity calculation, balance updates).
     - Decimal scaling logic (especially with mixed decimal assets).

2. **Integration Tests (Script Level)**:
   - Use `scripts/test.integration.sh` as the template.
   - Deploy canisters locally using `dfx start --clean`.
   - Perform end-to-end flows: Deposit -> Trade -> Settlement -> Withdrawal.
   - Verify final balances against expected calculations.

3. **Invariants to Assert**:
   - `equity_usd >= reserved_margin_usd` (Solvency).
   - `total_vault_assets == sum(user_balances)` (Conservation of value).
   - Atomicity: If a complex trade fails, no part of the state should change.

4. **Test Naming Style**:
   - `test_[function_name]_[scenario]` (e.g., `test_execute_trade_insufficient_margin`).

5. **CI & Quality Compliance**:
   - Ensure all tests pass with `cargo test`.
   - Run `npm run quality` (or `npm run quality:rust`) before submitting any change.
   - Fix all `clippy` warnings and formatting issues.
