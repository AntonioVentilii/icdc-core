use candid::Principal;
use shared::types::{
    BalanceDomain, Description, Outcome, OutcomeId, PayoffType, PayoutUnit, Price, Resolution,
    Series, SeriesId,
};

use crate::{
    api::trade::{api::validate_no_arbitrage, errors::TradeError, params::SubmitLimitOrderParams},
    memory::LIMIT_ORDERS,
    types::{
        trade::{LimitOrder, OrderId, Side},
        user::User,
    },
};

#[test]
fn categorical_arbitrage_validation() {
    let series_id = SeriesId::from("arb_test".to_owned());
    let outcome_yes = OutcomeId::from("Yes".to_owned());
    let outcome_no = OutcomeId::from("No".to_owned());

    let series = Series {
        resolution: Resolution::new("Resolved per oracle at expiry"),
        series_id: series_id.clone(),
        underlying: "ARB_TEST".to_owned(),
        expiry_ns: 2_000_000_000,
        start_ns: None,
        payoff_type: PayoffType::Categorical,
        strike: None,
        price_precision: 6,
        payout_unit: PayoutUnit::usd(),
        outcomes: Some(vec![
            Outcome {
                id: outcome_yes.clone(),
                title: "Yes".to_owned(),
                description: None,
                icon_url: None,
            },
            Outcome {
                id: outcome_no.clone(),
                title: "No".to_owned(),
                description: None,
                icon_url: None,
            },
        ]),
        oracle_source: "oracle".to_owned(),
        creator: Principal::anonymous(),
        created_at_ns: 1_000_000_000,
        title: "Arbitrage Test".to_owned(),
        description: Description::plain("Test for arbitrage"),
        icon_url: None,
        banner_url: None,
        balance_domain: BalanceDomain::Settlement,
        trading_access: vec![],
        engine_id: None,
        forked_from: None,
        locale: None,
    };

    LIMIT_ORDERS.with(|m| m.borrow_mut().clear());

    // 1. Submit "Yes" order at $0.60 (Allowed)
    let params_yes = SubmitLimitOrderParams {
        order_id: OrderId::from("order_yes".to_owned()),
        series_id: series_id.clone(),
        outcome_id: Some(outcome_yes.clone()),
        side: Side::Buy,
        qty: 10,
        price: Price::new(600_000, 6),
    };

    assert!(validate_no_arbitrage(&series, &params_yes).is_ok());

    // Add it to memory
    LIMIT_ORDERS.with(|m| {
        m.borrow_mut().insert(
            params_yes.order_id.clone(),
            LimitOrder {
                order_id: params_yes.order_id,
                creator: User(Principal::anonymous()),
                series_id: series_id.clone(),
                outcome_id: Some(outcome_yes.clone()),
                side: Side::Buy,
                qty: 10,
                price: params_yes.price,
                blocked_margin_usd: 60_000,
                balance_domain: BalanceDomain::Settlement,
            },
        );
    });

    // 2. Submit "No" order at $0.30 (Allowed, Sum = 0.90)
    let params_no_low = SubmitLimitOrderParams {
        order_id: OrderId::from("order_no_low".to_owned()),
        series_id: series_id.clone(),
        outcome_id: Some(outcome_no.clone()),
        side: Side::Buy,
        qty: 10,
        price: Price::new(300_000, 6),
    };
    assert!(validate_no_arbitrage(&series, &params_no_low).is_ok());

    // 3. Submit "No" order at $0.50 (Violation, Sum = 0.60 + 0.50 = 1.10)
    let params_no_high = SubmitLimitOrderParams {
        order_id: OrderId::from("order_no_high".to_owned()),
        series_id: series_id.clone(),
        outcome_id: Some(outcome_no.clone()),
        side: Side::Buy,
        qty: 10,
        price: Price::new(500_000, 6),
    };
    let result = validate_no_arbitrage(&series, &params_no_high);
    assert!(result.is_err());
    if let Err(TradeError::ArbitrageLimitExceeded { sum_usd, limit_usd }) = result {
        assert_eq!(sum_usd, 11_000);
        assert_eq!(limit_usd, 10_000);
    } else {
        panic!("Expected ArbitrageLimitExceeded error");
    }
}

#[test]
fn binary_arbitrage_validation() {
    let series_id = SeriesId::from("binary_arb".to_owned());
    let series = Series {
        resolution: Resolution::new("Resolved per oracle at expiry"),
        series_id: series_id.clone(),
        underlying: "BTC".to_owned(),
        expiry_ns: 2_000_000_000,
        start_ns: None,
        payoff_type: PayoffType::Binary,
        strike: Some(Price::new(50_000, 0)),
        price_precision: 6,
        payout_unit: PayoutUnit::usd(),
        outcomes: None,
        oracle_source: "oracle".to_owned(),
        creator: Principal::anonymous(),
        created_at_ns: 1_000_000_000,
        title: "Binary Arb".to_owned(),
        description: Description::plain("Binary arb test"),
        icon_url: None,
        banner_url: None,
        balance_domain: BalanceDomain::Settlement,
        trading_access: vec![],
        engine_id: None,
        forked_from: None,
        locale: None,
    };

    // Price $1.10 (Violation)
    let params = SubmitLimitOrderParams {
        order_id: OrderId::from("order1".to_owned()),
        series_id: series_id.clone(),
        outcome_id: None,
        side: Side::Buy,
        qty: 10,
        price: Price::new(1_100_000, 6),
    };
    let result = validate_no_arbitrage(&series, &params);
    assert!(result.is_err());
}
