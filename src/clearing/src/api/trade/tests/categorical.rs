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
        outcomes: Some(vec![
            shared::types::Outcome {
                id: outcome_a.clone(),
                title: "A".to_string(),
                description: None,
                icon_url: None,
            },
            shared::types::Outcome {
                id: outcome_b.clone(),
                title: "B".to_string(),
                description: None,
                icon_url: None,
            },
        ]),
        oracle_source: "oracle".to_string(),
        creator: Principal::anonymous(),
        created_at_ns: 1000000000,
        title: "Cat Test".to_string(),
        description: Description::plain("Categorical test"),
        icon_url: None,
        banner_url: None,
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
            shared::types::Outcome {
                id: outcome_a.clone(),
                title: "A".to_string(),
                description: None,
                icon_url: None,
            },
            shared::types::Outcome {
                id: outcome_b.clone(),
                title: "B".to_string(),
                description: None,
                icon_url: None,
            },
            shared::types::Outcome {
                id: outcome_c.clone(),
                title: "C".to_string(),
                description: None,
                icon_url: None,
            },
        ]),
        oracle_source: "oracle".to_string(),
        creator: Principal::anonymous(),
        created_at_ns: 1000000000,
        title: "Lifecycle Test".to_string(),
        description: Description::plain("Full scenario test"),
        icon_url: None,
        banner_url: None,
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

#[test]
fn test_categorical_short_settlement() {
    let seller = create_test_user(1);
    let buyer = create_test_user(2);
    let series_id = SeriesId::from("short_test".to_string());
    let outcome_a = OutcomeId::from("A".to_string());
    let outcome_b = OutcomeId::from("B".to_string());

    let series = Series {
        series_id: series_id.clone(),
        underlying: "EVENT".to_string(),
        expiry_ns: 2000000000,
        payoff_type: PayoffType::Categorical,
        strike: None,
        price_precision: 8,
        payout_unit: PayoutUnit::usd(),
        outcomes: Some(vec![
            shared::types::Outcome {
                id: outcome_a.clone(),
                title: "A".to_string(),
                description: None,
                icon_url: None,
            },
            shared::types::Outcome {
                id: outcome_b.clone(),
                title: "B".to_string(),
                description: None,
                icon_url: None,
            },
        ]),
        oracle_source: "oracle".to_string(),
        creator: Principal::anonymous(),
        created_at_ns: 1000000000,
        title: "Short Test".to_string(),
        description: Description::plain("Short test"),
        icon_url: None,
        banner_url: None,
    };

    setup_test_state(vec![(seller, 10_000_000), (buyer, 10_000_000)]);

    execute_trade_checked(
        series.clone(),
        ExecuteTradeParams {
            trade_id: TradeId::from("short_trade".to_string()),
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

    // After trade:
    // Buyer: 10 - 4 (margin/price) = 6
    // Seller: 10 - 6 (margin = 1 - 0.4) = 4
    verify_cash_balance(buyer, 6_000_000);
    verify_cash_balance(seller, 4_000_000);

    // Settle with Outcome B winning (so A loses)
    settle_series_checked(&series, SettlementInput::Outcome(outcome_b.clone()));

    // Results:
    // Buyer (Long A): receives 0. Final = 6 + 0 = 6.
    // Seller (Short A): receives 1.0 (collateral return). Final = 4 + 10 = 14.
    verify_cash_balance(buyer, 6_000_000);
    verify_cash_balance(seller, 14_000_000);
}

#[test]
fn test_categorical_short_loss() {
    let seller = create_test_user(1);
    let buyer = create_test_user(2);
    let series_id = SeriesId::from("short_loss_test".to_string());
    let outcome_a = OutcomeId::from("A".to_string());
    let outcome_b = OutcomeId::from("B".to_string());

    let series = Series {
        series_id: series_id.clone(),
        underlying: "EVENT".to_string(),
        expiry_ns: 2000000000,
        payoff_type: PayoffType::Categorical,
        strike: None,
        price_precision: 8,
        payout_unit: PayoutUnit::usd(),
        outcomes: Some(vec![
            shared::types::Outcome {
                id: outcome_a.clone(),
                title: "A".to_string(),
                description: None,
                icon_url: None,
            },
            shared::types::Outcome {
                id: outcome_b.clone(),
                title: "B".to_string(),
                description: None,
                icon_url: None,
            },
        ]),
        oracle_source: "oracle".to_string(),
        creator: Principal::anonymous(),
        created_at_ns: 1000000000,
        title: "Short Loss Test".to_string(),
        description: Description::plain("Short loss test"),
        icon_url: None,
        banner_url: None,
    };

    setup_test_state(vec![(seller, 10_000_000), (buyer, 10_000_000)]);

    execute_trade_checked(
        series.clone(),
        ExecuteTradeParams {
            trade_id: TradeId::from("short_loss_trade".to_string()),
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

    // Settle with Outcome A winning
    settle_series_checked(&series, SettlementInput::Outcome(outcome_a.clone()));

    // Results:
    // Buyer (Long A): receives 1.0. Final = 6 + 10 = 16.
    // Seller (Short A): receives 0. Final = 4 + 0 = 4.
    verify_cash_balance(buyer, 16_000_000);
    verify_cash_balance(seller, 4_000_000);
}
