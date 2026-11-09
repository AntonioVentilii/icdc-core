# Implementation plan: margin model, phase 1

Phase 1 of [`docs/risk/margin-model.md`](../risk/margin-model.md) (section 15): governance, price history and volatility.

**This phase does not change how margin is computed.** Nothing reads the new volatility, multipliers or history into `get_required_margin` or into any trade, settlement or withdrawal path. They become inputs in phase 6. This phase builds and exposes them so they can be loaded, inspected and backtested first.

The fund-withdrawal block of section 12 ships separately (it is a one-line security fix and should not wait for this PR).

## 1. Scope

### 1.1 Roles (doc section 12)

- A new role, **risk administrator**, held by a set of principals in `RiskState::risk_admins`.
- Risk endpoints require `caller_is_risk_admin`. Controllers are **not** implicitly risk administrators.
- Only controllers can add or remove a risk administrator. Adding goes through the timelock; removing is immediate (it can only reduce powers). A controller principal cannot be made a risk administrator, so the two roles never sit on the same key.
- Treasury operations (`withdraw_fund` and the rest of `admin`) stay controller-only, so a risk administrator can never move funds.
- **First risk administrator: to be decided.** The canister ships with no risk administrator, so every risk endpoint stays unusable until a controller proposes one and the timelock expires. Which principal that is (and whether it is a person, a multisig or a DAO-controlled canister) is not decided yet.

Limit, stated plainly: canister controllers can still upgrade the code. App-level roles bound what an admin key can do **through the API**; they do not and cannot bound what a code upgrade can do. Long-term governance of upgrades is open question 17.2 of the design.

### 1.2 Hard bounds and the timelock (doc sections 6.4, 12, 13)

Bounds are constants in code. A value outside them is rejected with `RiskError::OutOfBounds`, whoever the caller is.

| Parameter             | Default                             | Bounds              |
| --------------------- | ----------------------------------- | ------------------- |
| `lambda`              | 0.94                                | [0.80, 0.995]       |
| `weekend_weight`      | 1.0 (set to 0.3 for FX underlyings) | [0, 1]              |
| `floor_ratio`         | 0.75                                | [0.5, 1.5]          |
| `mpor_days`           | 3                                   | [1, 10]             |
| `k`, `global_k`       | 1.0                                 | [1, 3]              |
| calendar event weight | none                                | [0, 10]             |
| `timelock_duration`   | 48 hours                            | [24 hours, 30 days] |

The timelock is generic: a `PendingAction` records the action, who proposed it, when, and when it becomes executable. Pending actions are publicly queryable. A risk administrator or a controller can cancel one; anyone (non-anonymous) can execute one once its delay has passed, and execution re-validates the action against the current state.

Consumers in this phase:

| Change                    | Speed                                                                   |
| ------------------------- | ----------------------------------------------------------------------- |
| Raise `k` or `global_k`   | Immediate, within bounds                                                |
| Lower `k` or `global_k`   | Timelocked                                                              |
| Activate a history upload | Timelocked                                                              |
| Add a risk administrator  | Timelocked                                                              |
| Raise `timelock_duration` | Immediate                                                               |
| Lower `timelock_duration` | Timelocked (otherwise the timelock could be shortened to bypass itself) |

`lambda`, `weekend_weight`, `floor_ratio`, `mpor_days` and calendar entries change immediately in this phase because nothing consumes them yet. Phase 6, which makes them inputs to margin, puts the margin-lowering direction of each behind the timelock.

### 1.3 Price history and backfill (doc section 9.2)

- One history per underlying: daily `PricePoint { date, price }`, `date` in days since the Unix epoch (UTC), `price` in USD per unit of the underlying.
- **Backfill** in three steps, risk administrator only:
  1. `begin_history_upload { underlying, source, series_id }` opens an upload;
  2. `append_history_chunk { upload_id, points }` adds points (at most 2,000 per chunk, 20,000 per upload, dates strictly increasing across chunks, prices strictly positive);
  3. `seal_history_upload { upload_id, sha256_hex }` closes it. The canister recomputes the SHA-256 of the canonical encoding and rejects a mismatch; on a match it records source, series id, first and last date and the hash, and proposes the activation through the timelock.
