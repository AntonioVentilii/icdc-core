# Domain rules

The economic rules every change to the clearing must keep. The full design is [`NEW_ARCHITECTURE.md`](../../NEW_ARCHITECTURE.md); the flows are drawn in [`docs/architecture/flows.md`](../architecture/flows.md); how these rules turn into code is in [`patterns.md`](patterns.md). The design is the direction and the code is what runs: where this page describes the current implementation it was checked against the code, and where the design is not built yet it says so.

## 1. Units and rounding

- **One accounting unit.** Every USD amount (margin, cash, payoff, fee) is an integer in base units of `USD_DECIMALS` (4 decimals, `src/shared/src/constants.rs`). `10_000` is one dollar.
- **Prices** are `Price { value, decimals }` and are scaled to `USD_DECIMALS` by `scale_price` (`src/clearing/src/payoffs/utils.rs:4`) with an explicit `RoundingMode`.
- **Quantities** are whole contracts in an `i128`: positive is long, negative is short. There are no fractional contracts.
- **Rounding always favours the system.** A price used for margin is rounded up (`Ceil`), a price used for a payout is rounded down (`Floor`), a fee is rounded down, a collateral valuation is rounded down. Never round to nearest.
- **Both legs use the same rounded number.** A long reserves `|qty| * ceil(price)` and a short reserves `|qty| * (unit - ceil(price))`, so the two legs of a contract always add up to exactly one unit (or to the cap for `Linear`). For `Linear` the cap is floored in both margin and settlement so the two stay equal (`src/clearing/src/payoffs/mod.rs:250`). A change that rounds one leg differently from the other creates or destroys value.
- **Collateral valuation** is `balance * price * (10_000 - haircut_bps) / 10_000`, scaled to `USD_DECIMALS` and floored, computed in arbitrary precision (`calculate_raw_equity_i128`, `src/clearing/src/types/margin.rs:136`).
- **Amounts on a ledger** are in the asset's own base units (`CollateralAssetConfig::decimals`) and never mix with USD base units without a valuation.

## 2. The account model

Per user and per balance domain, `AccountState` (`src/clearing/src/types/margin.rs:35`) holds three numbers:

- `balances`: collateral assets deposited, in each asset's base units.
- `cash_balances_usd`: realised PnL, signed. It can be negative.
- `reserved_margins_usd`: margin locked by open positions and resting limit orders.

**Equity** is `cash + sum of valued collateral`. **Available margin** is `equity - reserved`. Opening a position or resting an order does not move cash: it raises `reserved`. Unrealised PnL is not part of equity.

A position (`Position`, same file) carries its own `reserved_margin_usd`: the margin at the price it was last traded at, which is also its cost basis. The account's reserved margin for a domain is meant to equal the sum of its positions' margins plus the `blocked_margin_usd` of its resting orders.

## 3. Atomicity

Every operation that affects balances, positions or margin follows a strict two-phase pattern inside one message:

1. **Validation.** Run every check (solvency, permissions, inputs) and compute every delta and target state. Nothing is mutated. If a check fails, return the error at once.
2. **Mutation.** Apply all the computed changes, emit the events, insert the history records.

Never perform an `await` between validation and mutation. If the operation fails, no part of the state changes.

Operations that must talk to another canister cannot be atomic, so they are made resumable instead, each with its own rule for what happens when a call fails half way:

