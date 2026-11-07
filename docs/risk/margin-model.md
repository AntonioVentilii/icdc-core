# Margin model

- **Status**: Draft for approval
- **Scope**: `clearing` (risk engine), `registry` (instrument parameters), `shared` (types)
- **Supersedes**: the full-collateral model in `src/clearing/src/payoffs/mod.rs`, and the `settlement_cap` approach proposed in PR #86

## 1. Why this exists

Today the clearing engine collateralizes every position up front by its maximum possible loss. That is not margin, it is locked collateral, and it breaks down exactly where derivatives matter:

- A short forward or a short call has no finite maximum loss, so it cannot be collateralized at all without inventing an artificial cap.
- Capital efficiency is poor: a participant posts the worst case instead of a realistic close-out loss.
- The short side of `Call` and `Put` is currently accounted with the wrong sign, both at settlement and in the per-trade re-mark (see section 13).

This document defines a risk-based margin model in line with how listed-derivatives central counterparties and Deribit operate: participants post **initial margin** sized to the loss a position could suffer while it is being closed out, at a high confidence level; positions are **marked to market** continuously; accounts that fall below **maintenance margin** are **liquidated**; an **insurance fund** absorbs what liquidation cannot recover.

We do not invent a methodology. We adopt published, industry-standard methods and own only their parameters.

## 2. Principles

1. **Risk based, not loss based.** Margin covers a high-confidence loss over the close-out horizon, not the theoretical maximum.
2. **One method for every instrument.** Positions are revalued under a set of scenarios and margin is taken from the resulting loss distribution. No per-instrument formulas.
3. **Portfolio based.** Positions on the same underlying are revalued together, so offsetting positions (a covered call, a calendar spread) are recognized.
4. **Transparent and reproducible.** Every parameter lives on chain and is queryable. Anyone can recompute any account's margin from public data.
5. **Conservative and anti-procyclical.** Margins must not collapse in calm markets and then spike in a crisis. Volatility has a floor and stress scenarios always apply.
6. **Published methods, our parameters.** Every choice below cites its source (section 15).

## 3. Concepts

| Term                    | Meaning                                                                    |
| ----------------------- | -------------------------------------------------------------------------- |
| Index price `S`         | Spot reference for an underlying, from the oracle                          |
| Mark price              | Fair value of an instrument used for margin and PnL (section 5)            |
| Equity                  | Cash + haircut collateral value + unrealized PnL of open positions at mark |
| Initial margin (IM)     | Equity required to open or increase risk                                   |
| Maintenance margin (MM) | Equity below which the account is liquidated; `MM = mm_ratio x IM`         |
| MPOR                    | Margin period of risk: the time assumed to close out a defaulting account  |
| Variation margin        | Mark-to-market PnL moved into cash                                         |

Account rules:

- A trade that increases risk is accepted only if `equity_after_trade >= IM_after_trade`.
- A trade that only reduces risk is always accepted.
- If `equity < MM`, the account enters liquidation (section 9.4).

## 4. Instrument coverage

