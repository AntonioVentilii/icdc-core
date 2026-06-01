use candid::Principal;
use shared::types::{BalanceDomain, Description, PayoffType, PayoutUnit, Price, Series, SeriesId};

use crate::{
    api::trade::{
        api::list_series_trade_history,
        params::ListSeriesTradeHistoryParams,
        tests::utils::{create_test_user, execute_trade_checked, setup_test_state},
    },
    memory::{rebuild_series_trade_history, EVENTS, SERIES_TRADE_HISTORY},
    trade::types::ExecuteTradeParams,
    types::{
        event::{Event, EventType},
        trade::TradeId,
        user::User,
    },
};

/// Builds an executed buyer-side [`Event`] (the one the index keeps). `qty` is
/// positive so the rebuild selects it; `clearing_id`/`user` are immaterial.
fn executed(event_id: u64, series_id: &SeriesId, timestamp: u64) -> Event {
    event(event_id, series_id, EventType::Executed, 1, timestamp)
}

fn event(
    event_id: u64,
    series_id: &SeriesId,
    event_type: EventType,
    qty: i128,
    timestamp: u64,
) -> Event {
    Event {
        event_id,
        clearing_id: Principal::anonymous(),
        series_id: series_id.clone(),
        user: User(Principal::anonymous()),
        qty,
        price: Price::new(u128::from(event_id), 0),
        event_type,
        timestamp,
    }
}

/// Seeds `EVENTS` and rebuilds the per-series price-history index from it, the
/// same projection `post_upgrade` performs.
fn seed(events: Vec<Event>) {
    EVENTS.with(|e| {
        let mut e = e.borrow_mut();
        e.clear();
        *e = events;
    });
    rebuild_series_trade_history();
}

fn query(series_id: &SeriesId, start_after: Option<u64>, limit: Option<u64>) -> Vec<u64> {
    list_series_trade_history(ListSeriesTradeHistoryParams {
        series_id: series_id.clone(),
        start_after,
        limit,
    })
    .items
    .iter()
    .map(|p| p.event_id)
    .collect()
}

#[test]
fn filters_by_series_and_executed_only() {
    let target = SeriesId::from("series_a".to_owned());
    let other = SeriesId::from("series_b".to_owned());

    seed(vec![
        executed(0, &target, 100),
        executed(1, &other, 110),
        event(2, &target, EventType::Settled, 1, 120),
        event(3, &target, EventType::Liquidated, 1, 130),
        executed(4, &target, 140),
    ]);

    let page = list_series_trade_history(ListSeriesTradeHistoryParams {
        series_id: target.clone(),
        start_after: None,
        limit: None,
    });

    let ids: Vec<u64> = page.items.iter().map(|p| p.event_id).collect();
    assert_eq!(ids, vec![0, 4], "only executed trades for series_a");
    assert!(page.next_cursor.is_none());
}

#[test]
fn collapses_buyer_and_seller_rows_to_one_point() {
    let series = SeriesId::from("series".to_owned());

    // One trade emits two rows sharing an event_id: buyer (qty > 0) and seller
    // (qty < 0). The price history keeps a single point per trade.
    seed(vec![
        event(7, &series, EventType::Executed, 5, 100),
        event(7, &series, EventType::Executed, -5, 100),
    ]);

    let page = list_series_trade_history(ListSeriesTradeHistoryParams {
        series_id: series.clone(),
        start_after: None,
        limit: None,
    });

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].event_id, 7);
    assert_eq!(page.items[0].qty, 5, "keeps the positive (trade-size) qty");
    assert!(page.next_cursor.is_none());
}

#[test]
fn orders_by_event_id() {
    let series = SeriesId::from("series".to_owned());

    // Seed in a jumbled order; the index is sorted by event_id (execution order).
    seed(vec![
        executed(5, &series, 100),
        executed(2, &series, 90),
        executed(9, &series, 110),
    ]);

    assert_eq!(query(&series, None, None), vec![2, 5, 9]);
}

