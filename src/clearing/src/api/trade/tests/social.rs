use candid::Principal;
use shared::types::{
    BalanceDomain, Description, NonMonetaryUnit, PayoffType, PayoutUnit, Price, Series, SeriesId,
    SettlementInput, SocialReward,
};

use crate::{
    api::trade::tests::utils::*,
    memory::POSITIONS,
    trade::types::ExecuteTradeParams,
    types::{margin::PositionsMap, trade::TradeId},
    RefCell,
};

#[test]
fn social_trade_scenario() {
    let seller = create_test_user(1);
    let buyer = create_test_user(2);
    let series_id = SeriesId::from("social_pizza_bet".to_owned());

    let series = Series {
        series_id: series_id.clone(),
        underlying: "PIZZA_CHALLENGE".to_owned(),
        expiry_ns: 2_000_000_000,
        payoff_type: PayoffType::Binary,
        strike: None,
        price_precision: 0,
        payout_unit: PayoutUnit::NonMonetary(NonMonetaryUnit::Social(SocialReward {
            title: "Pizza 🍕".to_owned(),
            description: Some("Betting a Pepperoni Pizza on whether I stop smoking".to_owned()),
            icon_url: Some("https://example.com/pizza.png".to_owned()),
        })),
        outcomes: None,
        oracle_source: "social_oracle".to_owned(),
        creator: Principal::anonymous(),
        created_at_ns: 1_000_000_000,
        title: "Stop Smoking Pizza Bet".to_owned(),
        description: Description::plain("Betting a pizza on whether I stop smoking"),
        icon_url: None,
        banner_url: None,
        balance_domain: BalanceDomain::Social,
        trading_access: vec![],
        engine_id: None,
        forked_from: None,
        locale: None,
    };

    // SETUP: Users have ZERO balance
    setup_test_state(vec![(seller, 0), (buyer, 0)]);

    let price = Price::new(50, 0); // 50% probability
    let qty = 1;

    // TRADE: Should succeed even with 0 balance
    execute_trade_checked(
        &series.clone(),
        ExecuteTradeParams {
            trade_id: TradeId::from("trade_social_1".to_owned()),
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

    // VERIFY: reserved_margin_usd should be 0
    verify_position_qty(buyer, &series_id, None, 1);
    verify_position_qty(seller, &series_id, None, -1);

    // Check internal reservation (margin) is 0
    POSITIONS.with(|p: &RefCell<PositionsMap>| {
        let p = p.borrow();
        let b_pos = p.get(&(buyer, series_id.clone(), None)).unwrap();
        let s_pos = p.get(&(seller, series_id.clone(), None)).unwrap();
        assert_eq!(b_pos.reserved_margin_usd, 0);
        assert_eq!(s_pos.reserved_margin_usd, 0);
    });

    // SETTLEMENT: Result is "Yes" (Price = 1.0)
    settle_series_checked(&series, &SettlementInput::Price(Price::new(1, 0)));

    // VERIFY: Balances remain 0
    verify_cash_balance(buyer, 0);
    verify_cash_balance(seller, 0);

    // Ensure positions are cleared
    POSITIONS.with(|p: &RefCell<PositionsMap>| {
        let p = p.borrow();
        assert!(!p.contains_key(&(buyer, series_id.clone(), None)));
        assert!(!p.contains_key(&(seller, series_id, None)));
    });
}
