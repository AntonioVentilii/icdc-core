use candid::Principal;
use shared::types::{Price, SeriesId};

use crate::{
    api::trade::{
        api::list_series_trade_history,
        params::ListSeriesTradeHistoryParams,
        results::{SeriesTradeHistoryPage, TradeHistoryCursor},
    },
    memory::EVENTS,
    types::{
        event::{Event, EventType},
        user::User,
    },
};

/// Builds an [`Event`] with the fields the series-scoped trade-history query
/// reads. `clearing_id`/`user`/`qty` are immaterial to the query, so they get
/// placeholder values.
fn event(event_id: u64, series_id: &SeriesId, event_type: EventType, timestamp: u64) -> Event {
    Event {
        event_id,
        clearing_id: Principal::anonymous(),
        series_id: series_id.clone(),
        user: User(Principal::anonymous()),
        qty: 1,
        price: Price::new(u128::from(event_id), 0),
        event_type,
        timestamp,
    }
}

fn seed(events: Vec<Event>) {
    EVENTS.with(|e| {
        let mut e = e.borrow_mut();
        e.clear();
        *e = events;
    });
}

fn query(
    series_id: &SeriesId,
    start_after: Option<TradeHistoryCursor>,
    limit: Option<u64>,
) -> SeriesTradeHistoryPage {
    list_series_trade_history(ListSeriesTradeHistoryParams {
        series_id: series_id.clone(),
        start_after,
        limit,
    })
}

#[test]
fn filters_by_series_and_executed_only() {
    let target = SeriesId::from("series_a".to_owned());
    let other = SeriesId::from("series_b".to_owned());

    seed(vec![
        event(0, &target, EventType::Executed, 100),
        event(1, &other, EventType::Executed, 110),
        event(2, &target, EventType::Settled, 120),
        event(3, &target, EventType::Liquidated, 130),
        event(4, &target, EventType::Executed, 140),
    ]);

    let page = query(&target, None, None);

    let ids: Vec<u64> = page.items.iter().map(|e| e.event_id).collect();
    assert_eq!(ids, vec![0, 4], "only executed events for series_a");
    assert!(page.next_cursor.is_none());
}

#[test]
fn orders_by_timestamp_then_event_id() {
    let series = SeriesId::from("series".to_owned());

    // event_id order deliberately disagrees with timestamp order (e.g. a
    // backfilled row with a newer id but older timestamp).
    seed(vec![
        event(5, &series, EventType::Executed, 100),
        event(2, &series, EventType::Executed, 100),
        event(9, &series, EventType::Executed, 90),
    ]);

    let page = query(&series, None, None);

    let ids: Vec<u64> = page.items.iter().map(|e| e.event_id).collect();
    // (90,9) < (100,2) < (100,5)
    assert_eq!(ids, vec![9, 2, 5]);
}

#[test]
fn paginates_with_exclusive_cursor() {
    let series = SeriesId::from("series".to_owned());

    seed(vec![
        event(0, &series, EventType::Executed, 100),
        event(1, &series, EventType::Executed, 200),
        event(2, &series, EventType::Executed, 300),
        event(3, &series, EventType::Executed, 400),
        event(4, &series, EventType::Executed, 500),
    ]);

    // First page of 2.
    let page1 = query(&series, None, Some(2));
    assert_eq!(
        page1.items.iter().map(|e| e.event_id).collect::<Vec<_>>(),
        vec![0, 1]
    );
    let cursor1 = page1.next_cursor.expect("more pages remain");
    assert_eq!(cursor1.event_id, 1);

    // Second page of 2, resuming after the cursor.
    let page2 = query(&series, Some(cursor1), Some(2));
    assert_eq!(
        page2.items.iter().map(|e| e.event_id).collect::<Vec<_>>(),
        vec![2, 3]
    );
    let cursor2 = page2.next_cursor.expect("one more event remains");

    // Final page: exactly the last event, no further cursor.
    let page3 = query(&series, Some(cursor2), Some(2));
    assert_eq!(
        page3.items.iter().map(|e| e.event_id).collect::<Vec<_>>(),
        vec![4]
    );
    assert!(page3.next_cursor.is_none());
}

#[test]
fn full_page_at_exact_boundary_has_no_cursor() {
    let series = SeriesId::from("series".to_owned());

    seed(vec![
        event(0, &series, EventType::Executed, 100),
        event(1, &series, EventType::Executed, 200),
    ]);

    // limit equals the number of remaining events: no cursor should be emitted.
    let page = query(&series, None, Some(2));
    assert_eq!(page.items.len(), 2);
    assert!(page.next_cursor.is_none());
}

#[test]
fn zero_limit_still_makes_forward_progress() {
    let series = SeriesId::from("series".to_owned());

    seed(vec![
        event(0, &series, EventType::Executed, 100),
        event(1, &series, EventType::Executed, 200),
    ]);

    // A limit of 0 is clamped to 1 so the caller is never stranded.
    let page = query(&series, None, Some(0));
    assert_eq!(
        page.items.iter().map(|e| e.event_id).collect::<Vec<_>>(),
        vec![0]
    );
    let cursor = page
        .next_cursor
        .expect("a cursor must advance past the single returned event");
    assert_eq!(cursor.event_id, 0);
}

#[test]
fn unknown_series_returns_empty_page() {
    seed(vec![event(
        0,
        &SeriesId::from("series".to_owned()),
        EventType::Executed,
        100,
    )]);

    let page = query(&SeriesId::from("nope".to_owned()), None, None);
    assert!(page.items.is_empty());
    assert!(page.next_cursor.is_none());
}
