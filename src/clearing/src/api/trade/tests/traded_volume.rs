use candid::Principal;
use shared::types::{Price, SeriesId};

use crate::{
    api::trade::{
        api::list_series_traded_volumes, params::ListSeriesTradedVolumesParams,
        results::SeriesTradedVolume,
    },
    memory::{index_executed_trade, rebuild_series_trade_history, EVENTS},
    types::{
        event::{Event, EventType, SeriesTradePoint},
        user::User,
    },
};

/// An executed trade row. `qty > 0` is the buyer side the index keeps; the
/// price carries its own precision so notional scaling can be exercised.
fn trade(event_id: u64, series_id: &SeriesId, value: u128, decimals: u8, qty: i128) -> Event {
    Event {
        event_id,
        clearing_id: Principal::anonymous(),
        series_id: series_id.clone(),
        user: User(Principal::anonymous()),
        qty,
        price: Price::new(value, decimals),
        event_type: EventType::Executed,
        timestamp: event_id,
    }
}

/// Seeds `EVENTS` and rebuilds the per-series trade index from it — the same
/// projection `post_upgrade` performs.
fn seed(events: Vec<Event>) {
    EVENTS.with(|e| {
        let mut e = e.borrow_mut();
        e.clear();
        *e = events;
    });
    rebuild_series_trade_history();
}

fn volumes(series_ids: &[&SeriesId]) -> Vec<SeriesTradedVolume> {
    list_series_traded_volumes(ListSeriesTradedVolumesParams {
        series_ids: series_ids.iter().map(|s| (*s).clone()).collect(),
    })
}

#[test]
fn sums_notional_across_trades() {
    let series = SeriesId::from("series".to_owned());

    // Prices in USD_DECIMALS (4): 0.5000 and 0.8000. With matching precision
    // the notional is exactly `qty · value`: 20·5000 + 10·8000 = 180_000
    // (i.e. 10 + 8 = 18 USD in base units).
    seed(vec![
        trade(0, &series, 5_000, 4, 20),
        trade(1, &series, 8_000, 4, 10),
    ]);

    let result = volumes(&[&series]);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].series_id, series);
    assert_eq!(result[0].volume, 180_000);
    assert_eq!(result[0].trade_count, 2);
}

#[test]
fn scales_notional_to_usd_base_units() {
    let series = SeriesId::from("series".to_owned());

    // Price 10 at decimals 0 (a whole unit), qty 3 → 3 · 10 = 30, scaled up to
    // USD base units (×10^4) = 300_000.
    seed(vec![trade(0, &series, 10, 0, 3)]);

    let result = volumes(&[&series]);

    assert_eq!(result[0].volume, 300_000);
    assert_eq!(result[0].trade_count, 1);
}

#[test]
fn unknown_or_untraded_series_totals_zero() {
    let traded = SeriesId::from("traded".to_owned());
    let empty = SeriesId::from("empty".to_owned());

    seed(vec![trade(0, &traded, 5_000, 4, 4)]);

    let result = volumes(&[&empty]);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].series_id, empty);
    assert_eq!(result[0].volume, 0);
    assert_eq!(result[0].trade_count, 0);
}

#[test]
fn returns_one_entry_per_requested_id_in_order() {
    let a = SeriesId::from("a".to_owned());
    let missing = SeriesId::from("missing".to_owned());
    let b = SeriesId::from("b".to_owned());

    seed(vec![
        trade(0, &a, 5_000, 4, 2), // a: 2·5000 = 10_000
        trade(1, &b, 5_000, 4, 6), // b: 6·5000 = 30_000
    ]);

    let result = volumes(&[&a, &missing, &b]);

    assert_eq!(result.len(), 3);
    assert_eq!(result[0].series_id, a);
    assert_eq!(result[0].volume, 10_000);
    assert_eq!(result[1].series_id, missing);
    assert_eq!(result[1].volume, 0);
    assert_eq!(result[2].series_id, b);
    assert_eq!(result[2].volume, 30_000);
}

#[test]
fn incrementally_indexed_trades_accumulate() {
    let series = SeriesId::from("series".to_owned());

    // Start from a clean projection, then fold trades in via the live execution
    // path (`index_executed_trade`) rather than a rebuild — the aggregate the
    // query reads must match either route.
    seed(vec![]);

    index_executed_trade(
        &series,
        SeriesTradePoint {
            event_id: 0,
            price: Price::new(5_000, 4),
            qty: 20,
            timestamp: 0,
        },
    );
    index_executed_trade(
        &series,
        SeriesTradePoint {
            event_id: 1,
            price: Price::new(8_000, 4),
            qty: 10,
            timestamp: 1,
        },
    );

    let result = volumes(&[&series]);

    // 20·5000 + 10·8000, exact since the series prices in USD_DECIMALS.
    assert_eq!(result[0].volume, 180_000);
    assert_eq!(result[0].trade_count, 2);
}
