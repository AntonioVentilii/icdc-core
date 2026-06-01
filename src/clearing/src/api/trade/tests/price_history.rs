use candid::Principal;
use shared::types::{Price, SeriesId};

use crate::{
    api::trade::{
        api::get_series_price_history, params::GetSeriesPriceHistoryParams,
        results::SeriesPriceCandle,
    },
    memory::{rebuild_series_trade_history, EVENTS},
    types::{
        event::{Event, EventType},
        price_history::PriceHistoryInterval,
        user::User,
    },
};

const HOUR: u64 = 3_600_000_000_000;
const DAY: u64 = 86_400_000_000_000;

fn price(value: u128) -> Price {
    Price::new(value, 0)
}

/// An executed trade row. `qty > 0` is the buyer side the index keeps;
/// `clearing_id`/`user` are immaterial to a market-wide read.
fn trade(
    event_id: u64,
    series_id: &SeriesId,
    price_value: u128,
    qty: i128,
    timestamp: u64,
) -> Event {
    Event {
        event_id,
        clearing_id: Principal::anonymous(),
        series_id: series_id.clone(),
        user: User(Principal::anonymous()),
        qty,
        price: price(price_value),
        event_type: EventType::Executed,
        timestamp,
    }
}

/// Seeds `EVENTS` and rebuilds the per-series price-history index from it — the
/// same projection `post_upgrade` performs.
fn seed(events: Vec<Event>) {
    EVENTS.with(|e| {
        let mut e = e.borrow_mut();
        e.clear();
        *e = events;
    });
    rebuild_series_trade_history();
}

fn history(
    series_id: &SeriesId,
    interval: PriceHistoryInterval,
    start_time: Option<u64>,
    end_time: Option<u64>,
) -> Vec<SeriesPriceCandle> {
    get_series_price_history(GetSeriesPriceHistoryParams {
        series_id: series_id.clone(),
        interval,
        start_time,
        end_time,
    })
    .candles
}

#[test]
fn buckets_trades_into_hourly_candles() {
    let series = SeriesId::from("series".to_owned());

    seed(vec![
        trade(0, &series, 10, 1, 0),        // hour 0
        trade(1, &series, 20, 2, HOUR / 2), // hour 0
        trade(2, &series, 15, 3, HOUR + 5), // hour 1
    ]);

    let candles = history(&series, PriceHistoryInterval::Hour, None, None);

    assert_eq!(candles.len(), 2, "two distinct hourly buckets");

    let h0 = &candles[0];
    assert_eq!(h0.bucket_start_ns, 0);
    assert_eq!(h0.open, price(10), "first trade in the hour");
    assert_eq!(h0.close, price(20), "last trade in the hour");
    assert_eq!(h0.high, price(20));
    assert_eq!(h0.low, price(10));
    assert_eq!(h0.volume, 3, "summed qty");
    assert_eq!(h0.trade_count, 2);

    let h1 = &candles[1];
    assert_eq!(h1.bucket_start_ns, HOUR);
    assert_eq!(h1.open, price(15));
    assert_eq!(h1.close, price(15));
    assert_eq!(h1.trade_count, 1);
}

#[test]
fn ohlc_tracks_extremes_independent_of_open_close() {
    let series = SeriesId::from("series".to_owned());

    // Within one hour: prices go 30 → 50 (high) → 20 (low) → 40 (close).
    seed(vec![
        trade(0, &series, 30, 1, 0),
        trade(1, &series, 50, 1, 1),
        trade(2, &series, 20, 1, 2),
        trade(3, &series, 40, 1, 3),
    ]);

    let candles = history(&series, PriceHistoryInterval::Hour, None, None);

    assert_eq!(candles.len(), 1);
    let c = &candles[0];
    assert_eq!(c.open, price(30));
    assert_eq!(c.close, price(40));
    assert_eq!(c.high, price(50));
    assert_eq!(c.low, price(20));
    assert_eq!(c.trade_count, 4);
}

#[test]
fn daily_interval_groups_trades_across_hours() {
    let series = SeriesId::from("series".to_owned());

    seed(vec![
        trade(0, &series, 10, 1, 0),          // day 0, 00:00
        trade(1, &series, 12, 1, 5 * HOUR),   // day 0, 05:00
        trade(2, &series, 14, 1, DAY + HOUR), // day 1
    ]);

    let candles = history(&series, PriceHistoryInterval::Day, None, None);

    assert_eq!(candles.len(), 2);
    assert_eq!(candles[0].bucket_start_ns, 0);
    assert_eq!(candles[0].trade_count, 2);
    assert_eq!(candles[0].close, price(12));
    assert_eq!(candles[1].bucket_start_ns, DAY);
    assert_eq!(candles[1].trade_count, 1);
}

