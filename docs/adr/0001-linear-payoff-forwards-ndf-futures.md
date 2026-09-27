# ADR 0001 — Linear payoff type for forwards, NDFs, and futures

- **Status**: Proposed
- **Date**: 2026-07-08
- **Deciders**: @antonioventilii
- **Scope**: `shared`, `clearing`, `registry`

## Context

ICDC natively clears four payoff types (`PayoffType` in
[`src/shared/src/types/series.rs`](../../src/shared/src/types/series.rs)):
`Binary`, `Call`, `Put`, `Categorical`. All four are **non-negative** payoffs —
the holder can lose at most their reserved margin. This maps cleanly onto the
engine's current risk model: **reserve the maximum possible loss as margin at
trade time, settle once to a single oracle price at `expiry_ns`, no running
liquidation.**

We want to offer **FX-style linear instruments**: outright forwards,
non-deliverable forwards (NDFs), and (dated) futures. These are the natural
underlying/hedging leg of an FX book and, for NDFs, the dominant product on
non-convertible pairs. A separate Deribit-style demo front-end (a private
sibling repo, not part of this repository) already shows Futures/Forwards tabs,
but they are **synthetic client-side facades** — the clearing engine does not
know about them.

### What "linear" means here

A forward / NDF / dated future all share **one** payoff, and it is
**model-free** (no Black–Scholes, no discounting required for settlement):

```
per-unit PnL, long  =  S_T − K
per-unit PnL, short =  K − S_T
```

where `K` is the agreed forward rate and `S_T` is the oracle fixing at expiry.
Note `K` is the **trade price** at which the position is opened (captured in
reserved margin, like an option premium) — a `Linear` series carries **no**
series-level `strike`. A deliverable forward, an NDF, and a dated future differ
**only** in margining cadence and delivery — none of which changes the payoff:

- On a USD cash-settled CCP there is no delivery leg, so **forward ≡ NDF** here
  (both cash-settle `S_T − K` in USD at the fixing).
- A **dated future** has the _same_ `S_T − K` payoff; it differs from a forward
  only by daily variation margin — a **margining** concern, not a payoff type.
- A **perpetual future** has _no_ `expiry_ns` and needs continuous funding +
  MTM; it does **not** fit the expiry-triggered settlement pipeline and is
  explicitly **out of scope** (see "Rejected alternatives").

Therefore we introduce **one** new payoff type, `Linear`, that expires and
cash-settles. It is simultaneously our forward and our NDF, and it is the
payoff a dated future will later reuse.

## Reuse of existing mathematical models (hard requirement)

Linear settlement must reuse the engine's existing numeric machinery, not
introduce a parallel one. The mapping:

| Concern                     | Existing model reused                                                                                                               | Change for `Linear`                                                                 |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| Per-unit payoff at fixing   | [`get_unit_payoff`](../../src/clearing/src/payoffs/mod.rs)                                                                          | New arm: `S_T − K` **without** the `saturating_sub`-to-zero clamp that Call/Put use |
| Position settlement value   | [`get_settlement_value`](../../src/clearing/src/payoffs/mod.rs)                                                                     | New arm: gross payout `≥ 0` within a `[0, cap]` band (see below)                    |
| Signed PnL / cashflow       | [`api/settlement/api.rs`](../../src/clearing/src/api/settlement/api.rs) `cashflow = payout − reserved_margin − fees` (line ~430)    | **Reused verbatim** — already `i128`; produces the signed PnL for us                |
| Margin at trade time        | [`get_required_margin`](../../src/clearing/src/payoffs/mod.rs)                                                                      | New arm: max loss within a declared price band (see below)                          |
| Decimal / precision scaling | [`scale_price`](../../src/clearing/src/payoffs/utils.rs)                                                                            | Reused verbatim                                                                     |
| Money representation        | `Price` / `DecimalValue` ([`price.rs`](../../src/shared/src/types/price.rs), [`decimal.rs`](../../src/shared/src/types/decimal.rs)) | Reused verbatim                                                                     |
| Equity / reserved margin    | [`types/margin.rs`](../../src/clearing/src/types/margin.rs) (`calculate_equity_usd`, `calculate_raw_equity_i128`, reserved-margin)  | Reused; already `i128`-aware                                                        |
| Settlement lifecycle        | `SettlementInput::Price(Price)` at `expiry_ns`                                                                                      | Reused verbatim — Linear settles to a single oracle `Price`, exactly like Call/Put  |

**No new pricing model is added.** Interest-rate-parity forward pricing
(`F = S · e^{(r_q − r_b)·τ}`) governs the _fair quote / mark_ of a forward, not
its settlement. Quotes are formed by the order book (`list_orders`); the mark is
the oracle price. IRP therefore belongs in the front-end / market-making layer,
**not** in the clearing core, and is out of scope for these PRs.

## How losses are represented (no signature change needed)

`get_unit_payoff` / `get_settlement_value` return **`u128`** and _cannot_
represent a loss — but they don't need to. The settlement layer already derives
a **signed** cashflow from a non-negative gross payout:

```rust
// src/clearing/src/api/settlement/api.rs (~line 430)
let cashflow: i128 = payoff_u128.cast_signed()
    - pos.reserved_margin_usd.cast_signed()
    - i_fee.cast_signed() - p_fee.cast_signed();
```

A position's loss is realised as `payout < reserved_margin ⇒ cashflow < 0`.
This is exactly the mechanism Binary/Categorical already use. **Linear reuses it
unchanged** — we only need to define Linear's _gross payout_ (`≥ 0`) and its
_reserved margin_ so that `payout − reserved` equals the intended signed PnL
`net_qty · (S_T − K)`. No `i128` refactor of the payoff API, no new settlement
plumbing. This is why the change is contained.

