use candid::Principal;
use shared::types::{
    BalanceDomain, Description, PayoffType, PayoutUnit, Price, Resolution, Series, SeriesId,
};

use crate::{
    api::trade::{submit_market_order_impl, tests::utils::*},
    memory::{ACCOUNT_STATES, LIMIT_ORDERS},
    payoffs::get_required_margin,
    types::{
        errors::CommonError,
        trade::{LimitOrder, OrderId, Side, TradeId},
        user::User,
    },
    TradeError,
};

/// $0.30 in the series' 8-decimal price precision. With 4 USD decimals this puts the
/// per-unit Binary margin at `3_000` (long) / `7_000` (short).
fn order_price() -> Price {
    Price::new(30_000_000, 8)
}

fn binary_series(series_id: &SeriesId) -> Series {
    Series {
        resolution: Resolution::new("Resolved per oracle at expiry"),
        series_id: series_id.clone(),
        underlying: "BITCOIN_UP_50K".to_owned(),
        expiry_ns: 2_000_000_000,
        payoff_type: PayoffType::Binary,
        strike: Some(Price::new(50_000, 0)),
        price_precision: 8,
        payout_unit: PayoutUnit::usd(),
        outcomes: None,
        oracle_source: "oracle".to_owned(),
        creator: Principal::anonymous(),
        created_at_ns: 1_000_000_000,
        title: "Binary Partial Fill Test".to_owned(),
        description: Description::plain("Yes/No Market"),
        icon_url: None,
        banner_url: None,
        balance_domain: BalanceDomain::Settlement,
        trading_access: vec![],
        engine_id: None,
        forked_from: None,
        locale: None,
    }
}

/// Inserts a resting limit order for `maker` and reserves its blocked margin in the
/// maker's account, mirroring what `submit_limit_order` does. Returns the blocked
/// margin so tests can assert against the placement-time formula.
fn place_resting_order(
    series: &Series,
    order_id: &OrderId,
    maker: User,
    side: Side,
    qty: i128,
) -> u128 {
    let margin_qty = if side == Side::Sell { -qty } else { qty };
    let blocked_margin_usd = get_required_margin(series, &order_price(), margin_qty, &None)
        .expect("margin calculation failed");

    LIMIT_ORDERS.with(|m| {
        let mut m = m.borrow_mut();
        m.clear();
        m.insert(
            order_id.clone(),
            LimitOrder {
                order_id: order_id.clone(),
                creator: maker,
                series_id: series.series_id.clone(),
                outcome_id: None,
                side,
                qty,
                price: order_price(),
                blocked_margin_usd,
                balance_domain: BalanceDomain::Settlement,
            },
        );
    });

    ACCOUNT_STATES.with(|acc| {
        let mut acc = acc.borrow_mut();
        acc.get_mut(&maker)
            .expect("Maker account not found")
            .set_reserved_margin_usd(BalanceDomain::Settlement, blocked_margin_usd);
    });

    blocked_margin_usd
}

fn get_order(order_id: &OrderId) -> Option<LimitOrder> {
    LIMIT_ORDERS.with(|m| m.borrow().get(order_id).cloned())
}

#[test]
fn partial_fill_leaves_reduced_resting_order() {
    let maker = create_test_user(1);
    let taker = create_test_user(2);
    let series_id = SeriesId::from("partial_fill_half".to_owned());
    let series = binary_series(&series_id);
    let order_id = OrderId::from("order_1".to_owned());

    setup_test_state(vec![(maker, 1_000_000), (taker, 1_000_000)]);
    let blocked = place_resting_order(&series, &order_id, maker, Side::Buy, 10);
    assert_eq!(blocked, 30_000);

    let order = get_order(&order_id).unwrap();
    let result = submit_market_order_impl(
        taker,
        &order,
        &TradeId::from("trade_1".to_owned()),
        &series,
        Some(5),
    );
    assert!(result.is_ok(), "partial fill failed: {result:?}");

    // The resting order remains with the unfilled quantity and a blocked margin
    // recomputed with the placement-time formula on the remaining quantity.
    let remaining = get_order(&order_id).expect("Order should remain on the book");
    assert_eq!(remaining.qty, 5);
    assert_eq!(remaining.blocked_margin_usd, 15_000);

    verify_position_qty(maker, &series_id, None, 5);
    verify_position_qty(taker, &series_id, None, -5);

    // Maker: 30_000 reserved before the fill, + 15_000 position margin for the 5 filled
    // units, - 15_000 unblocked (= 30_000 old order block - 15_000 remaining block).
    verify_reserved_margin(maker, 30_000);
    // Taker margin reflects only the filled quantity: 5 short units at $0.30 = 5 * 7_000.
    verify_reserved_margin(taker, 35_000);
}

#[test]
fn none_qty_fills_entire_order() {
    let maker = create_test_user(1);
    let taker = create_test_user(2);
    let series_id = SeriesId::from("partial_fill_none".to_owned());
    let series = binary_series(&series_id);
    let order_id = OrderId::from("order_1".to_owned());

    setup_test_state(vec![(maker, 1_000_000), (taker, 1_000_000)]);
    place_resting_order(&series, &order_id, maker, Side::Buy, 10);

    let order = get_order(&order_id).unwrap();
    let result = submit_market_order_impl(
        taker,
        &order,
        &TradeId::from("trade_1".to_owned()),
        &series,
        None,
    );
    assert!(result.is_ok(), "full fill failed: {result:?}");

    assert!(get_order(&order_id).is_none(), "Order should be removed");
    verify_position_qty(maker, &series_id, None, 10);
    verify_position_qty(taker, &series_id, None, -10);
    // The full 30_000 order block is released and replaced by the 30_000 position margin.
    verify_reserved_margin(maker, 30_000);
    verify_reserved_margin(taker, 70_000);
}