#[test]
fn windows_by_inclusive_start_and_exclusive_end() {
    let series = SeriesId::from("series".to_owned());

    seed(vec![
        trade(0, &series, 10, 1, 0),        // hour 0
        trade(1, &series, 20, 1, HOUR),     // hour 1
        trade(2, &series, 30, 1, 2 * HOUR), // hour 2
    ]);

    // [HOUR, 2*HOUR): keeps only the hour-1 trade — start inclusive, end exclusive.
    let candles = history(
        &series,
        PriceHistoryInterval::Hour,
        Some(HOUR),
        Some(2 * HOUR),
    );

    assert_eq!(candles.len(), 1);
    assert_eq!(candles[0].bucket_start_ns, HOUR);
    assert_eq!(candles[0].open, price(20));
}

#[test]
fn collapses_buyer_and_seller_rows_to_one_trade() {
    let series = SeriesId::from("series".to_owned());

    // One trade emits two rows sharing an event_id; the index keeps the buyer
    // (qty > 0) row, so the candle counts it once with the positive volume.
    seed(vec![
        trade(7, &series, 25, 5, 0),
        trade(7, &series, 25, -5, 0),
    ]);

    let candles = history(&series, PriceHistoryInterval::Hour, None, None);

    assert_eq!(candles.len(), 1);
    assert_eq!(candles[0].trade_count, 1, "one point per trade");
    assert_eq!(candles[0].volume, 5, "positive (trade-size) qty");
}

#[test]
fn ignores_non_executed_events() {
    let series = SeriesId::from("series".to_owned());

    let mut settled = trade(1, &series, 99, 1, HOUR);
    settled.event_type = EventType::Settled;
    let mut liquidated = trade(2, &series, 88, 1, 2 * HOUR);
    liquidated.event_type = EventType::Liquidated;

    seed(vec![trade(0, &series, 10, 1, 0), settled, liquidated]);

    let candles = history(&series, PriceHistoryInterval::Hour, None, None);

    // Only the executed trade produces a candle.
    assert_eq!(candles.len(), 1);
    assert_eq!(candles[0].bucket_start_ns, 0);
    assert_eq!(candles[0].close, price(10));
}

#[test]
fn unknown_or_untraded_series_returns_empty_history() {
    // A series that only ever settled has no executed rows, so no index entry.
    let traded = SeriesId::from("traded".to_owned());
    seed(vec![trade(0, &traded, 10, 1, 0)]);

    assert!(
        history(
            &SeriesId::from("nope".to_owned()),
            PriceHistoryInterval::Hour,
            None,
            None
        )
        .is_empty(),
        "unknown series → empty"
    );

    // A window before any trade also yields nothing — no fabricated points.
    assert!(
        history(&traded, PriceHistoryInterval::Hour, Some(DAY), None).is_empty(),
        "window with no trades → empty"
    );
}

#[test]
fn caps_to_the_most_recent_points() {
    let series = SeriesId::from("series".to_owned());

    // 1_001 trades, one per day, so each lands in its own daily bucket — one
    // more than the MAX_PRICE_HISTORY_POINTS cap of 1_000.
    let events = (0..=1_000_u64)
        .map(|i| trade(i, &series, u128::from(i), 1, i * DAY))
        .collect();
    seed(events);

    let candles = history(&series, PriceHistoryInterval::Day, None, None);

    assert_eq!(candles.len(), 1_000, "capped to the cap");
    // The earliest day (bucket 0) is dropped; the most recent are kept.
    assert_eq!(candles.first().unwrap().bucket_start_ns, DAY);
    assert_eq!(candles.last().unwrap().bucket_start_ns, 1_000 * DAY);
}

#[test]
fn oldest_kept_bucket_at_the_cap_is_fully_aggregated() {
    let series = SeriesId::from("series".to_owned());

    // Day 0 (overflow → dropped) and day 1 (the oldest kept bucket) each carry
    // several trades. The newest-first scan must fold every trade of the oldest
    // kept bucket before the next, strictly-older bucket ends the scan — not cut
    // it off mid-bucket once the cap is reached.
    let mut events = vec![
        trade(0, &series, 99, 1, 0),       // day 0 — dropped
        trade(1, &series, 98, 1, 1),       // day 0 — dropped
        trade(2, &series, 10, 1, DAY),     // day 1 — earliest → open
        trade(3, &series, 20, 1, DAY + 1), // day 1
        trade(4, &series, 30, 1, DAY + 2), // day 1 — latest → close
    ];
    // Fill days 2..=1000 with one trade each, so day 1 is exactly the 1000th
    // (oldest) kept bucket and day 0 is the overflow that stops the scan. Five
    // events precede these, so day `d`'s event_id continues from there.
    for (offset, day) in (2..=1_000_u64).enumerate() {
        let event_id = 5 + offset as u64;
        events.push(trade(event_id, &series, u128::from(day), 1, day * DAY));
    }
    seed(events);

    let candles = history(&series, PriceHistoryInterval::Day, None, None);

    assert_eq!(candles.len(), 1_000);
    let day1 = candles.first().unwrap();
    assert_eq!(
        day1.bucket_start_ns, DAY,
        "day 0 dropped, day 1 is oldest kept"
    );
    assert_eq!(
        day1.trade_count, 3,
        "all of the oldest kept bucket's trades"
    );
    assert_eq!(day1.open, price(10));
    assert_eq!(day1.close, price(30));
    assert_eq!(day1.high, price(30));
    assert_eq!(day1.low, price(10));
}
