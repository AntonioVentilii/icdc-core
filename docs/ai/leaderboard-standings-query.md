# Per-window leaderboard standings query

Status: implemented (icdc-core). Front-end (vici-app) wiring is a follow-up PR.

Closes [icdc-core#56](https://github.com/AntonioVentilii/icdc-core/issues/56).
Counterpart: [vici-app#353](https://github.com/AntonioVentilii/vici-app/issues/353).

## Problem

vici-app has three leaderboard / standings stats whose UI already exists (tabs,
↑/↓ arrows, rank pills) but render placeholder data, because the underlying
per-window ranking lives in the clearing canister, not the satellite:

- **Leaderboard per window** — This week / This month / All time.
- **Rank delta** — each principal's prior-window rank, for ↑/↓ movement.
- **Your league rank** — a user's rank within their league's member set.

The clearing canister already records, per settled position, a `Settled`
[`Event`] whose `qty` is the position's signed `cashflow_usd` (realized PnL;
positive = winner). That is exactly the per-principal realized PnL the standings
rank by — but there was no query that aggregates it into ranked, windowed
standings. The satellite owns the social graph / league membership, the clearing
canister owns settlement, so the ranking primitive belongs here and the league
filter is supplied by the caller.

## What changed

A new `query` (non-replicated) read, `list_leaderboard`, returns ranked
standings for one calendar window, derived from settled-position PnL, with each
entry's prior-window rank and stable cursor pagination. Read-only; no settlement
or correctness implications.

```candid
type LeaderboardWindow = variant { Week; Month; AllTime };

type ListLeaderboardParams = record {
  window      : LeaderboardWindow;     // which window to rank
  members     : opt vec principal;     // rank within this league set; None = global
  start_after : opt nat64;             // exclusive cursor = entries already consumed
  limit       : opt nat64;             // None = all remaining
};

type LeaderboardEntry = record {
  principal     : principal;
  rank          : nat64;               // 1-based, competition ranking
  prior_rank    : opt nat64;           // rank in the preceding period, for ↑/↓
  realized_pnl  : int;                 // net vUSD over the window
  settled_count : nat64;               // settlements in the window
  win_count     : nat64;               // of those, net-positive (accuracy = win/settled)
};

type LeaderboardPage = record {
  items       : vec LeaderboardEntry;  // ascending rank, ties broken by principal
  next_cursor : opt nat64;
  total       : nat64;                 // ranked principals in the window
};

service : {
  // ...
  list_leaderboard : (ListLeaderboardParams) -> (LeaderboardPage) query;
};
```

- **Ranking metric.** Net realized PnL over the window — the signed sum of the
  principal's `Settled` cashflows — descending. `win_count / settled_count`
  gives the FE an accuracy figure without exposing the per-settlement breakdown.
- **Competition ranking.** Equal-PnL principals share a rank and the next
  distinct PnL skips the tied positions (e.g. `1, 2, 2, 4`), so a tie reads as a
  genuine tie. Display order within a tie is by principal ascending, for a
  stable, deterministic page.
- **Prior-window rank.** Each entry carries its rank in the immediately
  preceding period (last week / last month) so the FE can render the ↑/↓ delta.
  `None` for the `AllTime` window (no prior period) or for a principal absent
  from the prior period (a newcomer).
- **League rank.** With `members` set, only that set is ranked, in isolation,
  and **every** listed member is included — even one with no settlements in the
  window, with a zeroed aggregate — so "your rank within your league" covers the
  full membership. With `members = None`, the global standings list only
  principals who settled at least one position in the window.
- Guarded by `caller_is_not_anonymous`, matching `get_trade_history`,
  `list_settled_series`, and `list_series_trade_history`.
- Pagination is **offset-based**: `start_after` is the number of entries already
  consumed and `next_cursor` is the new running count. This differs from the
  id/key cursors of `list_settled_series` (`SeriesId`) and
  `list_series_trade_history` (`event_id`), whose rows carry a stable key to
  resume from; a ranking has no such per-entry key, so it pages by position over
  the window's deterministic order. As in those queries, a `limit` of 0 is
  clamped to 1 so a paging caller always makes forward progress.
- The caller-supplied `members` set is capped at 10,000 principals (it is
  ranked in full); a longer list is truncated.

### Window semantics — calendar, not rolling

`Week` is an ISO week (Monday 00:00:00 UTC through the following Sunday) and
`Month` is a UTC calendar month. **Calendar (fixed) buckets** were chosen over
rolling 7-/30-day spans because the prior-window rank only has an unambiguous
meaning against a discrete preceding period, and fixed buckets let the index key
on a stable `(window, period_id)` pair. If vici-app's tabs turn out to want
rolling windows instead, that is a localized change to `LeaderboardWindow`'s
period math (`src/clearing/src/types/leaderboard.rs`) and does not alter the
Candid shape.

Period ids are monotonic, so the prior period is `period_id - 1`. Month
bucketing uses Howard Hinnant's `civil_from_days` to map epoch days → (year,
month) → a monotonic month index.