| Payoff                                | Margined by this model | Notes                                                                                                                                                                                 |
| ------------------------------------- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Linear` (forward, NDF, dated future) | Yes                    | No settlement cap. Replaces the bounded design of PR #86.                                                                                                                             |
| `Call`, `Put`                         | Yes                    | The buyer pays the premium at trade time and has no further margin. Sellers are margined.                                                                                             |
| `Binary`, `Categorical`               | No, unchanged          | Loss is bounded by one unit per contract, so full collateralization stays correct and simple, as on prediction markets. May move into the scenario engine later (open question 14.1). |

## 5. Mark prices

- **Index `S`**: the existing oracle price. If the price is older than `stale_after`, the underlying is frozen for risk-increasing trades and its margin is computed with `stale_multiplier`.
- **`Linear`**: mark `F = S x (1 + basis)`, with `basis = 0` initially. Carry (the interest-rate differential that separates an FX forward from spot) is a later refinement, either as a configured rate pair or as a basis observed in the order book.
- **`Call`, `Put`**: Black-76 on `F` with the model volatility of section 6 and time to expiry measured in **volatility time** (section 6.3). Implied volatility from the order book may replace the model volatility later.

## 6. Volatility

### 6.1 Base estimate: EWMA

On log returns `r` of the index, sampled at a fixed interval:

```
sigma2_t = lambda x sigma2_(t-1) + (1 - lambda) x r_(t-1)^2
```

Default `lambda = 0.94` for daily sampling (RiskMetrics). EWMA reacts within days to a change in regime while needing no fitting, which keeps it reproducible on chain.

### 6.2 Floor (anti-procyclicality)

```
sigma_used = max(sigma_ewma, floor_ratio x sigma_long_run)
```

`sigma_long_run` is measured over a lookback that includes a stressed period. This mirrors the anti-procyclicality tools required of EU central counterparties (a 25% buffer, a ten-year lookback, or a 25% weight on stressed observations).

### 6.3 Volatility time and event weights

Foreign-exchange option desks do not treat every day as equal. Each future interval `i` carries a weight `w_i`:

- normal trading interval: `w = 1`;
- weekends and holidays: lower (for example `0.3` for FX; crypto trades continuously and keeps `1`);
- scheduled events: higher (for example `2` to `5` for a central-bank rate decision, a payrolls release or an election).

Variance over a horizon is then

```
variance(horizon) = sigma_used^2 x sum(w_i x dt_i)   over the intervals i in the horizon
```

Each underlying has an **event calendar**: a list of `{ time, weight, label }` entries maintained by the risk administrator and stored on chain. When an event enters the margin period of risk, the price scan range widens and margin rises **before** the event; it reverts once the event has passed. The same weights define the volatility time used to price options.

### 6.4 Manual risk multiplier

Each underlying has a multiplier `k >= 1`, applied to the final margin. The risk administrator can raise it immediately for an event nobody scheduled (a sudden headline, a depeg, a market closure). A global multiplier applies to all underlyings at once. Every change emits an event.

### 6.5 Unscheduled jumps

A jump that no calendar announced cannot be forecast. It is covered in layers: stress scenarios that always apply (section 7.3), the volatility floor, the fast reaction of EWMA, the manual multiplier, and finally liquidation plus the insurance fund. This is a stated limitation, not a solved problem.

## 7. Scenario engine

For an account, positions are grouped by underlying. Each group is revalued at mark under every scenario `s`, giving `loss_s = value_now - value_s`. Offsets inside a group are recognized; no offsets are recognized across underlyings at first.

### 7.1 SPAN grid (always computed)

Price scan range and volatility scan range:

```
PSR = S x sigma_used x sqrt(volatility time over MPOR) x z      (z = 2.33 for 99%)
VSR = vsr_ratio x sigma_used
```

The 16 classic SPAN scenarios:

- price moves of `0`, `+-1/3`, `+-2/3` and `+-1` times `PSR`, each combined with volatility up and down by `VSR` (14 scenarios);
- two extreme moves of `+-3` times `PSR`, of which 35% of the loss counts (2 scenarios).

`grid_risk = max(loss_s)` over the 16 scenarios.

A **short option minimum** applies per short option contract (`short_option_min x S`), because a deep out-of-the-money option can show no loss on the grid while still carrying tail risk.

### 7.2 Filtered historical simulation (when history allows)

Once an underlying has at least `hs_min_observations` returns:

1. take the last `hs_lookback` returns over the MPOR horizon, including the stressed period;
2. rescale each one by `sigma_now / sigma_then`, so history is replayed at today's volatility;
3. revalue the group under each rescaled return;
4. take Expected Shortfall at 97.5%, the measure that replaced 99% VaR in the Basel market-risk framework because it looks at the size of the tail and not only its threshold.

### 7.3 Stress scenarios

A fixed list per underlying of historical extremes, for example EUR/CHF on 15 January 2015 or BTC on 12 March 2020, applied as price shocks with weight `stress_weight`.

### 7.4 Margin

```
IM_group   = k x max(grid_risk, hs_risk, stress_weight x stress_risk, short_option_minimum)
IM_account = sum(IM_group) over underlyings
MM_account = mm_ratio x IM_account
```

## 8. Why both SPAN and historical simulation

|                                          | SPAN grid                                    | Filtered historical simulation     |
| ---------------------------------------- | -------------------------------------------- | ---------------------------------- |
| Source of scenarios                      | Stylized moves from volatility               | Real past moves, rescaled to today |
| Data needed                              | Only a volatility estimate                   | A long, clean price history        |
| Captures fat tails and real correlations | Partly, through extreme moves                | Yes                                |
| Transparency                             | Very high, 16 scenarios                      | Lower, hundreds of scenarios       |
| Compute per account                      | Low                                          | Higher                             |
| Used by                                  | CME (historically), Deribit portfolio margin | CME SPAN 2, Eurex Prisma, LCH      |

Taking the maximum of the two, as Eurex Prisma does with its stress component, gives the accuracy of historical simulation where data exists and a transparent floor everywhere, including on day one for a new underlying.

## 9. Lifecycle

### 9.1 Pre-trade

Order placement and execution check `equity_after_trade >= IM_after_trade` at account level. This replaces the per-position `get_required_margin` reservation.

### 9.2 Option premium

The option buyer pays the premium to the seller at trade time. Today neither side pays and both lock collateral; this changes.

### 9.3 Mark-to-market

On a heartbeat (the timer mechanism already used by settlement), every open position is revalued at mark. Unrealized PnL counts in equity immediately; a daily settlement moves it into cash as variation margin. On the Internet Computer the heartbeat processes accounts in bounded batches so each call stays within its instruction limit.

### 9.4 Liquidation

When `equity < MM`:

1. the account is flagged and can only reduce risk;
2. the liquidation engine closes positions incrementally, largest margin contributor first, against the order book within a slippage limit, or through a backstop;
3. a liquidation fee goes to the insurance fund;
4. liquidation stops as soon as `equity >= MM`.

### 9.5 Insurance fund

The existing `INSURANCE_FUND` (already funded by settlement fees) covers any negative equity left after liquidation. What happens when it is exhausted is open question 14.3.

### 9.6 Expiry

The existing settlement pipeline is kept: positions settle against the oracle fixing and their margin is released.

## 10. Parameters

Per underlying, all stored on chain and readable through a query endpoint:

| Parameter             | Default                  | Meaning                                               |
| --------------------- | ------------------------ | ----------------------------------------------------- |
| `lambda`              | 0.94                     | EWMA decay                                            |
| `sampling_interval`   | 1 day                    | Return sampling interval                              |
| `floor_ratio`         | 0.75                     | Volatility floor as a fraction of long-run volatility |
| `long_run_lookback`   | 10 years or full history | Includes a stressed period                            |
| `mpor`                | 2 days                   | Margin period of risk                                 |
| `confidence`          | 99% (`z = 2.33`)         | Grid confidence                                       |
| `vsr_ratio`           | 0.25                     | Volatility scan range as a fraction of volatility     |
| `short_option_min`    | 0.01                     | Per short option contract, as a fraction of `S`       |
| `hs_lookback`         | 2 years                  | Historical simulation window                          |
| `hs_min_observations` | 250                      | Below this, historical simulation is skipped          |
| `stress_weight`       | 0.5                      | Weight of fixed stress scenarios                      |
| `mm_ratio`            | 0.8                      | Maintenance as a fraction of initial                  |
| `k`                   | 1.0                      | Manual risk multiplier                                |
| `event_calendar`      | empty                    | Scheduled events with weights                         |
| `stale_after`         | 15 minutes               | Oracle staleness bound                                |
| `stale_multiplier`    | 1.5                      | Margin multiplier while the oracle is stale           |
| `liquidation_fee`     | 0.5% of notional         | Paid to the insurance fund                            |

Global: `heartbeat_interval`, `global_k`.

Defaults are starting points to be calibrated per underlying (section 11). Only the risk administrator (a controller today) can change them, and every change emits an event.

## 11. Calibration and backtesting

- Daily backtest per underlying: count the days on which the realized loss over the MPOR exceeded IM. At 99% the expected breach rate is 1%; the Kupiec proportion-of-failures test decides whether the observed rate is acceptable.
- Breaches beyond tolerance trigger a parameter review.
- Margin levels are tracked over time to detect procyclical swings.

## 12. Implementation phases

Each phase is its own small, verifiable change.

1. Oracle price history per underlying, EWMA volatility, floor, event calendar and manual multiplier, with storage and admin endpoints.
2. Mark price service: index, `Linear` mark, Black-76 for options in volatility time.
3. Option premium flow, and the short side of `Call`/`Put` corrected.
4. Scenario engine with the SPAN grid and short option minimum; the account-level pre-trade IM check replaces `get_required_margin` for `Linear`, `Call` and `Put`.
5. Heartbeat mark-to-market, MM check, liquidation and insurance fund.
6. Filtered historical simulation and stress scenarios.
7. `Linear` forwards and NDFs on top of the new engine, without `settlement_cap` (the reworked PR #86).
8. Offsets across underlyings.

## 13. Known defect this model removes

In the current code the short side of `Call` and `Put` is accounted like the long side:

- settlement credits a short the option payoff (`payoffs/mod.rs`, the `_ => Ok(abs_qty * unit_payoff)` arm of `get_settlement_value`), so on an in-the-money call both legs are paid;
- a short call's reserved margin uses the same formula as a long's, so the per-trade re-mark credits the short when the premium rises;
- a short put that expires out of the money loses its full strike instead of keeping the premium.

No test exercises this path; the settlement tests set cashflows by hand. Phase 3 fixes it with tests that prove the corrected behavior.

## 14. Open questions

1. Should `Binary` and `Categorical` stay fully collateralized, or move into the scenario engine?
2. Who governs the parameters long term: controllers, an SNS DAO, or a dedicated risk committee role?
3. When the insurance fund is exhausted: socialized loss across profitable accounts, or auto-deleveraging of the most profitable opposite positions?
4. Should carry (interest-rate differential) enter the `Linear` mark from day one for FX underlyings?

## 15. References

- CME Group, _CME SPAN: Standard Portfolio Analysis of Risk_, methodology overview.
- CME Group, _SPAN 2_ framework documentation.
- Eurex Clearing, _Prisma: Portfolio-based Risk Management_, methodology description.
- Commission Delegated Regulation (EU) No 153/2013 (EMIR RTS on requirements for central counterparties), Articles 24 to 28: confidence level, lookback, margin period of risk, anti-procyclicality.
- CPMI-IOSCO, _Principles for Financial Market Infrastructures_ (2012), Principle 6: Margin.
- J.P. Morgan and Reuters, _RiskMetrics Technical Document_ (1996): EWMA volatility.
- Hull, J. and White, A. (1998), "Incorporating volatility updating into the historical simulation method for value-at-risk", _Journal of Risk_.
- Basel Committee on Banking Supervision (2019), _Minimum capital requirements for market risk_: Expected Shortfall at 97.5%.
- Kupiec, P. (1995), "Techniques for verifying the accuracy of risk measurement models", _Journal of Derivatives_.
- Black, F. (1976), "The pricing of commodity contracts", _Journal of Financial Economics_.
- Clark, I. J. (2011), _Foreign Exchange Option Pricing: A Practitioner's Guide_, Wiley: volatility time and event weights.
- Deribit, _Standard Margin_ and _Portfolio Margin_ documentation.