- **Canonical encoding** (so an off-chain script can reproduce the hash): for each point in ascending date order, the date as a 4-byte big-endian `u32`, then the price as a 16-byte big-endian `i128` in fixed point with 12 decimals (section 1.5). The points are concatenated with no separator and no header.
- **Activation** merges the upload into the history: uploaded points replace existing points on the same dates and new dates are inserted. The upload keeps its metadata and hash; its points are dropped once merged.
- **Preconditions and limits**: an upload is accepted only for an underlying already configured with `set_underlying_risk_params`, so every history has parameters. At most 8 uploads can be open or sealed at a time, an underlying holds at most 512 calendar events, and at most 256 actions can wait behind the timelock.
- **Daily sampling**: an hourly timer appends today's point for every underlying whose parameters name an `oracle_asset_id`, if the asset's `AssetMetrics::price_usd` was updated in the last 24 hours and today has no point yet. No new external call is made in this phase; the source is the existing collateral-asset price.

### 1.4 Volatility (doc sections 3, 6.1 to 6.4)

- **Returns** between consecutive history points `a < b`: `r = ln(P_b / P_a)`, normalized by the volatility time elapsed, `x = r^2 / tau` with `tau = sum of day weights for the days a+1 ..= b`. Day weight is `weekend_weight` on Saturday and Sunday, `1` otherwise. Historical returns are normalized by weekends only: event weights apply to the future horizon, so that event variance stays in the history the model learns from.
- **EWMA** (section 6.1): seeded with the mean of the first 20 normalized returns, then `sigma2 = lambda x sigma2 + (1 - lambda) x x_t` for each following return. At least 21 returns are required.
- **Long-run variance**: the mean of all normalized returns in the history.
- **Floor** (section 6.2): `sigma_used = max(sigma_ewma, floor_ratio x sigma_long_run)`.
- **Volatility time over the MPOR** (section 6.3): the sum of day weights over the next `mpor_days` days starting today, plus, for each calendar event inside that horizon, `(weight - 1) x day_weight x ramp`, with `ramp = 1 - (event_time - now) / mpor`. An event at the edge of the horizon contributes nothing and reaches its full weight as it arrives: there is no step.
- **Multiplier composition** (section 3): `k_eff = k x global_k`. The `stale_factor` arrives with the price pipeline in phase 2.
- All volatility figures are daily, per unit of volatility time.

### 1.5 Fixed-point arithmetic (doc section 7.7)

- `Fixed`: a signed 128-bit integer with 12 decimals (`SCALE = 10^12`). No `f64` anywhere in this module.
- Multiplication and division truncate toward zero. Every operation is checked and returns an error instead of overflowing.
- `sqrt` returns the floor of the exact square root (integer Newton iteration).
- `ln` reduces the argument to `[1, 2)` by powers of two, then sums the series `2 x atanh(y)` with `y = (m - 1) / (m + 1)`, to an absolute error below `10^-11` on the history's price ratios.
- At the API boundary values travel as `DecimalValue` (`value`, `decimals`): inputs are converted with truncation, outputs are returned with 12 decimals.

## 2. Files

| File                                                                                                  | Content                                                              |
| ----------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `src/clearing/src/risk/mod.rs`                                                                        | Module root                                                          |
| `src/clearing/src/risk/fixed.rs`                                                                      | `Fixed` and its arithmetic                                           |
| `src/clearing/src/risk/params.rs`                                                                     | Parameter types, defaults, bounds and validation                     |
| `src/clearing/src/risk/calendar.rs`                                                                   | Calendar events and day weights                                      |
| `src/clearing/src/risk/volatility.rs`                                                                 | Returns, EWMA, floor, volatility time, multiplier composition        |
| `src/clearing/src/risk/history.rs`                                                                    | Price history, uploads, canonical hash, merge                        |
| `src/clearing/src/risk/timelock.rs`                                                                   | Pending actions and their application                                |
| `src/clearing/src/risk/state.rs`                                                                      | `RiskState`, the single persisted risk record, and the audit log     |
| `src/clearing/src/risk/sampler.rs`                                                                    | Hourly sampling timer                                                |
| `src/clearing/src/api/risk/{mod,api,params,results,errors}.rs`                                        | Endpoints, following `.agents/patterns.md`                           |
| `src/clearing/src/guards.rs`                                                                          | `caller_is_risk_admin`                                               |
| `src/clearing/src/memory.rs`, `src/clearing/src/types/state.rs`, `src/clearing/src/migrations/mod.rs` | Persistence of `RiskState`                                           |
| `src/clearing/src/lib.rs`                                                                             | Module, endpoint imports, sampler timer on `init` and `post_upgrade` |
| `src/clearing/Cargo.toml`                                                                             | `sha2` (already a workspace dependency)                              |
| `src/clearing/tests/it/risk.rs`                                                                       | Integration tests                                                    |

## 3. Endpoints

Updates:

