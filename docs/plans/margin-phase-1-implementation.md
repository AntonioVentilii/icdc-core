# Implementation plan: margin model, phase 1

Phase 1 of [`docs/risk/margin-model.md`](../risk/margin-model.md) (section 15): governance, price history and volatility.

**This phase does not change how margin is computed.** Nothing reads the new volatility, multipliers or history into `get_required_margin` or into any trade, settlement or withdrawal path. They become inputs in phase 6. This phase builds and exposes them so they can be loaded, inspected and backtested first.

The fund-withdrawal block of section 12 ships separately (it is a one-line security fix and should not wait for this PR).

## 1. Scope

### 1.1 Roles (doc section 12)

- A new role, **risk administrator**, held by a set of principals in `RiskState::risk_admins`.
- Risk endpoints require `caller_is_risk_admin`. Controllers are **not** implicitly risk administrators, and the guard rejects a controller even if its principal is in `risk_admins` (it may have become a controller after being granted the role).
- Controllers keep two emergency powers on risk parameters: they can **raise** `k` and `global_k` immediately (an emergency brake that can only make margin stricter), and they can cancel any pending action. They cannot lower a multiplier or propose any other change.
- Only controllers can add or remove a risk administrator. Adding goes through the timelock; removing is immediate (it can only reduce powers). A controller principal cannot be made a risk administrator, so the two roles never sit on the same key.
- Treasury operations (`withdraw_fund` and the rest of `admin`) stay controller-only, so a risk administrator can never move funds.
- **First risk administrator: to be decided.** The canister ships with no risk administrator, so every risk endpoint stays unusable until a controller proposes one and the timelock expires. Which principal that is (and whether it is a person, a multisig or a DAO-controlled canister) is not decided yet.

Limit, stated plainly: canister controllers can still upgrade the code. App-level roles bound what an admin key can do **through the API**; they do not and cannot bound what a code upgrade can do. Long-term governance of upgrades is open question 17.2 of the design.

### 1.2 Hard bounds and the timelock (doc sections 6.4, 12, 13)

Bounds are constants in code. A value outside them is rejected with `RiskError::OutOfBounds`, whoever the caller is.

| Parameter             | Default                             | Bounds             |
| --------------------- | ----------------------------------- | ------------------ |
| `lambda`              | 0.94                                | [0.80, 0.995]      |
| `weekend_weight`      | 1.0 (set to 0.3 for FX underlyings) | [0, 1]             |
| `floor_ratio`         | 0.75                                | [0.5, 1.5]         |
| `mpor_days`           | 3                                   | [1, 10]            |
| `k`, `global_k`       | 1.0                                 | [1, 3]             |
| calendar event weight | none                                | [0, 10]            |
| `timelock_duration`   | 48 hours                            | [24 hours, 7 days] |

The timelock is generic: a `PendingAction` records the action, who proposed it, when, and when it becomes executable. Pending actions are publicly queryable. A risk administrator or a controller can cancel one, and execute one once its delay has passed. Rules that keep a pending action honest:

- **It records the value it changes from.** `LowerRiskMultiplier`, `LowerGlobalRiskMultiplier` and `ChangeTimelockDuration` carry `from` and `to`; `RemoveHistoryPoints` carries the SHA-256 of the points in its range. Execution is rejected with `ActionNoLongerValid` if the current value differs, so an action never applies to a state it was not reviewed against.
- **One pending action per key.** The key is the action kind plus its target (the underlying for `k`, the upload id, the principal, and so on). A new proposal with the same key replaces the old one, and the replacement is audited.
- **A raise withdraws pending lowerings.** Raising `k` or `global_k`, or lengthening the timelock immediately, withdraws any pending action on the same key. Without this, a lowering proposed before a crisis would still fire after an emergency raise.
- **Separate caps.** At most 256 risk-administrator actions and 16 controller actions (adding a risk administrator) can wait at once, so risk administrators can never fill the queue and block a controller.
- **Execution is restricted to risk administrators and controllers.** Execution does not need a privileged key in principle (the action was public for the whole delay and is re-validated), but restricting it means a change lands only when someone who can still cancel it chooses to land it, at a time they are watching. An expired action that nobody executes simply waits; it can be cancelled at any time.

Consumers in this phase:

| Change                                         | Speed                                                                   |
| ---------------------------------------------- | ----------------------------------------------------------------------- |
| Raise `k` or `global_k`                        | Immediate, within bounds (risk administrator or controller)             |
| Lower `k` or `global_k`                        | Timelocked (risk administrator only)                                    |
| Activate a history upload                      | Timelocked                                                              |
| Remove history points in a date range          | Timelocked                                                              |
| Add a risk administrator                       | Timelocked                                                              |
| Lengthen `timelock_duration` up to 2x current  | Immediate                                                               |
| Lengthen `timelock_duration` beyond 2x current | Timelocked (otherwise one call could freeze every change for a week)    |
| Shorten `timelock_duration`                    | Timelocked (otherwise the timelock could be shortened to bypass itself) |

`lambda`, `weekend_weight`, `floor_ratio`, `mpor_days` and calendar entries change immediately in this phase because nothing consumes them yet. Phase 6, which makes them inputs to margin, puts the margin-lowering direction of each behind the timelock.

### 1.3 Price history and backfill (doc section 9.2)

- One history per underlying: daily `PricePoint { date, price }`, `date` in days since the Unix epoch (UTC), `price` in USD per unit of the underlying.
- **Backfill** in three steps, risk administrator only:
  1. `begin_history_upload { underlying, source, series_id }` opens an upload;
  2. `append_history_chunk { upload_id, points }` adds points (at most 2,000 per chunk, 20,000 per upload, dates strictly increasing across chunks and not after today, prices strictly positive);
  3. `seal_history_upload { upload_id, sha256_hex }` closes it. The canister recomputes the SHA-256 of the canonical encoding and rejects a mismatch with `HashMismatch`, which does **not** return the computed hash: the declared hash must come from an independent computation on the public source, not be copied back from the canister. On a match it records source, series id, first and last date and the hash, and proposes the activation through the timelock.
- During the timelock anyone can page through the points of a sealed upload (`get_history_upload_points`) and compare them with the public source. An open or sealed upload can be abandoned (`abandon_history_upload`, risk administrator or controller), which drops its points, withdraws its pending activation and frees its slot.
- **Removing bad points**: `propose_history_removal { underlying, from_date, to_date }` proposes removing the active points in an inclusive date range, for example a bad print. The action records the hash of the points in the range and executes only if they are unchanged.
- **Canonical encoding** (so an off-chain script can reproduce the hash): for each point in ascending date order, the date as a 4-byte big-endian `u32`, then the price as a 16-byte big-endian `i128` in fixed point with 12 decimals (section 1.5). The points are concatenated with no separator and no header. Decimal inputs are converted to 12 decimals by truncation, exactly as the canister converts `PricePointInput`. The reference off-chain script that fetches a public series and computes this hash ships with the backfill tooling, before the first upload. It is not part of this PR and not part of the calendar loader script.
- **Activation** merges the upload into the history: uploaded points replace existing points on the same dates and new dates are inserted. The merge appends, sorts by date and keeps the incoming point on each duplicate date, in `O((n + m) log(n + m))`. The upload keeps its metadata and hash; its points are dropped once merged.
- **Preconditions and limits**: an upload is accepted only for an underlying already configured with `set_underlying_risk_params`, so every history has parameters. At most 8 uploads can be open or sealed at a time, an underlying holds at most 512 calendar events, and the pending-action caps of section 1.2 apply.
- **Daily sampling is built but disabled** (`risk::sampler::SAMPLER_ENABLED = false`). The only live price today is the collateral oracle, written by a single authorized principal through `update_asset_price`. The design retires that path because one bad print would stay in the history for years and distort the long-run volatility and the floor. Phase 2 turns the sampler on, fed by the robust multi-source index, and samples at a **fixed daily time** (a set UTC cut, so every point of every underlying is observed at the same moment of the day and the 24-hour return spacing holds), not at whatever hour the timer first fires. Until then the history grows only through hash-checked, timelocked backfill. The code (`sample_daily`) stays compiled and unit tested so enabling it is a one-line change.
- **Calendar hygiene**: `add_calendar_events` rejects events in the past and exact duplicates (same time, weight and label, against the calendar or within the batch), and prunes events that ended more than a day ago before checking the cap.

### 1.4 Volatility (doc sections 3, 6.1 to 6.4)

