use ::shared::types::{
    Description, PayoffType, PayoutUnit, Price, Series, SeriesId, SettlementInput,
};
use candid::Principal;

use crate::{api::trade::tests::utils::*, trade::types::ExecuteTradeParams, types::trade::TradeId};

#[test]
fn test_binary_lifecycle_scenario() {
    let seller = create_test_user(1);
    let buyer = create_test_user(2);
    let series_id = SeriesId::from("binary_test".to_string());

    let series = Series {
        series_id: series_id.clone(),
        underlying: "BITCOIN_UP_50K".to_string(),
        expiry_ns: 2000000000,
        payoff_type: PayoffType::Binary,
        strike: Some(Price::new(50_000, 0)),
        price_precision: 8,
        payout_unit: PayoutUnit::usd(),
        outcomes: None,
        oracle_source: "oracle".to_string(),
        creator: Principal::anonymous(),
        created_at_ns: 1000000000,
        title: "Binary Test".to_string(),
        description: Description::plain("Yes/No Market"),
        icon_url: None,
        banner_url: None,
        balance_domain: ::shared::types::BalanceDomain::Settlement,
    };

    setup_test_state(vec![
        (seller, 20_000_000), // $20.00
        (buyer, 10_000_000),  // $10.00
    ]);

    let price = Price::new(30_000_000, 8); // $0.30
    let qty = 10;

    execute_trade_checked(
        series.clone(),
        ExecuteTradeParams {
            trade_id: TradeId::from("trade_1".to_string()),
            series_id: series_id.clone(),
            outcome_id: None,
            buyer,
            seller,
            qty: qty as i128,
            price,
            buyer_unblock_amount: None,
            seller_unblock_amount: None,
        },
    );

    // Verify State after Trade
    // Long (Buyer) cost: 10 * 0.30 = $3.00
    // Short (Seller) cost: 10 * (1.00 - 0.30) = $7.00
    verify_cash_balance(buyer, 7_000_000); // 10 - 3 = 7
    verify_cash_balance(seller, 13_000_000); // 20 - 7 = 13
    verify_position_qty(buyer, &series_id, None, 10);
    verify_position_qty(seller, &series_id, None, -10);

    // 3. Final Settlement: Result is "Yes" (Price = 1.0)
    // 1.0 USD with 8 decimals = 100,000,000
    settle_series_checked(&series, SettlementInput::Price(Price::new(100_000_000, 8)));

    // Final Balances:
    // Buyer: $7 (cash) + 10 * $1.00 (payoff) = $17.00 = 17,000,000
    // Seller: $13 (cash) + 10 * $0 (payoff) = $13.00 = 13,000,000
    verify_cash_balance(buyer, 17_000_000);
    verify_cash_balance(seller, 13_000_000);
}

#[test]
fn test_binary_settle_no() {
    let seller = create_test_user(1);
    let buyer = create_test_user(2);
    let series_id = SeriesId::from("binary_test_no".to_string());

    let series = Series {
        series_id: series_id.clone(),
        underlying: "BITCOIN_UP_50K".to_string(),
        expiry_ns: 2000000000,
        payoff_type: PayoffType::Binary,
        strike: Some(Price::new(50_000, 0)),
        price_precision: 8,
        payout_unit: PayoutUnit::usd(),
        outcomes: None,
        oracle_source: "oracle".to_string(),
        creator: Principal::anonymous(),
        created_at_ns: 1000000000,
        title: "Binary Test NO".to_string(),
        description: Description::plain("Yes/No Market - Result No"),
        icon_url: None,
        banner_url: None,
        balance_domain: ::shared::types::BalanceDomain::Settlement,
    };

    setup_test_state(vec![(seller, 20_000_000), (buyer, 10_000_000)]);

    let price = Price::new(30_000_000, 8);
    let qty = 10;

    execute_trade_checked(
        series.clone(),
        ExecuteTradeParams {
            trade_id: TradeId::from("trade_no".to_string()),
            series_id: series_id.clone(),
            outcome_id: None,
            buyer,
            seller,
            qty: qty as i128,
            price,
            buyer_unblock_amount: None,
            seller_unblock_amount: None,
        },
    );

    // 0.0 USD with 8 decimals = 0
    settle_series_checked(&series, SettlementInput::Price(Price::new(0, 8)));

    // Final Balances:
    // Buyer: $7 (cash) + 10 * $0 (payoff) = $7.00 = 7,000,000
    // Seller: $13 (cash) + 10 * $1.00 (payoff for short on loss) = $23.00 = 23,000,000
    verify_cash_balance(buyer, 7_000_000);
    verify_cash_balance(seller, 23_000_000);
}
