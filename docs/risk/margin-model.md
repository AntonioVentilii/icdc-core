# Margin model

- **Status**: Approved design, revised after independent review
- **Scope**: `clearing` (risk engine, accounts), `registry` (instruments, venues), `shared` (types)
- **Supersedes**: the full-collateral model in `src/clearing/src/payoffs/mod.rs`, and the `settlement_cap` approach proposed in PR #86

## 1. Why this exists

Today the clearing engine collateralizes every position up front by its maximum possible loss. That is not margin, it is locked collateral, and it breaks down exactly where derivatives matter:

- A short forward or a short call has no finite maximum loss, so it cannot be collateralized at all without inventing an artificial cap.
- Capital efficiency is poor: a participant posts the worst case instead of a realistic close-out loss.

This document defines a risk-based margin model in line with how listed-derivatives central counterparties and Deribit operate: participants post **initial margin** sized to the loss a position could suffer while it is being closed out, at a high confidence level; positions are **marked to market** continuously; accounts that fall below **maintenance margin** are **liquidated**; a **default fund** absorbs what liquidation cannot recover.

We do not invent a methodology. We adopt published, industry-standard methods and own only their parameters.

## 2. Principles

1. **Risk based, not loss based.** Margin covers a high-confidence loss over the close-out horizon, not the theoretical maximum.
2. **One method for every instrument.** Positions are revalued under a set of scenarios and margin is taken from the resulting loss distribution. No per-instrument formulas.
3. **One account, many venues.** A participant holds one clearing account. Trades executed on any connected venue net into it, and margin is computed once on the net portfolio. This is the reason a clearing house exists (section 11).
4. **Portfolio based.** Positions on the same underlying are revalued together, so offsetting positions (a covered call, a calendar spread, a long on one venue against a short on another) are recognized.
5. **Transparent methods, private positions.** Methods, parameters, historical series and aggregate figures are public and queryable. Positions are visible only to their owner, to agents the owner authorizes, and to the risk function (section 11.3).
6. **Conservative and anti-procyclical.** Margins must not collapse in calm markets and then spike in a crisis. Volatility has a floor, a stressed period is always part of the history, and stress scenarios always apply.
7. **No single key moves money.** Risk parameters have hard bounds, reductions of margin wait behind a timelock, and nobody can withdraw from the default fund (section 12).
8. **Published methods, our parameters.** Every choice below cites its source (section 17).

## 3. Concepts

| Term                    | Meaning                                                                                                                                             |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Index price `S`         | Spot reference for an underlying, built from several sources (section 9.5)                                                                          |
| Mark price              | Fair value of an instrument used for margin, PnL and liquidation (section 5)                                                                        |
| Equity                  | Cash + haircut collateral value + unrealized PnL of open positions at mark, minus margin reserved by fully collateralized instruments (section 7.5) |
| Initial margin (IM)     | Equity required to open or increase risk                                                                                                            |
| Maintenance margin (MM) | Equity below which the account is liquidated; `MM = mm_ratio x IM`                                                                                  |
| MPOR                    | Margin period of risk: the time assumed to close out a defaulting account                                                                           |
| Variation margin        | Mark-to-market PnL moved into cash                                                                                                                  |

Account rules:

- A trade that increases risk is accepted only if `equity_after_trade >= IM_after_trade`.
- A trade that only reduces risk is always accepted.
- If `equity < MM`, the account enters liquidation (section 10.4).

Why three kinds of margin:

- **Initial margin** sizes the loss that could build up while a defaulting account is being closed out.
- **Maintenance margin** is lower so that an account opened exactly at IM is not liquidated by the first adverse tick. The gap between IM and MM is a buffer against routine moves.
- **Variation margin** settles profit and loss in cash every day, so losses never accumulate unseen until expiry and margin only has to cover the next few days of risk.

Central counterparties call their members for initial margin, and brokers issue a margin call to clients who fall below maintenance and ask them to restore initial margin by a deadline. ICDC follows Deribit and the other continuously trading venues instead: there is no call, the account is liquidated automatically, because the market never closes and nobody is there to answer a call.

All margin multipliers compose multiplicatively: `k_eff = k x global_k x stale_factor`, where `stale_factor` applies to IM only (section 5.4).

## 4. Instrument coverage