- **Returns** between consecutive history points `a < b`: `r = ln(P_b / P_a)`, normalized by the volatility time elapsed, `x = r^2 / tau` with `tau = sum of day weights for the days a+1 ..= b`. Day weight is `weekend_weight` on Saturday and Sunday, `1` otherwise. Historical returns are normalized by weekends only: event weights apply to the future horizon, so that event variance stays in the history the model learns from.
- **EWMA** (section 6.1): seeded with the mean of the first 20 normalized returns, then `sigma2 = lambda x sigma2 + (1 - lambda) x x_t` for each following return. At least 21 returns are required.
- **Long-run variance**: the mean of all normalized returns in the history, published only once the history holds at least 250 returns (about a year of trading days). Below that the reading is **provisional**: `is_provisional = true`, `sigma_long_run` and `sigma_used` are absent, and only the EWMA is published. A mean over a few weeks is not a long-run anchor, and a floor built on it would be noise.
- **Floor** (section 6.2): `sigma_used = max(sigma_ewma, floor_ratio x sigma_long_run)`, from 250 returns on.
- **Volatility time over the MPOR** (section 6.3): the sum, over the `mpor_days` calendar days starting today (whole UTC days, aligned with the history's dates), of each day's weight plus, for each future calendar event on that day, `(weight - 1) x day_weight x ramp`, with `ramp = 1 - (event_time - now) / mpor`. Each day's total is clamped at zero. An event at the edge of the horizon contributes nothing and reaches its full weight as it arrives: there is no step.
- **Holidays** are calendar events with a weight below 1 (for example 0.2 for a closed FX market): they lower their own day, and the per-day clamp keeps several of them from driving a day negative.
- **Multiplier composition** (section 3): `k_eff = k x global_k`. The `stale_factor` arrives with the price pipeline in phase 2.
- All volatility figures are daily, per unit of volatility time.

### 1.5 Fixed-point arithmetic (doc section 7.7)

- `Fixed`: a signed 128-bit integer with 12 decimals (`SCALE = 10^12`). No `f64` anywhere in this module.
- Multiplication and division truncate toward zero. Every operation is checked and returns an error instead of overflowing.
- `sqrt` returns the floor of the exact square root (integer Newton iteration).
- `ln` reduces the argument to `[1, 2)` by powers of two, then sums the series `2 x atanh(y)` with `y = (m - 1) / (m + 1)`, to an absolute error below `10^-10` on the history's price ratios (the worst case measured in the tests is about `1.6 x 10^-11`).
- At the API boundary values travel as `DecimalValue` (`value`, `decimals`): inputs are converted with truncation, outputs are returned with 12 decimals.

### 1.6 Event calendar loader (doc section 6.3)

The calendar is filled by an operator script, `scripts/calendar/`, never by dates written in code.

- `build.mjs` (Node, no dependencies) fetches each publisher's official schedule, parses its future events and writes a reviewable `calendar.json` with the source URL of every event. It never contacts the canister. No date is invented: a parser that no longer recognizes its page fails loudly, and the run exits non-zero unless `--allow-partial` is given. Times are converted to UTC with the time-zone database built into Node.
- `load.sh` loads that file through `add_calendar_events`. Dry run is the default; `--apply` is the only write path. It reads `list_calendar_events` first and skips events already present, drops anything less than 10 minutes away, respects the 512-event cap net of what the canister prunes, and on a rejection for a past or duplicate event re-reads the canister and retries once. An underlying without risk parameters is reported and skipped.
- `config.json` holds everything tunable: the provisional weights (central-bank decision 3, US CPI 2, US payrolls 2, to be recalibrated from realized variance), release times and time zones, the mapping of sources to underlyings (each FX underlying gets its central bank plus the US events, crypto gets the US events), and the publishers that cannot be read.
- Sources read automatically today: Federal Reserve (FOMC), ECB, Bank of England, Bank of Japan, RBA, SNB, Bank of Canada, RBI and Bank of Korea. Release times are published by the source except for the Bank of Japan, RBI and Bank of Korea, where a documented convention is used.
- US CPI and payrolls come from the BLS release schedule through FRED, because the BLS site refuses automated clients. They need a free `FRED_API_KEY`; without it they are skipped with a warning and every other source still builds.
- Not read automatically: RBNZ (403), Banxico (dates filled in by the browser), Banco Central do Brasil (no machine-readable schedule), Banco Central de Chile (bot challenge), MAS (only "no later than" dates), the Central Bank of the Republic of China (no English schedule) and the People's Bank of China (no fixed schedule). For NZD, MXN, BRL, CLP, SGD, TWD and CNY only the US events load until they are added by hand from the official pages.
- The first real load waits for a risk administrator, which is still to be decided (section 1.1). It was exercised against a locally deployed canister up to the role guard.

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
| `src/clearing/src/risk/sampler.rs`                                                                    | Sampling timer, disabled in phase 1                                  |
| `src/clearing/src/api/risk/{mod,api,params,results,errors}.rs`                                        | Endpoints, following `.agents/patterns.md`                           |
| `src/clearing/src/guards.rs`                                                                          | `caller_is_risk_admin`                                               |
| `src/clearing/src/memory.rs`, `src/clearing/src/types/state.rs`, `src/clearing/src/migrations/mod.rs` | Persistence of `RiskState`                                           |
| `src/clearing/src/lib.rs`                                                                             | Module, endpoint imports, sampler timer on `init` and `post_upgrade` |
| `src/clearing/Cargo.toml`                                                                             | `sha2` (already a workspace dependency)                              |
| `src/clearing/tests/it/risk.rs`                                                                       | Integration tests                                                    |
| `scripts/calendar/{build.mjs,load.sh,config.json,README.md}`                                          | Event calendar loader (section 1.6)                                  |

## 3. Endpoints

Updates:

| Endpoint                                                              | Caller                   | Effect                                                                                                                     |
| --------------------------------------------------------------------- | ------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| `add_risk_admin { principal }`                                        | controller               | Proposes adding a risk administrator (timelocked); returns the action id                                                   |
| `remove_risk_admin { principal }`                                     | controller               | Removes immediately                                                                                                        |
| `set_underlying_risk_params(...)`                                     | risk admin               | Sets `lambda`, `weekend_weight`, `floor_ratio`, `mpor_days`, `oracle_asset_id`; creates the underlying with `k = 1` if new |
| `set_risk_multiplier { underlying, k }`                               | risk admin or controller | Applied if raising; pending if lowering (risk admin only)                                                                  |
| `set_global_risk_multiplier { k }`                                    | risk admin or controller | Same rule                                                                                                                  |
| `set_timelock_duration { duration_ns }`                               | risk admin               | Applied if lengthening up to 2x; pending otherwise                                                                         |
| `add_calendar_events { underlying, events }`                          | risk admin               | Adds events; returns their ids                                                                                             |
| `remove_calendar_event { underlying, event_id }`                      | risk admin               | Removes one event                                                                                                          |
| `begin_history_upload`, `append_history_chunk`, `seal_history_upload` | risk admin               | Backfill (section 1.3)                                                                                                     |
| `abandon_history_upload { upload_id }`                                | risk admin or controller | Drops an open or sealed upload and its pending activation                                                                  |
| `propose_history_removal { underlying, from_date, to_date }`          | risk admin               | Proposes removing a date range of the active history (timelocked)                                                          |
| `cancel_pending_action { action_id }`                                 | risk admin or controller | Cancels; a cancelled upload activation also cancels the upload                                                             |
| `execute_pending_action { action_id }`                                | risk admin or controller | Executes once the delay has passed, after re-validation                                                                    |

Queries, all public: `get_risk_governance` (risk administrators and global parameters), `list_underlying_risk_params`, `list_calendar_events { underlying }`, `list_pending_actions`, `get_history_upload { upload_id }`, `get_history_upload_points { upload_id, from_date, limit }`, `get_price_history`, `get_volatility { underlying }` (EWMA, long-run and floored volatility once not provisional, `is_provisional`, observations, last date, volatility time over the MPOR and `k_eff`), `list_risk_audit`. Queries that return risk figures return a `Result`: a negative figure would mean a bug, so it is an error rather than a zero.

Every state change appends an entry to a public **risk audit log** (`list_risk_audit`), attributed to its caller. This is how phase 1 meets "every change emits an event" without touching the trading `Event` stream, whose variants drive trade history and leaderboards.

## 4. Storage and migration

- All phase-1 state lives in one record, `RiskState`, held in a `RISK_STATE` thread-local and persisted as a new **optional** field `risk: Option<RiskState>` of `StableState`.
- Per `.agents/patterns.md` section 5 and `docs/ai/migrations.md`, adding an `opt` field needs no migration: blobs written before this release decode with `risk = None`, which restores a default `RiskState` (no administrators, default global parameters, empty histories).
- `migrations::into_current` sets `risk: None` for the pre-resolution legacy schema.
- Tested both ways: a shadow of the pre-risk `StableState` (every field except `risk`) decodes into the current one with `risk` absent and every other field byte-identical, and a PocketIC upgrade keeps a pending action and an activated history (and leaves the sampler off).
- Footprint: about 3,650 points per underlying for ten years of history, 20 bytes each when encoded.

## 5. Tests

Unit:

- `Fixed`: conversions to and from `DecimalValue`, truncation, overflow errors, `sqrt` against exact squares and non-squares, `ln` against known values (ln 2, ln 10, ln of ratios near 1).
- EWMA against a hand-computed sequence; long-run mean; floor applied only when it binds.
- Returns across a weekend with `weekend_weight` 0.3 and 1.
- Volatility time: weekends, an event at the horizon edge (no contribution), halfway (half), today (full), a past event (none).
- Bounds: every parameter at, just inside and just outside its bounds.
- Timelock: propose, list, cancel, early execution rejected, execution after the delay, re-validation at execution, a stale `from` value rejected, one pending action per key, a raise withdrawing a pending lowering, controller actions never crowded out.
- `k`: raise applied immediately, lower pending, controllers may raise but not lower, composition `k x global_k`.
- Timelock duration: lengthening up to 2x immediate, beyond 2x and shortening timelocked, bounds.
- Roles: risk endpoints reject non-administrators and controllers; a controller cannot be made a risk administrator.
- Backfill: chunk validation (ordering, positivity, future dates, limits), hash mismatch rejected, matching hash proposes activation, activation merges and replaces dates, a 20,000 + 20,000 point merge of older overlapping data, abandoning open and sealed uploads, timelocked removal of a date range and its rejection after the range changed.
- Calendar: past and duplicate events rejected, ended events pruned, holidays lowering their own day without going negative.
- Audit log: entry contents (sequence, time, caller, description).
- Sampling: appends today from fresh oracle data, skips stale data and existing dates.
- Persistence: `StableState` without `risk` decodes to the default `RiskState`.

Integration (PocketIC): adding a risk administrator through the timelock by advancing time; `add_risk_admin` on a controller rejected; uploading (a future date rejected), sealing with an off-chain hash, paging the sealed points and activating a backfill; reading a provisional volatility; raising and lowering `k`, a controller raise withdrawing a pending lowering; rejection of an early execution, of an outsider executing, of non-administrator callers and of a removed administrator; audit entry contents; an upgrade with a pending action and a history, with the sampler staying off.

## 6. Deviations from the design

- **Audit log instead of trading events** (section 3 of this plan).
- **Historical returns normalized by weekends only**, not by event weights (section 1.4), so that event variance stays in the history.
- **Phase-1 parameters other than `k` change immediately** because nothing consumes them yet; phase 6 moves their margin-lowering direction behind the timelock (section 1.2).
- **Volatility is daily** per unit of volatility time; annualization is not needed until marks and margin use it.
- **Backfill is a risk-administrator operation**, not a controller one: the data feeds the risk model, so the role that owns the model owns its inputs, and the timelock plus the public hash give controllers and users the delay to object.
- **The sampler is disabled** until phase 2 (section 1.3).
- **Long-run volatility and the floor wait for 250 returns** (section 1.4).
- **Holidays are calendar events with weight below 1** rather than a separate holiday calendar (section 1.4).
- **The `ln` error bound is below `10^-10`**, measured about `1.6 x 10^-11` (section 1.5).
- **MPOR defaults are unchanged** (3 days for every underlying). Whether FX and crypto need different defaults is left to a separate decision.
- **No pre-filled calendar in code.** The design asks for a calendar pre-filled with central-bank meetings and data releases. Those dates must come from the publishers' official schedules, so they are loaded through `add_calendar_events` by the operator script of section 1.6, not hard-coded here.
