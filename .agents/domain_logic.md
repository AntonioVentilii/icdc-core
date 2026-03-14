# AI Guidance: Domain Logic

This document defines the strict requirements for core domain logic in the ICDC system, focusing on economic safety and atomicity.

## 1. Atomicity in Trading & Settlement

All operations affecting balances, positions, or margin MUST follow a strict two-phase commit pattern within the canister:

1. **Validation Phase**:
   - Perform all checks (solvency, permissions, inputs).
   - Calculate all deltas and target states.
   - NO mutation of state occurs here.
   - If any check fails, return an error immediately.

2. **Mutation Phase**:
   - Apply all calculated changes to the state.
   - Emit events.
   - insert records into history maps.

> [!CAUTION]
> NEVER perform an asynchronous call (await) between the Validation and Mutation phases if it leaves the system in an inconsistent state.

## 2. Settlement Principles

- **Single Accounting Unit**: All economic truth is settled in `USD` (ghost token/internal accounting).
- **Mark-to-Market**: Trades are executed at the current price, and any PnL from the previous price is realized immediately into the cash balance.
- **Payoff Valuation**: Settlement updates internal accounting balances, NOT physical assets. Asset transfers only occur on deposit or withdrawal.
- **Expiry Flow**:
  - Product expires -> Oracle provides result -> System computes payoff -> Positions closed -> PNL realized in USD accounting.

## 3. Asset Management (Vault Model)

- **No-Swap Principle**: The clearing does not swap or bridge assets. It only holds, values, and transfers the specific collateral assets deposited by users.
- **Vault-Backed Accounting**: Internal balances (USD) represent claims on the total value of assets held in the vault.
- **Deterministic Withdrawals**: Withdrawals are paid out in available vault assets according to a stable-first waterfall policy.

## 4. Internal USD Ledger (vUSD)

The system uses `vUSD` (or `cvUSD`) as a ghost ICRC token for internal accounting. Agents must understand its specific role:

- **Not a Collateral Asset**: Users do NOT deposit `vUSD`. They deposit supported collateral (ICP, BTC, USDC).
- **Realized PnL Ledger**: `vUSD` represents the user's realized PnL and cash claims within the clearing system.
- **Internal Only**: It is a "ghost token" used to simplify indexing and transaction history.
- **Withdrawal Mechanic**: When a user withdraws realized value, the system **burns** the corresponding `vUSD` and transfers an equivalent value of vault-backed assets to the user.

## 5. Consumer Independence

- **Generic Implementation**: Logic should be "stupid-proof" and generic. Do not make tactical choices that depend on a specific consumer (like Vici).
- **Venue Neutrality**: The clearing layer remains neutral; it only accepts valid fills and manages collateral.
- **Pluggable Architecture**: Design for future third-party venues to connect to the clearing layer.