| Payoff                                | Margined by this model | Notes                                                                                                                                                                                                                  |
| ------------------------------------- | ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Linear` (forward, NDF, dated future) | Yes                    | No settlement cap. Replaces the bounded design of PR #86.                                                                                                                                                              |
| `Call`, `Put`                         | Yes                    | The buyer pays the premium at trade time. A long option counts at mark in equity and its scenario losses never exceed that mark, so a long option needs no cash beyond the premium already paid. Sellers are margined. |
| `Binary`, `Categorical`               | No, unchanged          | Loss is bounded by one unit per contract, so full collateralization stays correct and simple, as on prediction markets. May move into the scenario engine later (open question 16.1).                                  |
| ARS instruments                       | Fully collateralized   | See section 9.4: risk-based margin for ARS would exceed the notional, so it is stated plainly as full collateral.                                                                                                      |

Every contract on ICDC settles in cash in USD, so for us any currency forward is economically an NDF. Whether a currency is "NDF" in the wider market only matters for where its fixing comes from and for where offshore demand is concentrated.

## 5. Prices

### 5.1 Index

The index `S` of each underlying is the robust median of several sources (section 9.5): outliers beyond `outlier_band` from the median are dropped, and a new index value may move at most `jump_limit` from the previous one per update. A move beyond the limit is accepted only once a second update confirms it.

### 5.2 Mark

- **`Linear`**: `F = S x (1 + (r_quote - r_base) x T)`, with `T` the time to expiry in years and `r_quote`, `r_base` configured money-market rates per currency (simple interest). Carry is applied from day one: for high-carry currencies (ARS, BRL, INR, MXN) the forward differs from spot by tens of percent a year, and marking at spot would create fictitious profit. Once a series is liquid, its mark is the mid of the order book, bounded within `mark_band` of the carry-adjusted model value.
- **`Call`, `Put`**: Black-76 on `F`. The volatility is implied from the order book when quotes exist and lie within `iv_band` of the model volatility of section 6, otherwise the model volatility. Time to expiry is measured in **volatility time** (section 6.3).
- **Liquidation uses the mark**, never a single raw index print. For `Linear`, the mark carries an exponential average of the observed basis, so one manipulated index print cannot by itself trigger liquidations.

### 5.3 Model marks do not create withdrawable cash

Unrealized gains on a position whose mark comes from a model (any option marked with model volatility, any `Linear` series without a liquid book) count towards margin but are not withdrawable until realized by a trade or at settlement. A change of a model parameter can therefore move margin requirements, but never pays cash out of the system.

### 5.4 Staleness and deleveraging-only mode

If the index of an underlying cannot be built (fewer than `pusher_quorum` sources agree, or the latest value is older than `stale_after`), the underlying enters **deleveraging-only mode**:

1. new risk-increasing trades are rejected, except trades that take over a position being liquidated;
2. IM for any remaining risk-increasing activity is multiplied by `stale_multiplier`; MM is **not** raised, so staleness by itself never pushes an account into liquidation;
3. liquidations continue on the last valid mark, which widens by the volatility elapsed since it was set;
4. the mode ends as soon as the quorum is restored.

## 6. Volatility

### 6.1 Base estimate: EWMA

On log returns `r` of the index, sampled at a fixed interval:

```
sigma2_t = lambda x sigma2_(t-1) + (1 - lambda) x r_(t-1)^2
```

Default `lambda = 0.94` for daily sampling (RiskMetrics). Recent returns weigh more, with weights decaying exponentially. EWMA reacts within days to a change in regime while needing no fitting, which keeps it reproducible on chain.

### 6.2 Floor (anti-procyclicality)

```
sigma_used = max(sigma_ewma, floor_ratio x sigma_long_run)
```

`sigma_long_run` is measured over the full available history, which includes the stressed periods of section 7.1. The floor applies everywhere volatility is used: in the SPAN grid, in option marks, and in the rescaling of historical simulation. Together with the stressed-period weight of section 7.1, this follows two of the anti-procyclicality tools required of EU central counterparties (a floor tied to a long lookback, and at least a 25% weight on stressed observations).

### 6.3 Volatility time and event weights

Foreign-exchange option desks do not treat every day as equal. Each future interval `i` carries a weight `w_i`:

- normal trading interval: `w = 1`;
- weekends and holidays: `weekend_weight` (`0.3` for FX, whose market closes from Friday to Sunday evening New York time; crypto trades continuously and keeps `1`);
- scheduled events: higher, for a central-bank decision, a CPI or payrolls release, an election, an option expiry.

Variance over a horizon is then

```
variance(horizon) = sigma_used^2 x sum(w_i x dt_i)   over the intervals i in the horizon
```

**Event calendar.** Each underlying has a calendar of `{ time, weight, label }` entries stored on chain. It is not empty by default: it is pre-filled with the recurring events of each currency (FOMC, ECB, BoJ, BoE, SNB, RBA, BoC, RBNZ, Banxico, BCB and the other central banks of section 9.1, US CPI and payrolls) and with option expiries. For a currency pair, the calendar is the union of both currencies' calendars.

**Calibration.** Each event type's weight is estimated from history as the ratio between realized variance on past occurrences of that event and on ordinary days, and reviewed periodically.

**No cliff.** An event's extra weight phases in linearly over the MPOR before it, instead of switching on the moment it enters the horizon, so there is no single minute at which margin jumps and participants can position around it.

### 6.4 Manual risk multiplier

Each underlying has a multiplier `k`, bounded to `[1, k_max]`, applied to the final margin. The risk administrator can raise it immediately for an event nobody scheduled (a sudden headline, a depeg, a market closure). Lowering it goes through the timelock of section 12. A global multiplier `global_k` with the same rules applies to all underlyings. Every change emits an event.

### 6.5 Unscheduled jumps

A jump that no calendar announced cannot be forecast. It is covered in layers: the stressed periods and stress scenarios that always apply (sections 7.1 and 7.3), the volatility floor, the fast reaction of EWMA, the manual multiplier, and finally liquidation plus the default fund. This is a stated limitation, not a solved problem.

## 7. Scenario engine

For an account, positions are grouped by underlying. Each group is revalued at mark under every scenario `s`, giving `loss_s = value_now - value_s`. Offsets inside a group are recognized; offsets across underlyings come later (phase 9).

### 7.1 Filtered historical simulation (primary measure)

1. **Window.** The last `hs_lookback` of daily index returns, **plus fixed stressed periods** per group, weighted to at least `stressed_weight` of the scenario set: for crypto March 2020 and May and November 2022; for FX 2008, the CHF floor removal of January 2015 and March 2020. A rolling window alone would drop its stressed period as time passes.
2. **Horizon.** Returns are taken over the MPOR as overlapping sums of daily log returns. `hs_min_observations` counts daily returns.
3. **Rescaling.** Each return is rescaled by `sigma_now / sigma_then`, both EWMA, with the floor of section 6.2 applied to `sigma_now`. Event weights in the coming MPOR raise `sigma_now` as in section 6.3.
4. **Revaluation.** The group is revalued under each rescaled return.
5. **Measure.** `risk_measure` is Expected Shortfall at 99%: the average of the losses beyond the 99% quantile. It is set at 99% rather than the 97.5% of the Basel market-risk framework because EU rules require 99.5% coverage for instruments that do not trade on a regulated market, which describes every instrument on ICDC, and Expected Shortfall at 99% sits at or above that level for normal and fat-tailed distributions. VaR at 99.5% is the alternative setting.

The historical backfill of section 9.2 provides enough history from day one for every underlying in section 9.1.

### 7.2 SPAN grid (floor)

Price scan range and volatility scan ranges:

```
PSR    = S x sigma_used x sqrt(volatility time over MPOR) x z      (z = 2.58 for 99.5%)
VSR_up = vsr_up x sigma_used,   VSR_down = vsr_down x sigma_used
```

The scan is asymmetric because implied volatility rises in a crash far more than it falls in a rally: Deribit uses +45% and -30% for crypto, and BTC implied volatility rose by more than half in a day in March 2020.

The 16 classic SPAN scenarios:

- price moves of `0`, `+-1/3`, `+-2/3` and `+-1` times `PSR`, each combined with volatility up by `VSR_up` and down by `VSR_down` (14 scenarios);
- two extreme moves of `+-3` times `PSR`, of which a fraction `extreme_fraction` of the loss counts (2 scenarios). The fraction is a per-product parameter in SPAN, typically 30% to 35%.

`grid_risk = max(loss_s)` over the 16 scenarios. The grid protects against a history that has been unusually calm.

A **short option minimum** applies per short option contract: `som_multiple x S x sigma_used x sqrt(MPOR)`. It scales with each underlying's volatility, so it is meaningful both for BTC and for an FX pair with a fraction of BTC's volatility.

### 7.3 Stress scenarios

A fixed list per underlying of historical extremes applied as price shocks in both directions with weight `stress_weight`, for example the EUR/CHF move of 15 January 2015 and the BTC move of 12 March 2020.

### 7.4 Concentration add-on

A position that is large relative to what the market can absorb cannot be closed within the MPOR at the mark. When a group's net size exceeds `liquidity_threshold` times the average daily traded volume of its series across all venues, an add-on proportional to the excess is added to its margin.

### 7.5 Collateral

- Collateral that is also an underlying (ckBTC, ckETH, ICP) enters the scenario group of that underlying as a long position, so a BTC forward collateralized in ckBTC shows its doubled exposure.
- Other collateral is valued with a haircut at least equal to the IM rate of a standalone long position in that asset.
- Collateral prices come from the same index pipeline as underlyings (section 9.5). The single-principal `update_asset_price` path is retired.
- Fully collateralized instruments (`Binary`, `Categorical`, ARS) keep their reserved margin, which is subtracted from equity before the IM check.

### 7.6 Margin

```
IM_group   = k_eff x (max(hs_risk, grid_risk, stress_weight x stress_risk, short_option_minimum) + concentration_addon)
IM_account = sum(IM_group) over underlyings
MM_account = mm_ratio x IM_account
```

### 7.7 Computation on the Internet Computer

A full revaluation per account per heartbeat would not fit. The engine follows the SPAN risk-array approach:

1. once per heartbeat, for every series, compute the P&L of one contract under every scenario (a risk array);
2. for every account, margin is a weighted sum of risk arrays by position size, followed by a maximum and a tail average: cheap vector arithmetic, no option pricing per account;
3. accounts are re-margined in bounded batches, prioritized by distance to MM, and immediately on any price move beyond a threshold.

All arithmetic is fixed point with documented rounding, so that anyone with read access can reproduce a margin figure exactly. A measured instruction and cycle budget per heartbeat is a deliverable of phase 6.

## 8. Why historical simulation leads and SPAN is the floor

|                                                                     | Filtered historical simulation     | SPAN grid                                           |
| ------------------------------------------------------------------- | ---------------------------------- | --------------------------------------------------- |
| Source of scenarios                                                 | Real past moves, rescaled to today | Stylized moves from volatility                      |
| Data needed                                                         | A long, clean price history        | Only a volatility estimate                          |
| Captures fat tails                                                  | Yes                                | Partly, through extreme moves                       |
| Captures co-movement within an underlying (spot, carry, volatility) | Yes                                | Through the grid's joint price and volatility moves |
| Transparency                                                        | Lower, hundreds of scenarios       | Very high, 16 scenarios                             |
| Used by                                                             | CME SPAN 2, Eurex Prisma, LCH      | CME (historically), Deribit portfolio margin        |

Historical simulation is the current standard of the large central counterparties and captures the real shape of the tails, so it is the primary measure. Taking the maximum with the SPAN grid keeps a transparent floor that still holds when recent history has been unusually quiet.

## 9. Underlyings and data

### 9.1 Initial list

Currencies are stored as one series each against USD. Any cross is derived from them, for example `GBP/JPY = GBP/USD x USD/JPY`, so the 28 crosses among the G10 currencies need no history of their own. For history, both legs come from the same source and fixing time (FRED H.10), which keeps crosses consistent. Live crosses are derived from live legs taken at the same instant.

| Group         | Underlyings                                | History source                                                                            | Notes                                                                                                                |
| ------------- | ------------------------------------------ | ----------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Crypto        | BTC, ETH, SOL, ICP (vs USD)                | Coinbase Exchange public daily candles (USD pairs); Binance (USDT pairs) as a cross-check | Continuous calendar. SOL and ICP history starts in 2021.                                                             |
| G10           | EUR, GBP, JPY, CHF, AUD, CAD, NZD (vs USD) | Federal Reserve H.10 daily rates via FRED; ECB reference rates as a cross-check           | All 28 crosses derived                                                                                               |
| Asia          | CNY, KRW, TWD, INR, SGD (vs USD)           | Federal Reserve H.10 via FRED                                                             | KRW, TWD, INR and CNY trade offshore as NDFs; see 9.3 for CNY and SGD                                                |
| Latin America | BRL, MXN (vs USD)                          | Federal Reserve H.10 via FRED                                                             | BRL trades offshore as an NDF                                                                                        |
| Latin America | CLP (vs USD)                               | Banco Central de Chile, "dólar observado" (also the NDF fixing)                           | The observed dollar averages the previous day, so this index lags the market by about a day; quoted against USD only |
| Latin America | ARS (vs USD)                               | Banco Central de la República Argentina, official reference rate (Communication A 3500)   | Fully collateralized, section 9.4                                                                                    |

FRED series identifiers: `DEXUSEU` (EUR), `DEXUSUK` (GBP), `DEXJPUS` (JPY), `DEXSZUS` (CHF), `DEXUSAL` (AUD), `DEXCAUS` (CAD), `DEXUSNZ` (NZD), `DEXCHUS` (CNY), `DEXKOUS` (KRW), `DEXTAUS` (TWD), `DEXINUS` (INR), `DEXSIUS` (SGD), `DEXBZUS` (BRL), `DEXMXUS` (MXN). Some are quoted as USD per unit and others as units per USD; the loader normalizes every series to USD per unit of currency. H.10 rates are daily but published weekly, so FRED serves the backfill only, never live prices. All identifiers, and the ARS and CLP sources, are confirmed against the providers before the backfill runs.

Excluded for now:

- **HKD**: a hard peg to USD with near-zero volatility, whose only real risk is a break of the peg. CNY and SGD differ: they move within managed bands and have real day-to-day volatility. HKD can be added later with a peg-break stress scenario.
- **CNH** (offshore yuan): no free official history on the same fixing as the other currencies.

### 9.2 Loading and provenance

1. **Backfill, once.** An off-chain script downloads each series from its source, normalizes it and uploads it in chunks through a controller-only endpoint. Each upload records the source, the series identifier, the first and last date, and a SHA-256 hash of the canonical data. An upload becomes active only after the timelock of section 12, so the hash can be checked against the public source **before** the data is used, not only after.
2. **Then, on chain.** After the backfill the canister appends one observation per `sampling_interval` from its own index.
3. **Gaps.** FX holidays are not filled in. Returns are computed between consecutive observations and weighted by the volatility time that elapsed between them.

Footprint: ten years of daily data is about 3,650 values per series.

### 9.3 Calendars and managed regimes

- **Crypto** trades continuously: every day has weight 1.
- **FX** carries `weekend_weight` on weekends and holidays.
- **CHF** was held at a 1.20 floor against the euro from September 2011 to 15 January 2015. That period understates CHF volatility, so the floor and the January 2015 stressed period dominate it.
- **CNY** trades within a band around the People's Bank of China daily central parity, and **SGD** within the Monetary Authority of Singapore policy band. Both get `floor_ratio = 1.0` (volatility never below its long-run level) and mandatory stress scenarios, and their central-bank decisions are in the event calendar.

### 9.4 ARS

The Argentine peso cannot be margined on risk:

- the official rate is administered and moves in small daily steps, so its historical volatility badly understates the risk;
- it is devalued in steps: on 13 December 2023 the official rate went from about 366 to 800 per dollar, a loss of more than half its value in a day;
- several exchange rates coexist (official, "blue", CCL), so every ARS contract's resolution clause names the fixing exactly.

A margin covering a repeat of December 2023 in either direction, with a meaningful buffer, exceeds the notional of the contract. ARS instruments are therefore **fully collateralized**, stated plainly rather than disguised as a risk-based margin, and use a payoff whose loss is bounded by that collateral.

### 9.5 Live prices

The guiding constraint is cost: the design picks the cheapest option that stays verifiable.

| Option                                                                             | What it provides                                                                | Cost (13-node subnet)                                                                                              | Limits                                                                                          |
| ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------- |
| Exchange Rate Canister (XRC)                                                       | Crypto rates at minute granularity; fiat rates once a day from 11 central banks | 20M cycles per fiat/fiat or cached request, 260M when one asset is crypto or USDT (1B attached, the rest refunded) | No intraday FX; TWD, CLP and ARS are not supported                                              |
| HTTPS outcalls to a data API                                                       | Whatever the API provides                                                       | Depends on `max_response_bytes`: about 110M cycles for a 10 KB response, up to about 20B at the 2 MB default       | Every node calls the API, so free tiers run out 13 times faster; API keys sit in canister state |
| Off-chain price pushers (the existing oracle mechanism with authorized principals) | Any source, at any frequency                                                    | One batched update call per push, around 10M to 15M cycles                                                         | Trust rests on the pushers, mitigated below                                                     |

Decision:

1. **Live index from pushers, with a quorum.** Pushers read public sources (crypto from at least five exchanges; FX from intraday feeds for the G10 currencies and from the official daily fixings for the NDF currencies) and push all prices in one batched call. The canister builds the index with the robust median of section 5.1 and requires `pusher_quorum` agreeing sources. At launch the pushers are operated by the ICDC team, which is a stated trust limitation: independently operated pushers are added as they become available, and the set of pushers changes only through the timelock of section 12.
2. **On-chain check for crypto: XRC.** The canister asks XRC for each crypto index every `xrc_check_interval` and once more immediately before executing any liquidation. A liquidation does not execute while the pushed index disagrees with XRC beyond `xrc_band`. This guards the moment a manipulated price would do damage.
3. **On-chain check for FX: daily XRC where supported.** The daily XRC fiat rate is appended to the history and compared with the pushed index at the same fixing; a persistent gap flags the underlying for review. TWD, CLP and ARS have no XRC check.
4. **No HTTPS outcalls for live prices.**

Indicative cycle budget for the 20 underlyings of section 9.1: one batched push per minute about 20B cycles per day, XRC crypto checks every 15 minutes about 0.1T cycles per day, XRC daily fiat sampling about 0.3B cycles per day: together roughly 45T cycles (about 45 XDR) per year, before the cost of the heartbeat itself, which is measured in phase 6. Data licensing of each free source is checked before use.

## 10. Lifecycle

### 10.1 Pre-trade

Order placement and execution check `equity_after_trade >= IM_after_trade` on the whole account, across all venues. This replaces the per-position `get_required_margin` reservation.

### 10.2 Option premium

The option buyer pays the premium to the seller at trade time. Today neither side pays and both lock collateral; this changes.

### 10.3 Mark-to-market

On a heartbeat (the timer mechanism already used by settlement), risk arrays are refreshed and accounts are re-margined as in section 7.7. Unrealized PnL counts in equity immediately; a daily settlement moves realized-price PnL into cash as variation margin. Model-marked gains stay non-withdrawable (section 5.3).

### 10.4 Liquidation

When `equity < MM`:

1. the account is flagged and can only reduce risk;
2. the engine closes positions incrementally, each time choosing the closure that reduces IM the most, so it never closes one leg of a hedge and leaves the other;
3. each closure is first offered to the order book within `liquidation_slippage` of the mark;
4. if the book cannot absorb it, the **default fund takes the position over** at the mark minus `liquidation_discount`, into a fund-owned account, and unwinds it over time; contracted market makers will later be offered positions first, in exchange for a share of fees;
5. a liquidation fee, capped at the remaining equity, goes to the default fund;
6. liquidation continues until `equity >= IM x (1 + liquidation_buffer)`, so the account does not sit on the edge and get liquidated again on the next tick.

### 10.5 Default fund and default waterfall

Margin is sized for high-confidence losses, but a market can move faster than liquidation can close a position. The **default fund** pays for that gap. It replaces the existing `INSURANCE_FUND` storage, which today is funded only by settlement fees and is never drawn on by the waterfall.

**Funding.** A share of every fee goes to the default fund: trading fees, settlement fees and the full liquidation fee. The share is `default_fund_fee_share` while the fund is at or above its target, and rises to `default_fund_fee_share_below_target` while it is below. The treasury may seed it (open question 16.4).

**Target size: Cover 2.** The fund plus the protocol tranche must be able to absorb the simultaneous default of the two accounts that would leave the largest uncovered loss under the stress scenarios of section 7.3, after their own margin. This is the standard of the CPMI-IOSCO principles for systemically important central counterparties. The target is recomputed on every heartbeat and published.

**Open interest cap.** While the fund is below target, a trade that would raise the Cover 2 requirement above `fund + protocol_tranche` is rejected. Growth of the venue is bounded by the resources that stand behind it, from day one.

**Waterfall.** When liquidation leaves an account with negative equity, the loss is covered in this order:

1. the defaulting account's own margin and collateral;
2. the **protocol tranche** (`protocol_tranche`, from protocol fee income held in the treasury): the clearing operator's own money, consumed before anyone else's, as EU rules require of central counterparties;
3. the default fund;
4. only then, loss allocation across the remaining accounts (open question 16.3).

Every draw on the fund, and every step of the waterfall that is reached, emits an event. No administrative path can withdraw from the default fund (section 12).

### 10.6 Expiry

Positions settle against the oracle fixing and their margin is released. The settlement pipeline is adapted: today it computes `cashflow = payoff - reserved_margin - fees` and aborts the whole series with `SolvencyViolation` if the system cannot pay. Under this model an account with negative equity at settlement goes into the waterfall of section 10.5 instead, and the series settles for everyone else.

## 11. Accounts, venues and delegation

### 11.1 One account, many venues

A clearing house is where trades from different venues meet. A participant (a trader, a market maker, a fund) holds **one** clearing account. It can trade on several connected venues (exchanges, order books, OTC platforms), and every trade, wherever it was executed, lands in that one account. A long bought on venue A and a short sold on venue B net to zero exposure, and the participant posts margin once, on the net portfolio, instead of capitalizing each venue separately.

### 11.2 Venues

A venue is a registered engine in the registry. It submits matched trades to the clearing for accounts that have authorized it. This replaces the current controller-only `submit_matched_trade`. Each trade records the venue it came from, for audit and for the per-venue volume used in section 7.4. Onboarding criteria for venues are open question 16.5.

### 11.3 Delegation

The account owner grants scoped, revocable permissions to other principals, called agents: a venue, a market maker's trading system, a risk dashboard.

| Permission      | Allows                                                   |
| --------------- | -------------------------------------------------------- |
| `ReadPositions` | Read the account's positions, margin, equity and history |
| `Trade`         | Submit orders and trades on the account's behalf         |

Each grant can be limited by venue, by underlying or series, by maximum IM it may consume, by maximum notional, and by expiry date. The owner can revoke any grant at any time with immediate effect. Every action an agent takes emits an event attributed to that agent, so the owner keeps a complete audit log.

Visibility follows from this: positions are readable by the owner, by agents holding `ReadPositions`, and by the risk function. Methods, parameters, historical series, the default fund, the Cover 2 target and aggregate open interest per series are public.

## 12. Governance and admin powers

- **Roles are separate.** A risk administrator changes risk parameters, the event calendar and `k`. A treasury administrator manages protocol fee income. Neither can do the other's job.
- **Hard bounds.** Every risk parameter has a minimum and maximum fixed in code (for example `k` within `[1, 3]`); no administrator can set a value outside them.
- **Asymmetric speed.** Changes that raise margin take effect immediately, within the bounds. Changes that lower margin, change the pusher set, or activate a backfill upload wait behind a public timelock of `timelock_duration` and can be inspected before they take effect.
- **No withdrawal from the default fund.** The existing withdrawal path for the insurance fund (`FundType::Insurance` in `src/clearing/src/api/admin/api.rs`) is removed. The fund is only ever drawn by the waterfall.
- Long-term governance (controllers, an SNS DAO, or a risk committee) is open question 16.2.

## 13. Parameters

Per underlying, all stored on chain and readable through a query endpoint:

| Parameter                    | Default                                                                                                | Meaning                                                                    |
| ---------------------------- | ------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------- |
| `lambda`                     | 0.94                                                                                                   | EWMA decay                                                                 |
| `sampling_interval`          | 1 day                                                                                                  | Return sampling interval                                                   |
| `weekend_weight`             | 0.3 for FX, 1 for crypto                                                                               | Volatility-time weight of closed days                                      |
| `floor_ratio`                | 0.75 (1.0 for CNY, SGD)                                                                                | Volatility floor as a fraction of long-run volatility                      |
| `mpor`                       | 3 days; 5 days for currencies whose live index is a single daily fixing (CNY, KRW, TWD, INR, BRL, CLP) | Margin period of risk; reviewed once liquidity is demonstrated             |
| `risk_measure`               | ES 99% (alternative: VaR 99.5%)                                                                        | Tail measure of historical simulation                                      |
| `hs_lookback`                | 2 years                                                                                                | Rolling part of the historical window                                      |
| `stressed_weight`            | 25%                                                                                                    | Minimum weight of fixed stressed periods in the scenario set               |
| `hs_min_observations`        | 250 daily returns                                                                                      | Below this, only the grid and stress apply                                 |
| `confidence`                 | 99.5% (`z = 2.58`)                                                                                     | Grid confidence                                                            |
| `vsr_up`, `vsr_down`         | crypto 0.45 and 0.30; FX 0.30 and 0.20                                                                 | Asymmetric volatility scan                                                 |
| `extreme_fraction`           | 0.33                                                                                                   | Share of the extreme-move loss that counts                                 |
| `som_multiple`               | 1.5                                                                                                    | Short option minimum, in multiples of `S x sigma_used x sqrt(MPOR)`        |
| `stress_weight`              | 0.5                                                                                                    | Weight of fixed stress scenarios                                           |
| `liquidity_threshold`        | 0.1                                                                                                    | Share of average daily volume above which the concentration add-on applies |
| `mm_ratio`                   | 0.8                                                                                                    | Maintenance as a fraction of initial                                       |
| `k`                          | 1.0, bounded to `[1, 3]`                                                                               | Manual risk multiplier                                                     |
| `event_calendar`             | pre-filled (section 6.3)                                                                               | Scheduled events with weights                                              |
| `outlier_band`, `jump_limit` | crypto 2% and 10%; FX 0.5% and 3%                                                                      | Index robustness (section 5.1)                                             |
| `stale_after`                | 15 minutes                                                                                             | Index staleness bound                                                      |
| `stale_multiplier`           | 1.5                                                                                                    | IM multiplier in deleveraging-only mode                                    |
| `mark_band`, `iv_band`       | 1% and 25%                                                                                             | Bounds of book-derived marks around the model                              |

Global:

| Parameter                             | Default                                      | Meaning                                                         |
| ------------------------------------- | -------------------------------------------- | --------------------------------------------------------------- |
| `heartbeat_interval`                  | 1 minute                                     | Re-margining and liquidation cadence                            |
| `global_k`                            | 1.0, bounded to `[1, 3]`                     | Multiplier applied to every underlying                          |
| `pusher_quorum`                       | 2 of 3                                       | Agreeing sources needed to build an index                       |
| `xrc_check_interval`                  | 15 minutes                                   | XRC check of crypto indices, plus once before every liquidation |
| `xrc_band`                            | 2%                                           | Maximum gap between pushed index and XRC before a liquidation   |
| `liquidation_fee`                     | 0.5% of notional, capped at remaining equity | Paid to the default fund                                        |
| `liquidation_slippage`                | 1%                                           | Book execution limit around the mark                            |
| `liquidation_discount`                | 2%                                           | Discount at which the default fund takes a position over        |
| `liquidation_buffer`                  | 10%                                          | Liquidate until equity reaches IM plus this buffer              |
| `default_fund_fee_share`              | 20%                                          | Share of every fee paid to the default fund at or above target  |
| `default_fund_fee_share_below_target` | 50%                                          | Share while the fund is below target                            |
| `default_fund_target`                 | Cover 2                                      | Size target of the default fund plus protocol tranche           |
| `protocol_tranche`                    | 10% of the target                            | Protocol money consumed before the default fund                 |
| `timelock_duration`                   | 48 hours                                     | Delay of margin-lowering and other sensitive changes            |

Defaults are starting points to be calibrated per underlying (section 14) and are subject to the bounds and timelocks of section 12.

## 14. Calibration, backtesting and validation

- **Daily backtest** per underlying of two measures separately: the unfloored risk measure, with a two-sided Kupiec proportion-of-failures test (it should breach about as often as its confidence implies), and the effective IM, with a one-sided test (it must not breach more often than allowed).
- **Monthly** sensitivity analysis of margins to each parameter, and **quarterly** reverse stress tests (which moves would exhaust the default fund).
- Breaches beyond tolerance trigger a parameter review.
- Material changes of the model or its parameters are validated by someone other than their author before activation, in the spirit of the independent validation EU rules require.
- Margin levels are tracked over time to detect procyclical swings.

## 15. Implementation phases

Each phase is its own small, verifiable change. Margin is never reduced before the machinery to liquidate exists.

1. **Governance and history.** Role separation, parameter bounds, the timelock, removal of the default-fund withdrawal path; price history storage, the timelocked backfill endpoint with provenance, daily sampling; EWMA, floor, pre-filled event calendar and manual multiplier.
2. **Prices.** Robust index with pusher quorum, XRC checks, carry-adjusted marks, implied-volatility marks with model fallback, deleveraging-only mode; collateral moved onto the same pipeline.
3. **Options on the margin model.** The premium paid at trade time, and `Call` and `Put` settlement and margin rebuilt on this model, with tests that prove both sides of every position.
4. **Stress library.** Fixed stressed periods and stress scenarios per underlying.
5. **Liquidation and default fund, on the current margin.** Heartbeat re-margining, MM check, liquidation to IM plus buffer, default-fund takeover, waterfall, open interest cap; settlement routed through the waterfall.
6. **Switch to risk-based IM.** Risk arrays, filtered historical simulation with SPAN floor, short option minimum, concentration add-on, collateral in scenarios, account-level pre-trade check; measured instruction and cycle budget.
7. **Accounts, venues and delegation.** Venue-authorized trade submission, `ReadPositions` and `Trade` grants with limits and audit events. Independent of the risk engine, so it can run in parallel from phase 1.
8. **`Linear` forwards and NDFs** on the new engine, without `settlement_cap` (the reworked PR #86).
9. **Offsets across underlyings.**

## 16. Open questions

1. Should `Binary` and `Categorical` stay fully collateralized, or move into the scenario engine?
2. Who governs the parameters long term: controllers, an SNS DAO, or a risk committee?
3. When the protocol tranche and the default fund are both exhausted (step 4 of the waterfall): socialized loss across profitable accounts, or auto-deleveraging of the most profitable opposite positions?
4. Does the treasury seed the default fund at launch, and with how much? Until then the open interest cap of section 10.5 bounds the venue.
5. What are the onboarding criteria for venues and, later, for contracted market makers who take over liquidated positions?
6. Which intraday FX feeds the pushers use for the G10 currencies, subject to their licence terms?

## 17. References

- CME Group, _CME SPAN: Standard Portfolio Analysis of Risk_, methodology overview.
- CME Group, _SPAN 2_ framework documentation.
- Eurex Clearing, _Prisma: Portfolio-based Risk Management_, methodology description.
- Commission Delegated Regulation (EU) No 153/2013 (EMIR RTS on requirements for central counterparties): Article 24 (99% for exchange-traded, 99.5% for other instruments), Article 25 (lookback), Article 26 (2 days for exchange-traded, 5 days for other instruments), Article 28 (anti-procyclicality), Articles 47 to 59 (model validation and backtesting).
- Regulation (EU) No 648/2012 (EMIR), Article 45: default waterfall and the CCP's own resources ahead of non-defaulting members.
- CPMI-IOSCO, _Principles for Financial Market Infrastructures_ (2012): Principle 4, Credit risk (Cover 2); Principle 6, Margin.
- J.P. Morgan and Reuters, _RiskMetrics Technical Document_ (1996): EWMA volatility.
- Hull, J. and White, A. (1998), "Incorporating volatility updating into the historical simulation method for value-at-risk", _Journal of Risk_.
- Basel Committee on Banking Supervision (2019), _Minimum capital requirements for market risk_: Expected Shortfall.
- Kupiec, P. (1995), "Techniques for verifying the accuracy of risk measurement models", _Journal of Derivatives_.
- Black, F. (1976), "The pricing of commodity contracts", _Journal of Financial Economics_.
- Clark, I. J. (2011), _Foreign Exchange Option Pricing: A Practitioner's Guide_, Wiley: volatility time and event weights.
- Board of Governors of the Federal Reserve System, _H.10 Foreign Exchange Rates_, via FRED (Federal Reserve Bank of St. Louis).
- European Central Bank, _Euro foreign exchange reference rates_.
- DFINITY, _Exchange rate canister_ documentation and source (`github.com/dfinity/exchange-rate-canister`): request cost, data cadence, supported forex sources.
- Deribit, _Standard Margin_ and _Portfolio Margin_ documentation: scenario grid, asymmetric volatility shocks, short option contingency.
