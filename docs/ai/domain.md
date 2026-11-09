# Domain rules

The economic rules every change to the clearing must keep. The full design is [`NEW_ARCHITECTURE.md`](../../NEW_ARCHITECTURE.md); the flows are drawn in [`docs/architecture/flows.md`](../architecture/flows.md); how these rules turn into code is in [`patterns.md`](patterns.md).

## 1. Atomicity in trading and settlement

Every operation that affects balances, positions or margin follows a strict two-phase pattern inside the canister:

1. **Validation.** Run every check (solvency, permissions, inputs) and compute every delta and target state. Nothing is mutated. If a check fails, return the error at once.
2. **Mutation.** Apply all the computed changes, emit the events, insert the history records.

Never perform an `await` between validation and mutation if that can leave the system inconsistent. If a complex operation fails, no part of the state changes.

## 2. Settlement

- **One accounting unit.** All economic truth is settled in USD, the internal `vUSD` unit (`USD_DECIMALS` in `src/shared/src/constants.rs`).
- **Mark to market.** A trade executes at the current price, and the PnL from the previous price is realised into the cash balance at once.
- **Payoff valuation.** Settlement updates internal balances, never physical assets. Assets move only on deposit and withdrawal.
- **Expiry flow.** The series expires, the oracle provides the result, the clearing computes the payoff, closes the positions and realises the PnL in USD.

## 3. Assets: the vault model

- **No swaps.** The clearing never swaps or bridges assets. It holds, values and transfers the collateral users deposited.
- **Vault-backed accounting.** Internal USD balances are claims on the total value of the assets in the vault.
- **Deterministic withdrawals.** Withdrawals are paid in available vault assets by a stable-first waterfall.

## 4. The internal USD ledger (vUSD)

`vUSD` is an ICRC "ghost" token used for internal accounting.

- **Not collateral.** Users do not deposit `vUSD`; they deposit supported collateral (ICP, BTC, USDC).
- **Realised PnL and cash.** `vUSD` is the user's realised PnL and cash claim. The cash balance does not decrease when a position opens or a limit order rests: the collateral is locked in `reserved_margin_usd`, and available margin is `equity - reserved`.
- **Realisation.** A trade realises PnL into the `vUSD` balance by comparing the previous cost basis to the value at the trade price.
- **Withdrawal.** `withdraw_collateral` debits the user's balance of the requested collateral asset, after checking that equity stays above reserved margin, and transfers that asset (`src/clearing/src/api/collateral/api.rs:171`). The internal asset itself can be neither deposited nor withdrawn (`:58`, `:188`). Burning `vUSD` against vault assets is the design in [`NEW_ARCHITECTURE.md`](../../NEW_ARCHITECTURE.md), not what the code does today.
- The clearing canister must be a controller of the `vUSD` ledger (`scripts/init.clearing.sh` adds it). Verified properties: [`internal_ledger_verification.md`](internal_ledger_verification.md).

## 5. Positions stay inside the clearing

ICDC is a Central Counterparty: positions are internal, never tokenised, so the clearing can net portfolios and compute margin across them. Positions move between clearings only through the controlled novation flow (`freeze_position_for_transfer`, `accept_position_transfer`). Background: the CCP section of [`README.md`](../../README.md).

## 6. Invariants

Assert these in tests (see [`testing.md`](testing.md)) and never break them in code:

- **Solvency:** `equity_usd >= reserved_margin_usd` for every account.
- **Conservation of value:** `total_vault_assets == sum(user_balances)`.
- **Atomicity:** a failed operation leaves no partial state.

## 7. Consumer independence

- **Generic.** Logic is generic and hard to misuse. No tactical choice that depends on one consumer (such as Vici).
- **Venue neutral.** The clearing accepts valid fills and manages collateral; it does not take a venue's side.
- **Pluggable.** Design so third-party venues can connect to the clearing later.