#[test]
fn paginates_with_exclusive_cursor() {
    let series = SeriesId::from("series".to_owned());

    seed(vec![
        executed(0, &series, 100),
        executed(1, &series, 200),
        executed(2, &series, 300),
        executed(3, &series, 400),
        executed(4, &series, 500),
    ]);

    let page1 = list_series_trade_history(ListSeriesTradeHistoryParams {
        series_id: series.clone(),
        start_after: None,
        limit: Some(2),
    });
    assert_eq!(
        page1.items.iter().map(|p| p.event_id).collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(page1.next_cursor, Some(1));

    assert_eq!(query(&series, Some(1), Some(2)), vec![2, 3]);

    let page3 = list_series_trade_history(ListSeriesTradeHistoryParams {
        series_id: series.clone(),
        start_after: Some(3),
        limit: Some(2),
    });
    assert_eq!(
        page3.items.iter().map(|p| p.event_id).collect::<Vec<_>>(),
        vec![4]
    );
    assert!(page3.next_cursor.is_none());
}

#[test]
fn full_page_at_exact_boundary_has_no_cursor() {
    let series = SeriesId::from("series".to_owned());

    seed(vec![executed(0, &series, 100), executed(1, &series, 200)]);

    let page = list_series_trade_history(ListSeriesTradeHistoryParams {
        series_id: series.clone(),
        start_after: None,
        limit: Some(2),
    });
    assert_eq!(page.items.len(), 2);
    assert!(page.next_cursor.is_none());
}

#[test]
fn zero_limit_still_makes_forward_progress() {
    let series = SeriesId::from("series".to_owned());

    seed(vec![executed(0, &series, 100), executed(1, &series, 200)]);

    // A limit of 0 is clamped to 1 so the caller is never stranded.
    let page = list_series_trade_history(ListSeriesTradeHistoryParams {
        series_id: series.clone(),
        start_after: None,
        limit: Some(0),
    });
    assert_eq!(
        page.items.iter().map(|p| p.event_id).collect::<Vec<_>>(),
        vec![0]
    );
    assert_eq!(page.next_cursor, Some(0));
}

#[test]
fn unknown_series_returns_empty_page() {
    seed(vec![executed(0, &SeriesId::from("series".to_owned()), 100)]);

    let page = list_series_trade_history(ListSeriesTradeHistoryParams {
        series_id: SeriesId::from("nope".to_owned()),
        start_after: None,
        limit: None,
    });
    assert!(page.items.is_empty());
    assert!(page.next_cursor.is_none());
}

/// Executing a real trade through the service path must populate the index
/// incrementally (one point per trade), not just the `post_upgrade` rebuild.
#[test]
fn live_execution_populates_index() {
    let seller = create_test_user(1);
    let buyer = create_test_user(2);
    let series_id = SeriesId::from("live_index_test".to_owned());

    let series = Series {
        series_id: series_id.clone(),
        underlying: "BTC".to_owned(),
        expiry_ns: 2_000_000_000,
        payoff_type: PayoffType::Binary,
        strike: Some(Price::new(50_000, 0)),
        price_precision: 8,
        payout_unit: PayoutUnit::usd(),
        outcomes: None,
        oracle_source: "oracle".to_owned(),
        creator: Principal::anonymous(),
        created_at_ns: 1_000_000_000,
        title: "Live Index Test".to_owned(),
        description: Description::plain("Yes/No Market"),
        icon_url: None,
        banner_url: None,
        balance_domain: BalanceDomain::Settlement,
        trading_access: vec![],
        engine_id: None,
        forked_from: None,
        locale: None,
    };

    // Start from a clean index so we only observe this test's trade.
    SERIES_TRADE_HISTORY.with(|idx| idx.borrow_mut().clear());
    EVENTS.with(|e| e.borrow_mut().clear());
    setup_test_state(vec![(seller, 200_000), (buyer, 100_000)]);

    let price = Price::new(30_000_000, 8); // $0.30
    execute_trade_checked(
        &series,
        ExecuteTradeParams {
            trade_id: TradeId::from("live_trade".to_owned()),
            series_id: series_id.clone(),
            outcome_id: None,
            buyer,
            seller,
            qty: 10,
            price: price.clone(),
            buyer_unblock_amount: None,
            seller_unblock_amount: None,
        },
    );

    let page = list_series_trade_history(ListSeriesTradeHistoryParams {
        series_id: series_id.clone(),
        start_after: None,
        limit: None,
    });

    assert_eq!(page.items.len(), 1, "one point per executed trade");
    assert_eq!(page.items[0].qty, 10);
    assert_eq!(page.items[0].price, price);
    assert!(page.next_cursor.is_none());
}
