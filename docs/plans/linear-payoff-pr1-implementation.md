# Implementation Plan — PR 1: `Linear` payoff (bounded forwards / NDFs)

Satisfies the "Plan First" rule in
[`.policies/code-modification.md`](../../.policies/code-modification.md) for
[ADR 0001](../adr/0001-linear-payoff-forwards-ndf-futures.md).

**One logical change**: add a single expiring, cash-settled `Linear` payoff type
(forward = NDF), settled model-free as `net_qty · (S_T − K)`, margined by a
per-series price band so the system stays provably solvent with **no**
liquidation engine. Signed settlement is introduced here because a linear
position can lose. Variation margin / liquidation is **PR 2**, not this PR.

Title: `feat(clearing,registry,shared): add Linear payoff (bounded forwards/NDF)`

## Guiding constraints

- **Reuse existing math** — route Linear through `get_unit_payoff`,
  `get_settlement_value`, `get_required_margin`, `scale_price`, `Price`. No new
  pricing model. No Black–Scholes in-core.
- **Solvency first** ([policy rule 4](../../.policies/code-modification.md)) —
  both legs pre-reserve max loss within the band; long+short PnL nets to zero.
- **Additive state** — new `Series.settlement_cap` is an `opt` field ⇒ no
  migration ([patterns.md §5](../../.agents/patterns.md)); re-verify decode.

## Steps (ordered; each compiles + tests green before the next)

### 1. `shared` — type surface
- [`types/series.rs`](../../src/shared/src/types/series.rs): add
  `PayoffType::Linear`; add `PayoffType::Linear => b"LINEAR"` to `as_id_bytes()`.
- Add `settlement_cap: Option<Price>` to `Series` and `AddSeriesParams` (`opt`).
- Decide `SeriesIdParams` inclusion: fold `settlement_cap` into id generation
  **only when `Some`** so two Linear series differing only by cap get distinct
  ids; existing ids unchanged. (Open question flagged for review.)
- `SettlementInput` unchanged — Linear settles via `Price` (reuses the Call/Put
  settlement path).

### 2. No signature change — reuse the existing signed cashflow
The settlement layer already turns a non-negative gross payout into a signed
cashflow: `cashflow = payout − reserved_margin − fees`
([`api/settlement/api.rs` ~L430](../../src/clearing/src/api/settlement/api.rs)).
Linear reuses this **unchanged** — we define its gross payout (`≥ 0`) and
reserved margin so `payout − reserved = net_qty·(S_T − K)`. `get_settlement_value`
stays `-> u128`. No `i128` refactor.

### 3. `clearing/payoffs/mod.rs` — the three arms (band-bounded, zero-sum)
Let `S* = clamp(S_T, 0, cap)`, all scaled to `USD_DECIMALS` via `scale_price`.
- `get_unit_payoff`: `Linear` ⇒ per-unit **long** gross payout `S*` (requires
  `strike` for the id/margin; requires `settlement_cap` to clamp). Reused by
  `get_settlement_value` exactly like the Binary/Categorical `max − unit` pattern.
- `get_settlement_value`: `Linear` ⇒ `qty ≥ 0 ? |qty|·S* : |qty|·(cap − S*)`.
- `get_required_margin`: `Linear` ⇒ `qty > 0 ? |qty|·K : |qty|·(cap − K)`;
  error if `strike` missing (`MissingStrike`) or `settlement_cap` missing
  (new `PayoffError::MissingSettlementCap`).
- Invariant to test: `long PnL + short PnL == 0` for `S*` inside and outside the
  band (clamp guarantees it).

### 4. `clearing/api/trade/api.rs` — trade-path arms
- Audit the `match series.payoff_type` at
  [`api/trade/api.rs:614`](../../src/clearing/src/api/trade/api.rs) and the
  `!= PayoffType::Categorical` guards (`:977`, `:1077`): `Linear` is scalar
  (non-categorical, no outcomes) — confirm it flows down the scalar path and add
  an explicit `Linear` arm where the match is exhaustive.

### 5. `registry/api/series.rs` — validation
- In `add_series_impl`, add Linear invariants: **require** `strike` (the agreed
  forward rate `K`); **require** `settlement_cap` and `cap > K`; **reject**
  `outcomes`. New `SeriesError` variants:
  `LinearRequiresStrike`, `LinearRequiresSettlementCap`, `LinearRejectsOutcomes`.

### 6. Migrations — re-verify, don't add
- Confirm the `Legacy*` shadows in
  [`src/shared/src/migrations/`](../../src/shared/src/migrations/),
  `src/clearing/src/migrations/`, `src/registry/src/migrations/` still decode
  pre-`Linear` blobs (they never contain the new variant / field). Add a
  round-trip decode test if not already covered.

### 7. Candid
- `npm run did` → regenerate `clearing.did`, `registry.did`. Commit the diff.

## Tests (mirror existing suites)

- **Unit** in `payoffs/mod.rs` (next to `call_payoff`/`put_payoff`/`margin_logic`):
  - `linear_long_profit` (`S_T > K` ⇒ `+`), `linear_long_loss` (`S_T < K` ⇒ `−`),
    `linear_short_mirror`, and **`linear_zero_sum`** (long PnL + short PnL == 0).
  - `linear_margin_band_long`, `linear_margin_band_short`,
    `linear_missing_strike`, `linear_missing_cap`.
- **Registry unit** in `api/series.rs` tests: `linear_requires_strike`,
  `linear_requires_cap`, `linear_rejects_outcomes`, `linear_happy_path`.
- **Integration** in [`clearing/tests/it/engine.rs`](../../src/clearing/tests/it/engine.rs):
  open a `Linear` series, trade long vs short, settle **above** and **below**
  `K`, assert signed account deltas, zero-sum netting, and no solvency violation
  (`SettlementError::SolvencyViolation` never fires within-band).

## Quality gates (from [pr-and-ci.md §5](../ai/pr-and-ci.md))

```bash
npm run quality && npm run quality:rust
npm run test:unit
npm run test:integration          # or: cargo test --test it -p clearing settlement
RUSTFLAGS="-D warnings" cargo test --lib --bins
RUSTFLAGS="-D warnings" cargo test --test it
npm run did
```

## PR body (template — all three sections required)

- **Motivation**: FX linear leg (forward = NDF on a cash-settled CCP); first half
  of ADR 0001. Bounded/solvency-safe; variation margin is a follow-up.
- **Changes**: bullet the files above; note `docs/ai`/`.agents` untouched, ADR +
  plan added, candid regenerated.
- **Tests**: the commands above + which suites were added.

## Explicitly out of scope (→ PR 2)

Variation margin, maintenance margin, liquidation heartbeat, removing the
settlement cap, and the pre-existing naked short-`Call` under-collateralization.

## Open questions for review

1. Include `settlement_cap` in `SeriesIdParams` id generation (recommended:
   yes, when `Some`)?
2. Settlement-price clamp lives in `get_unit_payoff` (Linear) — confirm the
   oracle-outside-band case (`S_T > cap`) settling at `cap` is the desired policy
   (vs. rejecting settlement). Recommended: clamp, for guaranteed solvency.
3. Band policy: per-series `settlement_cap` now; later a per-underlying default?