#[test]
fn oversized_qty_fills_entire_order() {
    let maker = create_test_user(1);
    let taker = create_test_user(2);
    let series_id = SeriesId::from("partial_fill_oversized".to_owned());
    let series = binary_series(&series_id);
    let order_id = OrderId::from("order_1".to_owned());

    setup_test_state(vec![(maker, 1_000_000), (taker, 1_000_000)]);
    place_resting_order(&series, &order_id, maker, Side::Buy, 10);

    let order = get_order(&order_id).unwrap();
    let result = submit_market_order_impl(
        taker,
        &order,
        &TradeId::from("trade_1".to_owned()),
        &series,
        Some(25),
    );
    assert!(result.is_ok(), "oversized fill failed: {result:?}");

    // The fill is capped at the resting quantity and the order is removed.
    assert!(get_order(&order_id).is_none(), "Order should be removed");
    verify_position_qty(maker, &series_id, None, 10);
    verify_position_qty(taker, &series_id, None, -10);
    verify_reserved_margin(maker, 30_000);
    verify_reserved_margin(taker, 70_000);
}

#[test]
fn non_positive_qty_is_rejected() {
    let maker = create_test_user(1);
    let taker = create_test_user(2);
    let series_id = SeriesId::from("partial_fill_zero".to_owned());
    let series = binary_series(&series_id);
    let order_id = OrderId::from("order_1".to_owned());

    setup_test_state(vec![(maker, 1_000_000), (taker, 1_000_000)]);
    place_resting_order(&series, &order_id, maker, Side::Buy, 10);

    for (trade, qty) in [("trade_zero", 0), ("trade_negative", -3)] {
        let order = get_order(&order_id).unwrap();
        let result = submit_market_order_impl(
            taker,
            &order,
            &TradeId::from(trade.to_owned()),
            &series,
            Some(qty),
        );
        assert!(
            matches!(
                &result,
                Err(TradeError::Common(CommonError::InvalidInput(_)))
            ),
            "expected InvalidInput for qty {qty}, got: {result:?}"
        );
    }

    // The book and the maker's reservation are untouched.
    let untouched = get_order(&order_id).unwrap();
    assert_eq!(untouched.qty, 10);
    assert_eq!(untouched.blocked_margin_usd, 30_000);
    verify_position_qty(maker, &series_id, None, 0);
    verify_position_qty(taker, &series_id, None, 0);
    verify_reserved_margin(maker, 30_000);
}

#[test]
fn second_taker_fills_remainder() {
    let maker = create_test_user(1);
    let taker_a = create_test_user(2);
    let taker_b = create_test_user(3);
    let series_id = SeriesId::from("partial_fill_remainder".to_owned());
    let series = binary_series(&series_id);
    let order_id = OrderId::from("order_1".to_owned());

    setup_test_state(vec![
        (maker, 1_000_000),
        (taker_a, 1_000_000),
        (taker_b, 1_000_000),
    ]);
    place_resting_order(&series, &order_id, maker, Side::Buy, 10);

    let order = get_order(&order_id).unwrap();
    submit_market_order_impl(
        taker_a,
        &order,
        &TradeId::from("trade_a".to_owned()),
        &series,
        Some(4),
    )
    .expect("first partial fill failed");

    let remaining = get_order(&order_id).expect("Order should remain after first fill");
    assert_eq!(remaining.qty, 6);
    assert_eq!(remaining.blocked_margin_usd, 18_000);

    submit_market_order_impl(
        taker_b,
        &remaining,
        &TradeId::from("trade_b".to_owned()),
        &series,
        None,
    )
    .expect("remainder fill failed");

    assert!(get_order(&order_id).is_none(), "Order should be removed");
    verify_position_qty(maker, &series_id, None, 10);
    verify_position_qty(taker_a, &series_id, None, -4);
    verify_position_qty(taker_b, &series_id, None, -6);
    verify_reserved_margin(maker, 30_000);
    verify_reserved_margin(taker_a, 28_000);
    verify_reserved_margin(taker_b, 42_000);
}

#[test]
fn partial_fill_sell_side_maker() {
    let maker = create_test_user(1);
    let taker = create_test_user(2);
    let series_id = SeriesId::from("partial_fill_sell".to_owned());
    let series = binary_series(&series_id);
    let order_id = OrderId::from("order_1".to_owned());

    setup_test_state(vec![(maker, 1_000_000), (taker, 1_000_000)]);
    // Short Binary margin at $0.30 is 7_000 per unit, so 10 units block 70_000.
    let blocked = place_resting_order(&series, &order_id, maker, Side::Sell, 10);
    assert_eq!(blocked, 70_000);

    let order = get_order(&order_id).unwrap();
    let result = submit_market_order_impl(
        taker,
        &order,
        &TradeId::from("trade_1".to_owned()),
        &series,
        Some(5),
    );
    assert!(result.is_ok(), "partial sell-side fill failed: {result:?}");

    let remaining = get_order(&order_id).expect("Order should remain on the book");
    assert_eq!(remaining.qty, 5);
    assert_eq!(remaining.blocked_margin_usd, 35_000);

    verify_position_qty(maker, &series_id, None, -5);
    verify_position_qty(taker, &series_id, None, 5);
    // Maker: 70_000 reserved + 35_000 short-position margin - 35_000 unblocked.
    verify_reserved_margin(maker, 70_000);
    // Taker long 5 units at $0.30 = 15_000.
    verify_reserved_margin(taker, 15_000);
}
