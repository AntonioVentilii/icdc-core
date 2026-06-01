# Series-scoped trade / price-history query

Status: implemented (icdc-core). Front-end (vici-app) wiring is a follow-up PR.

Closes [icdc-core#59](https://github.com/AntonioVentilii/icdc-core/issues/59).

## Problem

Clearing only exposed `get_trade_history : () -> (vec Event) query`, which is
**caller-scoped**: it returns the calling principal's own position-moving events
(executed/settled/liquidated), with no series filter. There was no way to read a
**market/series-scoped** trade or price history.

The vici-app market UI wants a real price-history sparkline on the market detail
page and the flow card — the YES-probability over time for a given series,
derived from that series' executed trades. With only the caller-scoped query the
front end could plot only the _viewer's own_ trades on a market (flat for markets
the viewer never traded). The interim FE shipped in vici-app PR #385 documents
this limitation.

## What changed

A new fast `query` (non-replicated) read returns the **executed-trade events for
a single series**, with stable cursor pagination — enough to derive a
YES-probability sparkline from the per-trade `price`/`timestamp`. Read-only; no
settlement or correctness implications.

### Clearing — series trade history (new query)

```candid
type TradeHistoryCursor = record {
  timestamp : nat64;  // ns of the last event in the previous page
  event_id  : nat64;  // id of the last event in the previous page
};

type ListSeriesTradeHistoryParams = record {
  series_id   : text;                     // series whose trades to return
  start_after : opt TradeHistoryCursor;   // exclusive cursor
  limit       : opt nat64;                // None = all remaining
};

type SeriesTradeHistoryPage = record {
  items       : vec Event;                // executed events, (timestamp, event_id) ascending
  next_cursor : opt TradeHistoryCursor;   // pass back as start_after to continue
};

service : {
  // ...
  list_series_trade_history : (ListSeriesTradeHistoryParams) -> (SeriesTradeHistoryPage) query;
};
```

- Returns only `EventType::Executed` events for `series_id`. Those carry the
  matched trade `price` the sparkline plots; settlement and liquidation events
  are excluded (they don't represent a market clearing price).
- Guarded by `caller_is_not_anonymous`, matching `get_trade_history` and
  `list_settled_series`.
- Events are ordered by `(timestamp, event_id)` so backfilled rows (whose
  timestamp reflects the original settlement/trade time) interleave
  chronologically rather than appearing in storage-insertion order — identical
  to `get_trade_history`'s ordering.
- Cursor convention mirrors `list_settled_series` / `backfill_settlement_events`:
  `start_after` is **exclusive**, `next_cursor` is the **last event returned**
  (so resuming neither drops nor repeats an event), and a `limit` of 0 is clamped
  to 1 so a paging caller always makes forward progress.

## Cursor design decision

`EVENTS` is a flat `Vec<Event>` keyed implicitly by insertion, with a separate
monotonically-increasing `event_id`. The query sorts by `(timestamp, event_id)`,
so the resume cursor must carry **both** fields, not just `event_id`:
`backfill_settlement_events` synthesizes events with old timestamps but new ids,
so `event_id` is not monotonic in timestamp order. A bare `event_id` cursor would
either skip or repeat backfilled rows. `TradeHistoryCursor` therefore records
`(timestamp, event_id)` and the page is resumed with a `partition_point` over the
sorted slice.

Two options were considered for storage:

- **(a) Filter + sort the existing `Vec<Event>` per query.** ✅ chosen.
- **(b) Add a per-series secondary index (`BTreeMap<SeriesId, …>`) to make the
  read O(page) instead of O(events).**

**Chosen (a)** because it adds no new persisted state, no migration, and no
second structure to keep consistent on every trade/settlement/backfill write. The
clearing event log is bounded by realistic trade volume per canister and the
query is a non-replicated `query`; if log size ever makes the per-query scan a
problem, (b) is a self-contained follow-up that doesn't change this endpoint's
candid shape.

## Front-end wiring (separate vici-app PR)

1. `npm run did` to regenerate bindings (already done in icdc-core; the FE
   regenerates its own).
2. Rewire `MarketDetailChartCard` / `FlowCardSparkline` (via
   `market-price-history.utils`) to page `clearing.list_series_trade_history({
series_id, start_after, limit })` and derive the YES-% series from each event's
   `price`/`timestamp`.
3. Drop the caller-scoped derivation that plotted only the viewer's own trades.

Cross-ref: vici-app sparkline interim in PR #385.

## Tests

- `clearing`: series + executed-only filtering, `(timestamp, event_id)`
  ordering (including a backfilled-style row whose id disagrees with timestamp
  order), stable exclusive-cursor pagination across pages, the exact-boundary
  no-cursor case, `limit = 0` forward progress, and the unknown-series empty
  case (`src/clearing/src/api/trade/tests/series_history.rs`).

Integration (pocket-ic) tests require the downloaded `ledger`/`index` wasm
fixtures (`target/ic/…`); they are unaffected by this change.