| Endpoint                                                              | Caller                   | Effect                                                                                                                     |
| --------------------------------------------------------------------- | ------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| `add_risk_admin(principal)`                                           | controller               | Proposes adding a risk administrator (timelocked); returns the action id                                                   |
| `remove_risk_admin(principal)`                                        | controller               | Removes immediately                                                                                                        |
| `set_underlying_risk_params(...)`                                     | risk admin               | Sets `lambda`, `weekend_weight`, `floor_ratio`, `mpor_days`, `oracle_asset_id`; creates the underlying with `k = 1` if new |
| `set_risk_multiplier { underlying, k }`                               | risk admin               | Applied if raising, pending if lowering                                                                                    |
| `set_global_risk_multiplier { k }`                                    | risk admin               | Same rule                                                                                                                  |
| `set_timelock_duration { duration_ns }`                               | risk admin               | Applied if raising, pending if lowering                                                                                    |
| `add_calendar_events { underlying, events }`                          | risk admin               | Adds events; returns their ids                                                                                             |
| `remove_calendar_event { underlying, event_id }`                      | risk admin               | Removes one event                                                                                                          |
| `begin_history_upload`, `append_history_chunk`, `seal_history_upload` | risk admin               | Backfill (section 1.3)                                                                                                     |
| `cancel_pending_action(id)`                                           | risk admin or controller | Cancels; a cancelled upload activation also cancels the upload                                                             |
| `execute_pending_action(id)`                                          | any non-anonymous caller | Executes once the delay has passed                                                                                         |

Queries, all public: `get_risk_governance` (risk administrators and global parameters), `list_underlying_risk_params`, `list_calendar_events`, `list_pending_actions`, `get_history_upload`, `get_price_history`, `get_volatility` (EWMA, long-run and floored volatility, observations, last date, volatility time over the MPOR and `k_eff`), `list_risk_audit`.

Every state change appends an entry to a public **risk audit log** (`list_risk_audit`), attributed to its caller. This is how phase 1 meets "every change emits an event" without touching the trading `Event` stream, whose variants drive trade history and leaderboards.

## 4. Storage and migration

- All phase-1 state lives in one record, `RiskState`, held in a `RISK_STATE` thread-local and persisted as a new **optional** field `risk: Option<RiskState>` of `StableState`.
- Per `.agents/patterns.md` section 5 and `docs/ai/migrations.md`, adding an `opt` field needs no migration: blobs written before this release decode with `risk = None`, which restores a default `RiskState` (no administrators, default global parameters, empty histories).
- `migrations::into_current` sets `risk: None` for the pre-resolution legacy schema.
- Footprint: about 3,650 points per underlying for ten years of history, 20 bytes each when encoded.

## 5. Tests

Unit:

- `Fixed`: conversions to and from `DecimalValue`, truncation, overflow errors, `sqrt` against exact squares and non-squares, `ln` against known values (ln 2, ln 10, ln of ratios near 1).
- EWMA against a hand-computed sequence; long-run mean; floor applied only when it binds.
- Returns across a weekend with `weekend_weight` 0.3 and 1.
- Volatility time: weekends, an event at the horizon edge (no contribution), halfway (half), today (full), a past event (none).
- Bounds: every parameter at, just inside and just outside its bounds.
- Timelock: propose, list, cancel, early execution rejected, execution after the delay, re-validation at execution.
- `k`: raise applied immediately, lower pending, composition `k x global_k`.
- Roles: risk endpoints reject non-administrators and controllers; a controller cannot be made a risk administrator.
- Backfill: chunk validation (ordering, positivity, limits), hash mismatch rejected, matching hash proposes activation, activation merges and replaces dates.
- Sampling: appends today from fresh oracle data, skips stale data and existing dates.
- Persistence: `StableState` without `risk` decodes to the default `RiskState`.

Integration (PocketIC): adding a risk administrator through the timelock by advancing time; uploading, sealing and activating a backfill; reading volatility; raising and lowering `k`; rejection of an early execution and of non-administrator callers.

## 6. Deviations from the design

- **Audit log instead of trading events** (section 3 of this plan).
- **Historical returns normalized by weekends only**, not by event weights (section 1.4), so that event variance stays in the history.
- **Phase-1 parameters other than `k` change immediately** because nothing consumes them yet; phase 6 moves their margin-lowering direction behind the timelock (section 1.2).
- **Volatility is daily** per unit of volatility time; annualization is not needed until marks and margin use it.
- **No pre-filled calendar in code.** The design asks for a calendar pre-filled with central-bank meetings and data releases. Those dates must come from the publishers' official schedules, so they are loaded through `add_calendar_events` by an operator script, not hard-coded here.
