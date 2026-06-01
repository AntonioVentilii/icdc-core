# Series price-history (time-bucketed consensus) query

Status: implemented (icdc-core). Front-end (vici-app) wiring is a follow-up PR.

Closes [icdc-core#51](https://github.com/AntonioVentilii/icdc-core/issues/51).

## Problem

The vici-app market UI renders **time-scoped charts** — the market-detail period chips (`1d / 7d / 30d / all`) and the flow-card / market-detail sparklines — but the engine had no historical consensus source at chart resolution. `list_series_trade_history` (icdc-core#60) exposes the **raw per-trade tape** for a series, which is the right primitive for a fine sparkline but forces the front end to fetch and re-bucket the _entire_ tape client-side to draw a `30d` or `all` chart, and grows unbounded as a market trades. The period chips therefore stayed a visual-only switch and the sparklines fell back to synthetic, id-seeded data.

## What changed

A new fast `query` (non-replicated) read returns a series' executed trades **aggregated into fixed-width time buckets** (OHLC + volume + trade count), time-ordered. The front end picks the resolution and window and renders the candles directly. Read-only; no settlement or correctness implications.

### Clearing — series price history (new query)

```candid
type PriceHistoryInterval = variant { Hour; Day };

type GetSeriesPriceHistoryParams = record {
  series_id  : text;                  // series whose trades to aggregate
  interval   : PriceHistoryInterval;  // bucket width
  start_time : opt nat64;             // inclusive lower bound on trade ts (ns)
  end_time   : opt nat64;             // exclusive upper bound on trade ts (ns)
};

type SeriesPriceCandle = record {
  bucket_start_ns : nat64;  // bucket start (ns), epoch-aligned to interval
  open            : Price;  // first trade in the bucket
  high            : Price;  // max trade price in the bucket
  low             : Price;  // min trade price in the bucket
  close           : Price;  // last trade in the bucket — the consensus the FE maps to 0..1
  volume          : int;    // summed traded qty in the bucket
  trade_count     : nat64;  // executed trades in the bucket
};

type SeriesPriceHistory = record {
  candles : vec SeriesPriceCandle;  // ascending by bucket_start_ns; empty buckets omitted
};

service : {
  // ...
  get_series_price_history : (GetSeriesPriceHistoryParams) -> (SeriesPriceHistory) query;
};
```

- One candle per bucket that contains at least one executed trade on `series_id`. Buckets are **fixed-width and epoch-aligned** (`[k·width, (k+1)·width)` ns), so two calls over an overlapping range return byte-identical candles for the shared buckets and the same instant always lands in the same bucket. `Hour` backs the short windows (`1d` = 24 candles, `7d` = 168); `Day` backs the long ones (`30d` = 30, `all` = one per active day).
- `close` is the bucket's consensus the front end maps to a 0..1 YES probability (last trade price); `open`/`high`/`low` let it draw candlesticks; `volume`/`trade_count` back volume overlays. As with `list_series_trade_history`, mapping `Price` → consensus is the front end's job — the engine stays payoff-agnostic (binary vs. categorical).
- `start_time`/`end_time` window the trades considered (inclusive lower, exclusive upper), so the caller fetches just the window it draws.
- **Empty/partial is graceful.** An unknown series, an untraded series, or a window with no trades returns an empty `candles` vector — no fabricated points — and buckets with no trades are simply absent (gaps the chart leaves as gaps).
- Guarded by `caller_is_not_anonymous`, matching `list_series_trade_history`, `get_trade_history`, and `list_settled_series`.

## Performance / cycles design decision

Served from the **same `SERIES_TRADE_HISTORY` index** that backs `list_series_trade_history` (icdc-core#60) — the per-series `Vec<SeriesTradePoint>`, one point per trade in ascending `event_id` (execution) order. So this endpoint:

- **Adds no write-path cost and no persisted state.** It is a second read-side projection of an index that is already maintained on the trade-execution path and rebuilt in `post_upgrade`; the `StableState` layout is unchanged (no migration).
- **Aggregates in `O(trades in range)`.** Each trade folds into its bucket via a `BTreeMap<bucket_start, candle>` (the map keeps buckets time-ordered with no separate sort). Because the index is in execution order — which is chronological, since trades execute sequentially and a higher `event_id` never carries an earlier timestamp — the last point folded into a bucket is its `close`, and the first its `open`, with no per-bucket sort.
- **Bounds the response.** Aggregated output is far smaller than the raw tape, but an open-ended range at hourly resolution over a long-lived market could still grow without limit, so the result is capped at `MAX_PRICE_HISTORY_POINTS = 1000` candles, keeping the **most recent** buckets (a chart reads the latest window). In normal use the front end's window + interval stay well under the cap.

### Why a separate query from `list_series_trade_history`

The two serve different shapes of the same underlying data and neither subsumes the other:

- `list_series_trade_history` — the **raw tape**, cursor-paginated, one point per trade. Right for a high-resolution recent sparkline and for clients that want every fill.
- `get_series_price_history` — **bounded, chart-ready candles** at a chosen resolution. Right for the `1d / 7d / 30d / all` period chips, where fetching the whole tape to re-bucket client-side is wasteful and unbounded.

Keeping both, sharing one index, avoids duplicating either the storage or the maintenance.

## Front-end wiring (separate vici-app PR)

1. `npm run did` to regenerate bindings (already done in icdc-core; the FE regenerates its own).
2. Wire `MarketDetailChartCard` to call `clearing.get_series_price_history({ series_id, interval, start_time, end_time })` on `activePeriod` change — `Hour` for `1d`/`7d`, `Day` for `30d`/`all`, with `start_time` set to the window's lower bound — and plot each candle's `close` as the consensus (and `volume`/OHLC where the chart supports it).
3. Back the flow-card / market-detail sparklines with the same query (or the raw `list_series_trade_history` tape where a finer recent sparkline is wanted) and drop the id-seeded synthetic series.

Cross-ref: the raw-tape primitive in [series-trade-history-query.md](series-trade-history-query.md) and vici-app PR #385.

## Tests

`clearing` (`src/clearing/src/api/trade/tests/price_history.rs`): hourly and daily bucketing, OHLC extremes tracked independently of open/close, inclusive-start / exclusive-end windowing, buyer/seller rows collapsing to one trade (volume + count), executed-only filtering (settlement/liquidation ignored), the empty cases (unknown series, window with no trades), and the most-recent-points cap. Bucket-alignment math is unit-tested in `src/clearing/src/types/price_history.rs`.

Integration (pocket-ic) tests require the downloaded `ledger`/`index` wasm fixtures (`target/ic/…`); they are unaffected by this change.
