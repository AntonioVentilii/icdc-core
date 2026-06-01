# Server-side "open markets" discovery

Status: implemented (icdc-core). Front-end (vici-app) wiring is a follow-up PR.

## Problem

The Vici flow (swipe deck) built its candidate set by calling the registry's
`list_series` with no limit — fetching **every** series ever created across all
domains and payoff types — then filtering client-side to: open (binary payoff),
in the user's balance domain, not yet expired (`expiry_ns > now`), and **not
resolved**. "Not resolved" was reverse-engineered on the client by downloading
the global activity log and scanning for settlement events.

Both the full-series fetch and the resolution reconstruction scale badly and are
the root cause of flow-mode slowness.

## What changed

Two fast `query` (non-replicated) reads now let the front end fetch only the
currently-tradeable set, filterable by `balance_domain` and `payoff_type`, with
stable cursor pagination. No update calls, no activity-log scan.

### Registry — expiry filter (self-contained)

`ListSeriesParams` gains one optional field:

```candid
type ListSeriesParams = record {
  // ... existing fields unchanged ...
  only_unexpired : opt bool;   // NEW
};
```

- `Some(true)` excludes series whose `expiry_ns <= now`, where `now` is the
  registry canister's own `time()` evaluated **server-side** (the caller cannot
  widen the window with a stale/forged clock).
- `Some(false)` / `None` apply no expiry filtering — i.e. the existing
  `list_series` / `list_series_with` behavior is preserved for all current
  callers. The field is additive and optional.

Both `list_series` and `list_series_with` are unchanged in signature
(`ListSeriesParams` is the carrier); they remain `query`. Pagination is
unchanged (cursor over the `SeriesId`-ordered `BTreeMap`).

The registry owns `expiry_ns`, so this half is entirely self-contained.

### Clearing — resolution set (new query)

```candid
type ListSettledSeriesParams = record {
  balance_domain : opt BalanceDomain;  // optional domain filter
  start_after    : opt text;           // exclusive cursor (SeriesId)
  limit          : opt nat64;          // None = all remaining
};

type SettledSeriesPage = record {
  items       : vec text;   // settled SeriesIds, ascending
  next_cursor : opt text;   // last returned id; pass as start_after to continue
};

service : {
  // ...
  list_settled_series : (ListSettledSeriesParams) -> (SettledSeriesPage) query;
};
```

- Returns the ids of series that have been **settled (resolved)**. A series
  appears here as soon as a `SettlementPlan` is opened for it (any
  `PlanStatus`), which is precisely when it stops being tradeable.
- Guarded by `caller_is_not_anonymous`, matching `get_settlement_status`.
- Ids come from `SETTLEMENT_PLANS` (a `BTreeMap<SeriesId, _>`) in ascending
  order; cursor pagination is stable as long as the set isn't mutated
  mid-traversal. Cursor convention matches `backfill_settlement_events`:
  `start_after` is **exclusive**, and `next_cursor` is the **last id returned**
  (so resuming neither drops nor repeats an id).

## Resolution-status design decision

The registry is the catalog source of truth; it has **no** notion of resolution
(`Series` carries `expiry_ns`/`created_at_ns` but no status field, and the
registry crate has zero references to settlement). Settlement/resolution state
lives authoritatively in the **clearing** canister's `SETTLEMENT_PLANS`. On
finalization clearing removes the series from its own `SERIES` mirror, but the
registry retains the catalog entry — so the registry's list still includes
resolved markets.

Two options were considered:

- **(a) Expose a clearing query of settled ids; the FE subtracts.** ✅ chosen.
- **(b) Push a "resolved" flag onto the registry's `Series` (clearing notifies
  the registry on settlement) so `list_series_with` filters it directly.**

**Chosen (a)** because it keeps a single source of truth:

- Settlement state stays owned by clearing. Option (b) duplicates it into the
  registry (dual write), introducing an eventual-consistency window where the
  two canisters disagree.
- Option (b) adds a cross-canister **update** on the already-chunked,
  multi-call settlement hot path. That settlement flow is governed by strict
  atomicity/economic-safety rules; making it depend on a registry notify that
  must succeed/retry complicates failure reasoning for no functional gain.
- Option (a) is two independent **query** calls — both fast and non-replicated,
  with no new coupling and no new settlement failure mode.

Cost of (a): the FE intersects two paginated sources (subtract the settled set
from the open/unexpired page). For a flow deck this is acceptable — a page may
yield slightly fewer than `limit` tradeable candidates after subtraction, which
is strictly better than the previous "fetch everything + scan the log" path.

## Front-end wiring (separate vici-app PR)

1. `npm run did` to regenerate bindings (already done in icdc-core; the FE
   regenerates its own).
2. Flow lite-fetch:
   - `registry.list_series_with({ balance_domain, payoff_type = Binary,
only_unexpired = opt true, pagination })`
   - `clearing.list_settled_series({ balance_domain, ... })` → build a `Set`
   - candidate set = open/unexpired page minus settled ids.
3. Drop the activity-log-derived resolution map entirely.

## Tests

- `shared` / `registry`: `only_unexpired` boundary (strict `>` at `now == expiry`),
  exclusion of expired series, and unset-filter legacy behavior.
- `clearing`: `list_settled_series` ascending order, `balance_domain` filter,
  stable cursor pagination across pages, and the empty case.

Integration (pocket-ic) tests require the downloaded `ledger`/`index` wasm
fixtures (`target/ic/…`); they are unaffected by this change.
