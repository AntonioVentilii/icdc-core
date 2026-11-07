# Margin model

- **Status**: Draft for approval
- **Scope**: `clearing` (risk engine), `registry` (instrument parameters), `shared` (types)
- **Supersedes**: the full-collateral model in `src/clearing/src/payoffs/mod.rs`, and the `settlement_cap` approach proposed in PR #86

## 1. Why this exists

Today the clearing engine collateralizes every position up front by its maximum possible loss. That is not margin, it is locked collateral, and it breaks down exactly where derivatives matter:

- A short forward or a short call has no finite maximum loss, so it cannot be collateralized at all without inventing an artificial cap.
- Capital efficiency is poor: a participant posts the worst case instead of a realistic close-out loss.
- The short side of `Call` and `Put` is currently accounted with the wrong sign, both at settlement and in the per-trade re-mark (see section 14).

This document defines a risk-based margin model in line with how listed-derivatives central counterparties and Deribit operate: participants post **initial margin** sized to the loss a position could suffer while it is being closed out, at a high confidence level; positions are **marked to market** continuously; accounts that fall below **maintenance margin** are **liquidated**; an **insurance fund** absorbs what liquidation cannot recover.

We do not invent a methodology. We adopt published, industry-standard methods and own only their parameters.

## 2. Principles

1. **Risk based, not loss based.** Margin covers a high-confidence loss over the close-out horizon, not the theoretical maximum.
2. **One method for every instrument.** Positions are revalued under a set of scenarios and margin is taken from the resulting loss distribution. No per-instrument formulas.
3. **Portfolio based.** Positions on the same underlying are revalued together, so offsetting positions (a covered call, a calendar spread) are recognized.
4. **Transparent and reproducible.** Every parameter and every historical series lives on chain and is queryable. Anyone can recompute any account's margin from public data.
5. **Conservative and anti-procyclical.** Margins must not collapse in calm markets and then spike in a crisis. Volatility has a floor and stress scenarios always apply.
6. **Published methods, our parameters.** Every choice below cites its source (section 16).

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
- If `equity < MM`, the account enters liquidation (section 10.4).

## 4. Instrument coverage

