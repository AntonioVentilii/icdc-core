use ::shared::types::{
    Description, OutcomeId, PayoffType, PayoutUnit, Price, Series, SeriesId, SettlementInput,
};
use candid::Principal;

use crate::{
    api::trade::{
        api::{mint_complete_set_logic, redeem_complete_set_logic},
        tests::utils::*,
    },
    trade::types::ExecuteTradeParams,
    types::trade::TradeId,
};

#[test]
fn test_categorical_mint_redeem_complete_set() {
    let user = create_test_user(1);
    let series_id = SeriesId::from("cat_test".to_string());
    let outcome_a = OutcomeId::from("A".to_string());
    let outcome_b = OutcomeId::from("B".to_string());

    let series = Series {
        series_id: series_id.clone(),
        underlying: "ICP".to_string(),
        expiry_ns: 2000000000,
        payoff_type: PayoffType::Categorical,
        strike: None,
        price_precision: 8,
        payout_unit: PayoutUnit::usd(),
        outcomes: Some(vec![outcome_a.clone(), outcome_b.clone()]),
        oracle_source: "oracle".to_string(),
        creator: Principal::anonymous(),
        created_at_ns: 1000000000,
        title: "Cat Test".to_string(),
        description: Description::plain("Categorical test"),
    };

    setup_test_state(vec![(user, 10_000_000)]);

    mint_complete_set_logic(user, series_id.clone(), series.clone(), 5).expect("mint failed");
    verify_cash_balance(user, 5_000_000);
    verify_position_qty(user, &series_id, Some(outcome_a.clone()), 5);
    verify_position_qty(user, &series_id, Some(outcome_b.clone()), 5);

    redeem_complete_set_logic(user, series_id.clone(), series, 2).expect("redeem failed");
    verify_cash_balance(user, 7_000_000);
    verify_position_qty(user, &series_id, Some(outcome_a.clone()), 3);
    verify_position_qty(user, &series_id, Some(outcome_b.clone()), 3);
}

#[test]
fn test_categorical_lifecycle_scenario() {
    let seller = create_test_user(1);
    let buyer = create_test_user(2);
    let series_id = SeriesId::from("lifecycle_test".to_string());
    let outcome_a = OutcomeId::from("A".to_string());
    let outcome_b = OutcomeId::from("B".to_string());
    let outcome_c = OutcomeId::from("C".to_string());

    let series = Series {
        series_id: series_id.clone(),
        underlying: "EVENT_2024".to_string(),
        expiry_ns: 2000000000,
        payoff_type: PayoffType::Categorical,
        strike: None,
        price_precision: 8,
        payout_unit: PayoutUnit::usd(),
        outcomes: Some(vec![
            outcome_a.clone(),
            outcome_b.clone(),
            outcome_c.clone(),
        ]),
        oracle_source: "oracle".to_string(),
        creator: Principal::anonymous(),
        created_at_ns: 1000000000,
        title: "Lifecycle Test".to_string(),
        description: Description::plain("Full scenario test"),
    };

    setup_test_state(vec![(seller, 20_000_000), (buyer, 10_000_000)]);

    mint_complete_set_logic(seller, series_id.clone(), series.clone(), 10).expect("mint failed");
    verify_cash_balance(seller, 10_000_000);

    execute_trade_checked(
        series.clone(),
        ExecuteTradeParams {
            trade_id: TradeId::from("trade_1".to_string()),
            series_id: series_id.clone(),
            outcome_id: Some(outcome_a.clone()),
            buyer,
            seller,
            qty: 10,
            price: Price::new(40_000_000, 8),
            buyer_unblock_amount: None,
            seller_unblock_amount: None,
        },
    );

    verify_cash_balance(seller, 14_000_000);
    verify_cash_balance(buyer, 6_000_000);

    verify_position_qty(seller, &series_id, Some(outcome_a.clone()), 0);
    verify_position_qty(seller, &series_id, Some(outcome_b.clone()), 10);
    verify_position_qty(seller, &series_id, Some(outcome_c.clone()), 10);
    verify_position_qty(buyer, &series_id, Some(outcome_a.clone()), 10);

    settle_series_checked(&series, SettlementInput::Outcome(outcome_a.clone()));

    verify_cash_balance(buyer, 16_000_000);
    verify_cash_balance(seller, 14_000_000);

    verify_position_qty(buyer, &series_id, Some(outcome_a.clone()), 0);
    verify_position_qty(seller, &series_id, Some(outcome_b.clone()), 0);
    verify_position_qty(seller, &series_id, Some(outcome_c.clone()), 0);
}