| Operation                  | Order                                                                                                                                            | If a call fails                                                                                                                                                                      |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Trade, order, mint, redeem | awaits first (series from the registry, trading access), then validate and mutate in one message                                                 | Nothing was mutated before the await, so there is nothing to undo. State is re-read after the await (the order may have changed, the account may have been reassigned).              |
| Deposit                    | plan, then `icrc2_transfer_from` into the user's custody subaccount, then credit the internal balance                                            | The balance is credited only once a ledger receipt is stored. A failed call leaves the plan `Executing`; the caller retries with the same `deposit_id` and the ledger deduplicates.  |
| Withdrawal                 | plan, check equity against reserved margin, debit the internal balance, then `icrc1_transfer` out of the custody subaccount                      | The debit is refunded when the transfer returns an error. The caller retries with the same `withdrawal_id`.                                                                          |
| Treasury withdrawal        | plan, deduct the fund, then transfer from the canister's main account                                                                            | The deduction stays; a controller retries with the same `request_id` or calls `cancel_fund_withdrawal` to refund it.                                                                 |
| Settlement                 | one message removes the positions, cancels the series' orders and stores the plan; cashflows are then applied 100 positions at a time on a timer | No call to another canister after authorization. An interrupted run resumes from `accounting_cursor`: automatically after an upgrade and every 5 minutes, or by `resume_settlement`. |
| Account reassignment       | re-key the internal state in one message, then sweep each custody subaccount                                                                     | The plan and the locks stay until the sweep finishes; a retry resumes it. See [`patterns.md`](patterns.md#moving-user-state-between-principals).                                     |

Three things follow for anyone changing these flows:

- **A bounded-wait call can fail without telling you whether it ran.** Treat a transport error as "unknown", not as "did not happen". The ledger's `created_at_time` deduplication is what makes a retry safe, so a retry must reuse the plan's `idempotency_ns` and never stamp a new time. Deduplication only lasts for the ledger's transaction window (24 hours by default).
- **Only settlement resumes by itself.** Deposits, withdrawals and treasury withdrawals wait for their caller to retry.
- **Between settlement chunks the state is intermediate on purpose.** An account whose chunk has not run yet still has its margin reserved, so it cannot withdraw what it is about to lose. Keep that property: release margin and apply the cashflow for an account in the same step.

## 4. Order of operations in a trade

`execute_trade_impl` (`src/clearing/src/trade/service.rs:39`) is the only place a trade changes state. In order:

1. Return `Ok` if `trade_id` was already executed (idempotency); reject a self-trade.
2. For buyer and seller, compute the margin of the **new** net position at the trade price.
3. Mark the **existing** position to the trade price: the difference between its stored margin (cost basis) and its margin at the trade price is realised into cash at once. Only the two parties of the trade are marked; other holders of the series keep their cost basis until they trade or the series settles.
4. Compute the change in the account's reserved margin: new position margin, minus old position margin, minus the margin a filled limit order had blocked.
5. Validate both accounts. Both must be able to carry their new reserved margin with their equity **after** the cash realised in step 3. A failure on either side rejects the whole trade.
6. Mutate: cash and reserved margin of both accounts, both positions, two `Executed` events, the price-history index, the `EXECUTED_TRADES` marker.

A limit order blocks its full margin when it is placed. A market order re-reads the resting order after its awaits, fills `min(requested, remaining)`, and releases exactly the filled share of the blocked margin by recomputing the margin of what remains with the same formula (`submit_market_order_impl`, `src/clearing/src/api/trade/api.rs:196`).

## 5. Settlement

- **Who.** A controller, or a principal the registry authorizes for the series' `oracle_source`. Authorization runs on every call, including resumes through `settle_series`.
- **Payoff per contract** (`get_unit_payoff`, `src/clearing/src/payoffs/mod.rs:41`): `Binary` pays the settlement price scaled to USD and floored, so its settlement price must be between 0 and 1; `Categorical` pays one unit on the resolved outcome and zero on the others; `Linear` pays the fixing clamped to `[0, cap]`. A series in the `Social` domain pays zero. `Call` and `Put` exist in `PayoffType` but are not full-collateral products like the other three: ask the owner before listing, trading or changing them.
- **Gross payout** (`get_settlement_value`, `:183`) for `Binary`, `Categorical` and `Linear`: a long receives `|qty| * unit_payoff`, a short receives `|qty| * (unit - unit_payoff)` (or `cap - unit_payoff` for `Linear`). Long plus short is one unit (or the cap) per contract.
- **Fees.** Two fees are taken from every gross payout, each floored: the protocol fee and the insurance fee, in basis points from `Config` (`calculate_settlement_fee`, `src/clearing/src/payoffs/fees.rs:8`). They are credited to `TREASURY` and `INSURANCE_FUND` under the internal asset id, once, when the last chunk has been applied.
- **Cashflow.** Each position realises `payout - reserved_margin - fees` into cash and releases its reserved margin. This is the mark to market of step 3 in a trade, done one last time against the settlement value.
- **Solvency check.** Before anything is mutated, the sum of the positive cashflows must not exceed the domain's total equity after the cashflows, with each account floored at zero (`check_settlement_solvency`, `src/clearing/src/api/settlement/api.rs:594`). The check is on the aggregate of the domain, not per account.
- **After.** Positions and resting orders of the series are gone, the plan stays forever as the record, and any plan (in any status) bars new trading on the series (`assert_series_not_settled`).
- **No assets move.** Settlement changes internal numbers only. Assets move on deposit and withdrawal.

## 6. Assets, custody and vUSD

- **No swaps.** The clearing never swaps or bridges assets. It holds, values and transfers the collateral users deposited.
- **Custody is per user.** Each user's collateral sits in its own ledger subaccount of the clearing canister, derived from the principal (`derive_user_subaccount_for_canister`, `src/clearing/src/utils/account.rs:12`). A deposit moves tokens into it; a withdrawal moves them out of it. One subaccount backs all of the user's balance domains for that asset.
- **The ledger fee of a withdrawal comes out of the subaccount on top of the amount**, and the internal balance is debited by the amount only. So the ledger balance of a subaccount is the sum of the user's internal balances for that asset minus the fees past withdrawals paid: never more. Code that moves custody derives the amount from the ledger, not from internal balances.
- **Withdrawals pay the asset the user asks for, from the user's own subaccount**, after checking that equity without it still covers reserved margin (`withdraw_collateral`, `src/clearing/src/api/collateral/api.rs:171`). The stable-first withdrawal waterfall and the pooled vault of the design are not in the code.
- **Realised PnL has no payout path yet.** A positive cash balance raises equity and so frees more of the user's own collateral; a negative one locks it. No code pays a positive cash balance out in assets, and none moves collateral from one user's subaccount to another's. Do not write code that assumes either exists.
- **vUSD** is the internal accounting asset (`Config::internal_ledger`). It cannot be deposited or withdrawn as collateral (`:58`, `:188`); the settlement fees are booked under its asset id. Burning `vUSD` against vault assets is the design in [`NEW_ARCHITECTURE.md`](../../NEW_ARCHITECTURE.md), not what the code does today.
- **Funds.** The treasury can be withdrawn by a controller from the canister's main account; the insurance fund cannot be withdrawn at all (`withdraw_fund`, `src/clearing/src/api/admin/api.rs`).
- The clearing canister must be a controller of the `vUSD` ledger (`scripts/init.clearing.sh` adds it). The minter sends `vUSD` from its own account for authorised callers; the clearing does not call it.

## 7. Positions stay inside the clearing

ICDC is a Central Counterparty: positions are internal, never tokenised, so the clearing can net portfolios and compute margin across them. Positions move between clearings only through the controlled novation flow (`freeze_position_for_transfer`, `accept_position_transfer`), which is controller-only and whose proofs are not signed yet. Background: the CCP section of [`README.md`](../../README.md).

## 8. Invariants

Assert these in tests (see [`testing.md`](testing.md)) and never break them in code.

- **Solvency at every check point.** After placing an order, executing a trade, minting a complete set or withdrawing, the account's `equity >= reserved_margin` in that domain, at current asset prices. It is a check-point rule, not a continuous one: a collateral price can fall between check points, and there is no liquidation engine yet.
- **Full collateral per contract.** Long margin plus short margin is exactly one unit per contract (the cap for `Linear`), and so is long payout plus short payout.
- **Zero sum.** Over the life of a series, the cash realised by all accounts (in trades and at settlement) plus the settlement fees adds up to zero. No operation may realise cash on one account without the matching entry on another account or on a fund.
- **Margin bookkeeping.** An account's reserved margin in a domain equals the margins of its positions plus the blocked margins of its resting orders in that domain. Every path that changes one side changes the other in the same message.
- **Custody.** For each user and asset, the ledger subaccount holds the sum of the internal balances minus the withdrawal fees already paid. An internal balance never exceeds what a deposit put on the ledger.
- **Idempotency.** Replaying a call with the same id (`trade_id`, `order_id`, `deposit_id`, `withdrawal_id`, `request_id`, `transfer_id`, `reassignment_id`, a settlement of the same series with the same input) changes nothing the second time.
- **Atomicity.** A failed single-message operation leaves no partial state; a multi-message one leaves a plan that resumes.

## 9. Upgrades

- The whole state of each canister lives on the heap and is serialised with Candid into stable memory in `pre_upgrade`, then decoded in `post_upgrade` (`src/clearing/src/memory.rs:140`, `:203`). Nothing uses `ic-stable-structures`.
- If either hook traps, the upgrade is rolled back and the canister keeps running the old code with its state intact. That is the safety net, and also the limit: the state, including the append-only `EVENTS` log, must stay small enough to be saved and restored inside one message each.
- A change to the shape of any saved type follows [`migrations.md`](migrations.md). The clearing's `SERIES` cache is saved too, so a `Series` change migrates in both canisters.
- Timers do not survive an upgrade: `post_upgrade` re-drives unfinished settlements and re-arms the heartbeat (`src/clearing/src/lib.rs`). Derived indexes (trade history, traded volume, leaderboard, reassignment locks) are rebuilt from the saved state, not saved.
- An in-flight deposit, withdrawal or reassignment survives as its plan and waits for a retry.

## 10. Consumer independence

- **Generic.** Logic is generic and hard to misuse. No tactical choice that depends on one consumer (such as Vici).
- **Venue neutral.** The clearing accepts valid fills and manages collateral; it does not take a venue's side.
- **Pluggable.** Design so third-party venues can connect to the clearing later.