## Performance / cycles design decision

A leaderboard tab is read on essentially every standings view; settlements
(writes) are far rarer. The naive query would scan the whole `EVENTS` log for
`Settled` rows, bucket them by window, then rank — `O(total events)` on every
call.

Chosen design: a **per-`(window, period)` aggregate index**, mirroring the
`SERIES_TRADE_HISTORY` precedent.

- `SETTLEMENT_LEADERBOARD`, keyed by `(window, period)` and holding a
  per-`User` `PnlAggregate`. Each settled position folds its signed
  `cashflow_usd` into its current week, month, and all-time buckets. A query
  then ranks only the two relevant buckets (current + prior period) —
  `O(bucket)` instead of `O(total events)`.
- **Heap-only, rebuilt on upgrade.** The index is a pure projection of the
  `Settled` rows in `EVENTS`, so it is **not** added to the persisted
  `StableState` — `post_upgrade` reconstructs it via `rebuild_leaderboard` after
  `EVENTS` is restored. No stable-layout migration. Period bucketing is a
  deterministic function of each event's `timestamp`, so the rebuild reproduces
  exactly the live buckets.
- **Maintained at both settlement write sites.** `index_settled_events(&emitted)`
  runs alongside the `EVENTS` append in both `apply_settlement_accounting_logic`
  (live settlement) and `backfill_settlement_events` (one-shot backfill), so the
  index stays current between upgrades. Write cost is three `BTreeMap` updates
  per settled position.
- **Privacy.** The shape is an aggregate: window totals (net PnL, settled/win
  counts) and rank only — never the per-series or per-settlement breakdown. A
  public leaderboard inherently associates principals with standings; this
  exposes no more than that, and the per-trade counterparty principals stay out
  of the market-wide reads (`list_series_trade_history` already omits them).

### Ranking only the page, not the whole population

The index removes the event-log scan, but ranking still has a per-call cost: a
window can hold many principals while the FE shows a page of ~25. Two further
optimizations keep that cost proportional to the page, not the population:

- **Bounded top-K selection for the current period.** A competition rank counts
  the strictly-higher PnLs, which for any shown entry are themselves within the
  top `end` of the population. So the query uses `select_nth_unstable_by` to
  partition the top `end` in `O(total)` and sorts only that prefix —
  `O(total + end·log end)` — instead of an `O(total·log total)` full sort on
  every call. The comparator is a strict total order (PnL desc, then principal),
  so the top-K is well-defined even across PnL ties. It degrades to a full sort
  only when the caller asks for everything (`limit = None`), i.e. never worse.
- **Sort-free prior ranks.** The prior period is needed only for the shown
  principals' `prior_rank` (the ↑/↓ delta), not a full prior ranking. Rather
  than sort the whole prior bucket, `global_prior_ranks` computes
  `1 + count(strictly-higher PnL)` for all shown principals in a single
  `O(bucket·log page)` pass (distinct shown PnLs as sorted targets, a
  range-increment tally resolved by a prefix sum). League queries
  (`members = Some`) are bounded by the member set, so they just rank it in
  full.

Both are pure read-path algebra over the same index — no extra state, no cache
to invalidate, output identical to a full-sort reference (asserted by a
randomized equivalence test over 600 cases across all windows, league filters,
cursors, and limits).

## Front-end wiring (separate vici-app PR)

1. `npm run did` to regenerate bindings (already done in icdc-core; the FE
   regenerates its own).
2. Page `clearing.list_leaderboard({ window, members, start_after, limit })` for
   each tab; render `rank`, the `prior_rank → rank` delta as ↑/↓, and
   `win_count / settled_count` as accuracy.
3. For "your league rank", pass the affiliation's member principals as
   `members` and read the caller's entry.

## Tests

- `src/clearing/src/types/leaderboard.rs`: window math — Monday-aligned weeks,
  sub-day resolution, calendar months across year boundaries and a leap year,
  all-time single period, prior-period `= id - 1`, no prior at epoch start, and
  the aggregate's win/loss folding.
- `src/clearing/src/api/leaderboard/api.rs`: PnL-descending competition ranking
  with ties, prior-window rank populated / `None` (newcomer, all-time), the
  `members` league filter (excludes non-members, includes inactive members
  zeroed), stable cursor pagination with `limit = 0` clamp, the empty/unknown
  window case, a write-path test feeding `Settled` events through
  `index_settled_events` and reading them back bucketed by window, and a
  randomized equivalence test asserting the optimized read path (bounded
  selection + sort-free prior ranks) matches a naive full-sort reference over
  600 cases across all windows, league filters, cursors, and limits.

`cargo test -p clearing --lib` — 81 passed (18 new). `npm run quality` and
`npm run did` pass.

[`Event`]: ../../src/clearing/src/types/event.rs