| Payoff                                | Margined by this model | Notes                                                                                                                                                                                 |
| ------------------------------------- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Linear` (forward, NDF, dated future) | Yes                    | No settlement cap. Replaces the bounded design of PR #86.                                                                                                                             |
| `Call`, `Put`                         | Yes                    | The buyer pays the premium at trade time and has no further margin. Sellers are margined.                                                                                             |
| `Binary`, `Categorical`               | No, unchanged          | Loss is bounded by one unit per contract, so full collateralization stays correct and simple, as on prediction markets. May move into the scenario engine later (open question 15.1). |

Every contract on ICDC settles in cash in USD, so for us any currency forward is economically an NDF. Whether a currency is "NDF" in the wider market only matters for where its fixing comes from and for where offshore demand is concentrated.

## 5. Mark prices

- **Index `S`**: the oracle price. If it is older than `stale_after`, the underlying is frozen for risk-increasing trades and its margin is computed with `stale_multiplier`.
- **`Linear`**: mark `F = S x (1 + basis)`, with `basis = 0` initially. Carry (the interest-rate differential that separates an FX forward from spot) is a later refinement, either as a configured rate pair or as a basis observed in the order book.
- **`Call`, `Put`**: Black-76 on `F` with the model volatility of section 6 and time to expiry measured in **volatility time** (section 6.3). Implied volatility from the order book may replace the model volatility later.

## 6. Volatility

### 6.1 Base estimate: EWMA

On log returns `r` of the index, sampled at a fixed interval:

```
sigma2_t = lambda x sigma2_(t-1) + (1 - lambda) x r_(t-1)^2
```

Default `lambda = 0.94` for daily sampling (RiskMetrics). Recent returns weigh more, with weights decaying exponentially. EWMA reacts within days to a change in regime while needing no fitting, which keeps it reproducible on chain. It is also the volatility used to rescale history in section 7.1.

### 6.2 Floor (anti-procyclicality)

```
sigma_used = max(sigma_ewma, floor_ratio x sigma_long_run)
```

`sigma_long_run` is measured over a lookback that includes a stressed period. This mirrors the anti-procyclicality tools required of EU central counterparties (a 25% buffer, a ten-year lookback, or a 25% weight on stressed observations).

### 6.3 Volatility time and event weights

Foreign-exchange option desks do not treat every day as equal. Each future interval `i` carries a weight `w_i`:

- normal trading interval: `w = 1`;
- weekends and holidays: `weekend_weight` (for example `0.3` for FX; crypto trades continuously and keeps `1`);
- scheduled events: higher (for example `2` to `5` for a central-bank rate decision, a payrolls release or an election).

Variance over a horizon is then

```
variance(horizon) = sigma_used^2 x sum(w_i x dt_i)   over the intervals i in the horizon
```

Each underlying has an **event calendar**: a list of `{ time, weight, label }` entries maintained by the risk administrator and stored on chain. When an event enters the margin period of risk, margin rises **before** the event; it reverts once the event has passed. The same weights define the volatility time used to price options. For a currency pair, the calendar is the union of both currencies' calendars (an ECB decision moves EUR/USD and EUR/JPY alike).

### 6.4 Manual risk multiplier

Each underlying has a multiplier `k >= 1`, applied to the final margin. The risk administrator can raise it immediately for an event nobody scheduled (a sudden headline, a depeg, a market closure). A global multiplier applies to all underlyings at once. Every change emits an event.

### 6.5 Unscheduled jumps

A jump that no calendar announced cannot be forecast. It is covered in layers: stress scenarios that always apply (section 7.3), the volatility floor, the fast reaction of EWMA, the manual multiplier, and finally liquidation plus the insurance fund. This is a stated limitation, not a solved problem.

## 7. Scenario engine

For an account, positions are grouped by underlying. Each group is revalued at mark under every scenario `s`, giving `loss_s = value_now - value_s`. Offsets inside a group are recognized; no offsets are recognized across underlyings at first.

### 7.1 Filtered historical simulation (primary measure)

1. Take the last `hs_lookback` returns of the index over the MPOR horizon, including a stressed period.
2. Rescale each return by `sigma_now / sigma_then` (both EWMA, section 6.1), so history is replayed at today's volatility. Event weights in the coming MPOR scale `sigma_now` up, as in section 6.3.
3. Revalue the group under each rescaled return.
4. Take the tail measure set by `risk_measure`: by default Expected Shortfall at 97.5%, the measure that replaced 99% VaR in the Basel market-risk framework because it measures the size of the tail and not only its threshold. VaR at 99% is the alternative setting.

This requires at least `hs_min_observations` returns. The historical backfill of section 9.2 provides them from day one for every underlying in section 9.1.

### 7.2 SPAN grid (floor)

Price scan range and volatility scan range:

```
PSR = S x sigma_used x sqrt(volatility time over MPOR) x z      (z = 2.33 for 99%)
VSR = vsr_ratio x sigma_used
```

The 16 classic SPAN scenarios:

- price moves of `0`, `+-1/3`, `+-2/3` and `+-1` times `PSR`, each combined with volatility up and down by `VSR` (14 scenarios);
- two extreme moves of `+-3` times `PSR`, of which 35% of the loss counts (2 scenarios).

`grid_risk = max(loss_s)` over the 16 scenarios. The grid protects against a history that has been too calm, and it is the only measure for an underlying whose history is still shorter than `hs_min_observations`.

A **short option minimum** applies per short option contract (`short_option_min x S`), because a deep out-of-the-money option can show no loss in any scenario while still carrying tail risk.

### 7.3 Stress scenarios

A fixed list per underlying of historical extremes applied as price shocks with weight `stress_weight`, for example:

- EUR/CHF on 15 January 2015, when the Swiss National Bank removed the 1.20 floor;
- BTC on 12 March 2020;
- the official ARS rate in December 2023 (section 9.4).

### 7.4 Margin

```
IM_group   = k x max(hs_risk, grid_risk, stress_weight x stress_risk, short_option_minimum)
IM_account = sum(IM_group) over underlyings
MM_account = mm_ratio x IM_account
```

## 8. Why historical simulation leads and SPAN is the floor

|                                          | Filtered historical simulation     | SPAN grid                                    |
| ---------------------------------------- | ---------------------------------- | -------------------------------------------- |
| Source of scenarios                      | Real past moves, rescaled to today | Stylized moves from volatility               |
| Data needed                              | A long, clean price history        | Only a volatility estimate                   |
| Captures fat tails and real correlations | Yes                                | Partly, through extreme moves                |
| Transparency                             | Lower, hundreds of scenarios       | Very high, 16 scenarios                      |
| Compute per account                      | Higher                             | Low                                          |
| Used by                                  | CME SPAN 2, Eurex Prisma, LCH      | CME (historically), Deribit portfolio margin |

Historical simulation is the current standard of the large central counterparties and captures the real shape of the tails, so it is the primary measure. Taking the maximum with the SPAN grid, as Eurex Prisma does with its stress component, keeps a transparent floor that still holds when recent history has been unusually quiet.

## 9. Underlyings and historical data

### 9.1 Initial list

Currencies are stored as one series each against USD. Any cross is derived from them, for example `GBP/JPY = GBP/USD x USD/JPY`, so the 28 crosses among the G10 currencies need no history of their own. Crosses are only consistent if both legs come from the same source and fixing time, which is why every FRED currency below uses the same daily fix.

| Group         | Underlyings                                | History source                                                                            | Notes                                                                 |
| ------------- | ------------------------------------------ | ----------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| Crypto        | BTC, ETH, SOL, ICP (vs USD)                | Coinbase Exchange public daily candles (USD pairs); Binance (USDT pairs) as a cross-check | Continuous calendar                                                   |
| G10           | EUR, GBP, JPY, CHF, AUD, CAD, NZD (vs USD) | Federal Reserve H.10 daily rates via FRED; ECB reference rates as a cross-check           | All 28 crosses derived                                                |
| Asia          | CNY, KRW, TWD, INR, SGD (vs USD)           | Federal Reserve H.10 via FRED                                                             | KRW, TWD, INR and CNY trade offshore as NDFs; see 9.3 for CNY and SGD |
| Latin America | BRL, MXN (vs USD)                          | Federal Reserve H.10 via FRED                                                             | BRL trades offshore as an NDF                                         |
| Latin America | CLP (vs USD)                               | Banco Central de Chile, "dólar observado" (also the NDF fixing)                           | Different fixing time; quoted against USD only                        |
| Latin America | ARS (vs USD)                               | Banco Central de la República Argentina, official wholesale rate (Communication A 3500)   | Special regime, section 9.4                                           |

FRED series identifiers: `DEXUSEU` (EUR), `DEXUSUK` (GBP), `DEXJPUS` (JPY), `DEXSZUS` (CHF), `DEXUSAL` (AUD), `DEXCAUS` (CAD), `DEXUSNZ` (NZD), `DEXCHUS` (CNY), `DEXKOUS` (KRW), `DEXTAUS` (TWD), `DEXINUS` (INR), `DEXSIUS` (SGD), `DEXBZUS` (BRL), `DEXMXUS` (MXN). Some are quoted as USD per unit and others as units per USD; the loader normalizes every series to USD per unit of currency. All identifiers, and the ARS and CLP sources, are confirmed against the providers before the backfill runs.

Excluded for now:

- **HKD**: hard peg to USD. Its volatility is near zero and its real risk is a break of the peg, which only a stress scenario can express. It can be added on that basis later.
- **CNH** (offshore yuan): no free official history on the same fixing as the other currencies.

### 9.2 Loading and provenance

1. **Backfill, once.** An off-chain script downloads each series from its source, normalizes it and uploads it in chunks through a controller-only endpoint. Each upload records the source, the series identifier, the first and last date, and a SHA-256 hash of the canonical data, so anyone can re-download from the public source and check what the canister holds.
2. **Then, on chain.** After the backfill the canister samples its own oracle once per `sampling_interval` and appends to the series.
3. **Gaps.** FX holidays are not filled in. Returns are computed between consecutive observations and weighted by the volatility time that elapsed between them.
4. **Later.** The Internet Computer's Exchange Rate Canister may become an on-chain source once its historical depth has been verified.

Footprint: ten years of daily data is about 3,650 values per series.

### 9.3 Calendars and managed regimes

- **Crypto** trades continuously: every day has weight 1.
- **FX** is closed from Friday evening to Sunday evening New York time: weekends and holidays carry `weekend_weight`.
- **CHF** was held at a 1.20 floor against the euro from September 2011 to 15 January 2015. That period understates CHF volatility, so the floor and the January 2015 stress scenario dominate it.
- **CNY** trades within a band around the People's Bank of China daily central parity, and **SGD** within the Monetary Authority of Singapore policy band. Both get a higher `floor_ratio` and mandatory stress scenarios, and their central-bank decisions go into the event calendar.

### 9.4 ARS

The Argentine peso needs its own treatment:

- the official rate is administered and moves in small daily steps, so its historical volatility badly understates the risk;
- it is devalued in steps: in December 2023 the official rate lost more than half its value in a single day;
- several exchange rates coexist (official, "blue", CCL), so every ARS contract's resolution clause names the fixing exactly.

Its margin is driven by stress scenarios rather than volatility: a step devaluation at least as large as December 2023 is a mandatory stress scenario, `stress_weight = 1`, and `k` starts at 2.

## 10. Lifecycle

### 10.1 Pre-trade

Order placement and execution check `equity_after_trade >= IM_after_trade` at account level. This replaces the per-position `get_required_margin` reservation.

### 10.2 Option premium

The option buyer pays the premium to the seller at trade time. Today neither side pays and both lock collateral; this changes.

### 10.3 Mark-to-market

On a heartbeat (the timer mechanism already used by settlement), every open position is revalued at mark. Unrealized PnL counts in equity immediately; a daily settlement moves it into cash as variation margin. On the Internet Computer the heartbeat processes accounts in bounded batches so each call stays within its instruction limit.

### 10.4 Liquidation

When `equity < MM`:

1. the account is flagged and can only reduce risk;
2. the liquidation engine closes positions incrementally, largest margin contributor first, against the order book within a slippage limit, or through a backstop;
3. a liquidation fee goes to the insurance fund;
4. liquidation stops as soon as `equity >= MM`.

### 10.5 Insurance fund

The existing `INSURANCE_FUND` (already funded by settlement fees) covers any negative equity left after liquidation. What happens when it is exhausted is open question 15.3.

### 10.6 Expiry

The existing settlement pipeline is kept: positions settle against the oracle fixing and their margin is released.

## 11. Parameters

Per underlying, all stored on chain and readable through a query endpoint:

| Parameter             | Default                         | Meaning                                               |
| --------------------- | ------------------------------- | ----------------------------------------------------- |
| `lambda`              | 0.94                            | EWMA decay                                            |
| `sampling_interval`   | 1 day                           | Return sampling interval                              |
| `weekend_weight`      | 0.3 for FX, 1 for crypto        | Volatility-time weight of closed days                 |
| `floor_ratio`         | 0.75 (higher for CNY, SGD)      | Volatility floor as a fraction of long-run volatility |
| `long_run_lookback`   | 10 years or full history        | Includes a stressed period                            |
| `mpor`                | 2 days                          | Margin period of risk                                 |
| `risk_measure`        | ES 97.5% (alternative: VaR 99%) | Tail measure of historical simulation                 |
| `hs_lookback`         | 2 years                         | Historical simulation window                          |
| `hs_min_observations` | 250                             | Below this, only the grid and stress apply            |
| `confidence`          | 99% (`z = 2.33`)                | Grid confidence                                       |
| `vsr_ratio`           | 0.25                            | Volatility scan range as a fraction of volatility     |
| `short_option_min`    | 0.01                            | Per short option contract, as a fraction of `S`       |
| `stress_weight`       | 0.5 (1 for ARS)                 | Weight of fixed stress scenarios                      |
| `mm_ratio`            | 0.8                             | Maintenance as a fraction of initial                  |
| `k`                   | 1.0 (2 for ARS)                 | Manual risk multiplier                                |
| `event_calendar`      | empty                           | Scheduled events with weights                         |
| `stale_after`         | 15 minutes                      | Oracle staleness bound                                |
| `stale_multiplier`    | 1.5                             | Margin multiplier while the oracle is stale           |
| `liquidation_fee`     | 0.5% of notional                | Paid to the insurance fund                            |

Global: `heartbeat_interval`, `global_k`.

Defaults are starting points to be calibrated per underlying (section 12). Only the risk administrator (a controller today) can change them, and every change emits an event.

## 12. Calibration and backtesting

- Daily backtest per underlying: count the days on which the realized loss over the MPOR exceeded IM. At 99% the expected breach rate is 1%; the Kupiec proportion-of-failures test decides whether the observed rate is acceptable.
- Breaches beyond tolerance trigger a parameter review.
- Margin levels are tracked over time to detect procyclical swings.

## 13. Implementation phases

Each phase is its own small, verifiable change.

1. Price history per underlying: storage, the backfill endpoint with provenance, daily sampling of the oracle; EWMA volatility, floor, event calendar and manual multiplier.
2. Mark price service: index (including a live FX source, open question 15.5), `Linear` mark, Black-76 for options in volatility time.
3. Option premium flow, and the short side of `Call`/`Put` corrected.
4. Scenario engine: filtered historical simulation as the primary measure, the SPAN grid as the floor, the short option minimum; the account-level pre-trade IM check replaces `get_required_margin` for `Linear`, `Call` and `Put`.
5. Heartbeat mark-to-market, MM check, liquidation and insurance fund.
6. Stress scenario library per underlying (CHF 2015, crypto 2020 and 2022, ARS 2023).
7. `Linear` forwards and NDFs on top of the new engine, without `settlement_cap` (the reworked PR #86).
8. Offsets across underlyings.

## 14. Known defect this model removes

In the current code the short side of `Call` and `Put` is accounted like the long side:

- settlement credits a short the option payoff (`payoffs/mod.rs`, the `_ => Ok(abs_qty * unit_payoff)` arm of `get_settlement_value`), so on an in-the-money call both legs are paid;
- a short call's reserved margin uses the same formula as a long's, so the per-trade re-mark credits the short when the premium rises;
- a short put that expires out of the money loses its full strike instead of keeping the premium.

No test exercises this path; the settlement tests set cashflows by hand. Phase 3 fixes it with tests that prove the corrected behavior.

## 15. Open questions

1. Should `Binary` and `Categorical` stay fully collateralized, or move into the scenario engine?
2. Who governs the parameters long term: controllers, an SNS DAO, or a dedicated risk committee role?
3. When the insurance fund is exhausted: socialized loss across profitable accounts, or auto-deleveraging of the most profitable opposite positions?
4. Should carry (interest-rate differential) enter the `Linear` mark from day one for FX underlyings?
5. The current live oracle (CoinGecko) covers crypto but not FX. Which live source provides FX index prices: the Exchange Rate Canister, or a market-data provider through HTTPS outcalls?

## 16. References

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
- Board of Governors of the Federal Reserve System, _H.10 Foreign Exchange Rates_, via FRED (Federal Reserve Bank of St. Louis).
- European Central Bank, _Euro foreign exchange reference rates_.
- Deribit, _Standard Margin_ and _Portfolio Margin_ documentation.