## The margin problem and how we bound it

Under the current "pre-reserve max loss, no liquidation" model:

- **Long Linear** max loss = `K` (when `S_T → 0`): **bounded**, fully
  collateralizable as `qty · K`.
- **Short Linear** max loss = **unbounded** (`S_T → ∞`): cannot be
  pre-collateralized with a finite number.

Real markets solve the short side with **maintenance margin + mark-to-market +
liquidation**. That is the correct end state — but it is a risk-core project of
its own, and the repo's policy is incremental, verifiable patches
([`.policies/code-modification.md`](../../.policies/code-modification.md)).

We therefore **sequence** it:

### PR 1 — `Linear` payoff, bounded (solvency-safe, no liquidation)

Add a per-series **settlement price band** used only by `Linear`
(`settlement_cap: Option<Price>`, an `opt` field → no state migration needed,
per [patterns.md §5](../../.agents/patterns.md)). Margin fully pre-reserves the
max loss **within the band**:

```
long  reserves  qty · (K − 0)      = qty · K
short reserves  qty · (cap − K)
```

At settlement the fixing is **clamped to `[0, cap]`** so both legs net to zero
even if the oracle prints outside the band. Gross payouts (fed to the existing
`payout − reserved` cashflow) are then:

```
S* = clamp(S_T, 0, cap)
long  gross payout = |qty| · S*          reserved = |qty| · K   ⇒ PnL = |qty|·(S* − K)
short gross payout = |qty| · (cap − S*)  reserved = |qty|·(cap−K) ⇒ PnL = |qty|·(K − S*)
```

`long PnL + short PnL = 0` for all `S*` — **provably solvent with zero
liquidation engine**, existing model untouched. The instrument is a _capped_
linear forward/NDF, which for FX (wide, realistic bands) is economically fine.
Result: a real forward/NDF you can trade and settle end-to-end. The `cap` doubles
as the margin bound and the settlement clamp; PR 2 removes it.

### PR 2 — variation margin + maintenance margin + liquidation

Replace the band with real-market margining: periodic mark-to-market, a
maintenance-margin threshold, and a liquidation heartbeat. This unbounds the
short leg (removes the cap), unlocks capital efficiency, and — notably — also
fixes the **existing** naked short-`Call`, which today reserves only premium
against unbounded upside ([`get_required_margin`](../../src/clearing/src/payoffs/mod.rs),
`PayoffType::Call` arm). Larger, lands on "how real markets do it."

## Decision

1. Introduce `PayoffType::Linear` — a single, expiring, cash-settled linear
   payoff that serves as **forward, NDF, and (later) dated future**.
2. Settle it model-free as `net_qty · (S_T − K)` using the **existing**
   `get_unit_payoff` / `get_settlement_value` / `scale_price` / `Price` stack,
   made **signed**.
3. Ship in two atomic PRs: **PR 1** bounded (band-collateralized, solvency-safe);
   **PR 2** real variation-margin + liquidation.
4. Do **not** add separate `Future`/`Forward`/`NDF` payoff types (they are the
   same payoff) and do **not** add perpetuals.

## Consequences

**Positive**

- Completes the FX suite: `Call` + `Put` + `Linear`.
- One payoff type, maximal reuse, no new math stack, no pricing model in-core.
- PR 1 is solvency-safe by construction and independently shippable.
- PR 2 generalises margining and repairs a pre-existing short-Call gap.

**Negative / risks**

- PR 1's instrument is _capped_; a mispriced/too-tight band would truncate
  extreme settlements. Mitigation: bands set generously per pair; documented in
  the series `resolution` clause.
- Signed settlement touches the settlement hot path — covered by unit +
  integration tests mirroring the existing `payoffs` and `settlement` suites.
- Adding an enum variant: existing persisted series never contain `Linear`, so
  candid decode of old state is unaffected; the `Legacy*` migration shadows
  ([`src/*/src/migrations/`](../../src/shared/src/migrations/)) decode only
  pre-existing blobs and stay valid. To be re-verified in PR 1.

## Rejected alternatives

- **Separate `Future` payoff type** — a dated future's payoff _is_ the forward's
  (`S_T − K`); the difference is margining. Modelling it as a distinct payoff
  duplicates logic (violates [patterns.md](../../.agents/patterns.md) / the
  "No Redundancy" policy rule).
- **Perpetual futures first** — no `expiry_ns`, requires continuous funding +
  MTM + liquidation and a different lifecycle than the expiry-triggered
  settlement pipeline. Wrong first step.
- **Liquidation engine in the first PR** — correct end state but a large
  risk-core change; deferred to PR 2 to keep PR 1 atomic and solvency-safe.

## References

- [`src/clearing/src/payoffs/mod.rs`](../../src/clearing/src/payoffs/mod.rs) — payoff & margin math
- [`src/shared/src/types/series.rs`](../../src/shared/src/types/series.rs) — `PayoffType`, `Series`, `SettlementInput`
- [`src/clearing/src/api/settlement/api.rs`](../../src/clearing/src/api/settlement/api.rs) — settlement application
- [`docs/plans/linear-payoff-pr1-implementation.md`](../plans/linear-payoff-pr1-implementation.md) — PR 1 implementation plan
- [`.policies/code-modification.md`](../../.policies/code-modification.md), [`docs/ai/pr-and-ci.md`](../ai/pr-and-ci.md)
