use candid::Principal;
use shared::types::{
    BalanceDomain, Description, PayoffType, PayoutUnit, Price, Series, SeriesId, SettlementInput,
};

use crate::{api::trade::tests::utils::*, trade::types::ExecuteTradeParams, types::trade::TradeId};

#[test]
fn binary_lifecycle_scenario() {
    let seller = create_test_user(1);
    let buyer = create_test_user(2);
    let series_id = SeriesId::from("binary_test".to_owned());

    let series = Series {
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
        title: "Binary Test".to_owned(),
        description: Description::plain("Yes/No Market"),
        icon_url: None,
        banner_url: None,
        balance_domain: BalanceDomain::Settlement,
        trading_access: vec![],
    };

    setup_test_state(vec![
        (seller, 200_000), // $20.00
        (buyer, 100_000),  // $10.00
    ]);

    let price = Price::new(30_000_000, 8); // $0.30
    let qty = 10;

    execute_trade_checked(
        &series.clone(),
        ExecuteTradeParams {
            trade_id: TradeId::from("trade_1".to_owned()),
            series_id: series_id.clone(),
            outcome_id: None,
            buyer,
            seller,
            qty: i128::from(qty),
            price,
            buyer_unblock_amount: None,
            seller_unblock_amount: None,
        },
    );

    // Verify State after Trade
    // In the new model, cash balance DOES NOT change on trade (only PnL is realized).
    // The margin is tracked in the reserved_margin field.
    verify_cash_balance(buyer, 100_000); // Original: 100_000, unchanged
    verify_cash_balance(seller, 200_000); // Original: 200_000, unchanged
    verify_position_qty(buyer, &series_id, None, 10);
    verify_position_qty(seller, &series_id, None, -10);

    // 3. Final Settlement: Result is "Yes" (Price = 1.0)
    // 1.0 USD with 8 decimals = 100,000,000
    settle_series_checked(&series, &SettlementInput::Price(Price::new(100_000_000, 8)));

    // Final Balances:
    // Buyer: 100k initial + (100k payoff - 30k margin) = 170k
    // Seller: 200k initial + (0k payoff - 70k margin) = 130k
    verify_cash_balance(buyer, 170_000);
    verify_cash_balance(seller, 130_000);
}

#[test]
fn binary_settle_no() {
    let seller = create_test_user(1);
    let buyer = create_test_user(2);
    let series_id = SeriesId::from("binary_test_no".to_owned());

    let series = Series {
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
        title: "Binary Test NO".to_owned(),
        description: Description::plain("Yes/No Market - Result No"),
        icon_url: None,
        banner_url: None,
        balance_domain: BalanceDomain::Settlement,
        trading_access: vec![],
    };

    setup_test_state(vec![(seller, 200_000), (buyer, 100_000)]);

    let price = Price::new(30_000_000, 8);
    let qty = 10;

    execute_trade_checked(
        &series.clone(),
        ExecuteTradeParams {
            trade_id: TradeId::from("trade_no".to_owned()),
            series_id: series_id.clone(),
            outcome_id: None,
            buyer,
            seller,
            qty: i128::from(qty),
            price,
            buyer_unblock_amount: None,
            seller_unblock_amount: None,
        },
    );

    // 0.0 USD with 8 decimals = 0
    settle_series_checked(&series, &SettlementInput::Price(Price::new(0, 8)));

    // Final Balances:
    // Buyer: 100k initial + (0 payoff - 30k margin) = 70k
    // Seller: 200k initial + (100k payoff - 70k margin) = 230k
    verify_cash_balance(buyer, 70_000);
    verify_cash_balance(seller, 230_000);
}
