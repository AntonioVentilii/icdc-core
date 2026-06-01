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
type SeriesTradePoint = record {
  event_id  : nat64;   // strictly increasing in execution order; doubles as cursor
  price     : Price;   // execution price
  qty       : int;     // traded quantity (positive)
  timestamp : nat64;   // execution time (ns)
};

type ListSeriesTradeHistoryParams = record {
  series_id   : text;        // series whose trades to return
  start_after : opt nat64;   // exclusive cursor = last returned event_id
  limit       : opt nat64;   // None = all remaining
};

type SeriesTradeHistoryPage = record {
  items       : vec SeriesTradePoint;   // one point per trade, event_id ascending
  next_cursor : opt nat64;              // last event_id; pass as start_after to continue
};

service : {
  // ...
  list_series_trade_history : (ListSeriesTradeHistoryParams) -> (SeriesTradeHistoryPage) query;
};
```

- Returns one `SeriesTradePoint` **per executed trade** on `series_id`, ordered
  by `event_id` (execution order). Each trade carries the matched `price` the
  sparkline plots plus `qty` (volume); settlement and liquidation events are
  excluded (they don't represent a market clearing price).
- Guarded by `caller_is_not_anonymous`, matching `get_trade_history` and
  `list_settled_series`.
- Cursor convention mirrors `list_settled_series`: `start_after` is
  **exclusive**, `next_cursor` is the **last `event_id` returned** (so resuming
  neither drops nor repeats a trade), and a `limit` of 0 is clamped to 1 so a
  paging caller always makes forward progress.

## Performance / cycles design decision

An executed trade emits **two** `Event` rows (buyer + seller) that share one
`event_id`, `price`, and `timestamp`. The naive query — filter the flat
`EVENTS: Vec<Event>` by `series_id`, clone all matches, sort, paginate — is
`O(total events)` in compute **and** clones every match before discarding all
but one page, on a read that fires on essentially every market view. It also
returns both counterparty rows (duplicate price points) and the duplicate
`(timestamp, event_id)` key breaks an exclusive cursor that lands between a
trade's two rows.

Chosen design: a **per-series price-history index** maintained on write.

- `SERIES_TRADE_HISTORY: BTreeMap<SeriesId, Vec<SeriesTradePoint>>`, one point
  per trade, appended in execution order so each vector is already sorted by
  `event_id`. The query is then `O(log series + page)`: map lookup +
  `partition_point` for the cursor + clone of just the page.
- **Heap-only, rebuilt on upgrade.** The index is a pure projection of `EVENTS`,
  so it is **not** added to the persisted `StableState` — `post_upgrade`
  reconstructs it via `rebuild_series_trade_history` after `EVENTS` is restored.
  This avoids a stable-layout migration and keeps the persisted state minimal;
  the cost is one extra `O(N)` pass per upgrade, which is negligible (upgrades
  are rare and `post_upgrade` is already `O(N)`).
- **Dedup + bare cursor.** Collapsing the buyer/seller pair to one point per
  trade is what a sparkline wants, halves the payload, and makes `event_id` a
  unique, strictly-increasing key — so the cursor is a bare `nat64` instead of a
  `(timestamp, event_id)` tuple, and the duplicate-key edge case disappears.
- **Privacy.** `SeriesTradePoint` omits the per-trade buyer/seller principal, so
  this market-wide read exposes only the public trade tape (price, qty, time),
  not who traded.

Write cost is one `Vec::push` per trade at the single execution site
(`trade::service::internal_execute_trade`); trades are far rarer than sparkline
reads, so moving work to the write path is the right trade. If per-series heap
ever matters, the index could store `EVENTS` offsets instead of cloned points —
a self-contained follow-up that doesn't change this endpoint's candid shape.

## Front-end wiring (separate vici-app PR)

1. `npm run did` to regenerate bindings (already done in icdc-core; the FE
   regenerates its own).
2. Rewire `MarketDetailChartCard` / `FlowCardSparkline` (via
   `market-price-history.utils`) to page `clearing.list_series_trade_history({
series_id, start_after, limit })` and derive the YES-% series from each point's
   `price`/`timestamp`.
3. Drop the caller-scoped derivation that plotted only the viewer's own trades.

Cross-ref: vici-app sparkline interim in PR #385.

## Tests

- `clearing` (`src/clearing/src/api/trade/tests/series_history.rs`): series +
  executed-only filtering, buyer/seller rows collapsing to one point per trade,
  `event_id` ordering (jumbled seed sorted on rebuild), stable exclusive-cursor
  pagination across pages, the exact-boundary no-cursor case, `limit = 0`
  forward progress, the unknown-series empty case, and a live-execution test
  asserting the write path (`internal_execute_trade`) populates the index — not
  just the `post_upgrade` rebuild.

Integration (pocket-ic) tests require the downloaded `ledger`/`index` wasm
fixtures (`target/ic/…`); they are unaffected by this change.
