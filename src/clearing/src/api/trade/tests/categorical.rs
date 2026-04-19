use candid::Principal;
use shared::types::{
    BalanceDomain, Description, Outcome, OutcomeId, PayoffType, PayoutUnit, Price, Series,
    SeriesId, SettlementInput,
};

use crate::{
    api::trade::{
        api::{mint_complete_set_logic, redeem_complete_set_logic},
        tests::utils::*,
    },
    trade::types::ExecuteTradeParams,
    types::trade::TradeId,
};

#[test]
fn categorical_mint_redeem_complete_set() {
    let user = create_test_user(1);
    let series_id = SeriesId::from("cat_test".to_owned());
    let outcome_a = OutcomeId::from("A".to_owned());
    let outcome_b = OutcomeId::from("B".to_owned());

    let series = Series {
        series_id: series_id.clone(),
        underlying: "ICP".to_owned(),
        expiry_ns: 2_000_000_000,
        payoff_type: PayoffType::Categorical,
        strike: None,
        price_precision: 8,
        payout_unit: PayoutUnit::usd(),
        outcomes: Some(vec![
            Outcome {
                id: outcome_a.clone(),
                title: "A".to_owned(),
                description: None,
                icon_url: None,
            },
            Outcome {
                id: outcome_b.clone(),
                title: "B".to_owned(),
                description: None,
                icon_url: None,
            },
        ]),
        oracle_source: "oracle".to_owned(),
        creator: Principal::anonymous(),
        created_at_ns: 1_000_000_000,
        title: "Cat Test".to_owned(),
        description: Description::plain("Categorical test"),
        icon_url: None,
        banner_url: None,
        balance_domain: BalanceDomain::Settlement,
        trading_access: vec![],
        engine_id: None,
        forked_from: None,
    };

    setup_test_state(vec![(user, 100_000)]);

    mint_complete_set_logic(user, &series_id.clone(), &series.clone(), 5).expect("mint failed");
    verify_cash_balance(user, 100_000); // Cash stays same
    verify_position_qty(user, &series_id, Some(&outcome_a.clone()), 5);
    verify_position_qty(user, &series_id, Some(&outcome_b.clone()), 5);

    redeem_complete_set_logic(user, &series_id.clone(), &series, 2).expect("redeem failed");
    verify_cash_balance(user, 100_000); // Cash stays same
    verify_position_qty(user, &series_id, Some(&outcome_a.clone()), 3);
    verify_position_qty(user, &series_id, Some(&outcome_b.clone()), 3);
}

#[test]
fn categorical_lifecycle_scenario() {
    let seller = create_test_user(1);
    let buyer = create_test_user(2);
    let series_id = SeriesId::from("lifecycle_test".to_owned());
    let outcome_a = OutcomeId::from("A".to_owned());
    let outcome_b = OutcomeId::from("B".to_owned());
    let outcome_c = OutcomeId::from("C".to_owned());

    let series = Series {
        series_id: series_id.clone(),
        underlying: "EVENT_2024".to_owned(),
        expiry_ns: 2_000_000_000,
        payoff_type: PayoffType::Categorical,
        strike: None,
        price_precision: 8,
        payout_unit: PayoutUnit::usd(),
        outcomes: Some(vec![
            Outcome {
                id: outcome_a.clone(),
                title: "A".to_owned(),
                description: None,
                icon_url: None,
            },
            Outcome {
                id: outcome_b.clone(),
                title: "B".to_owned(),
                description: None,
                icon_url: None,
            },
            Outcome {
                id: outcome_c.clone(),
                title: "C".to_owned(),
                description: None,
                icon_url: None,
            },
        ]),
        oracle_source: "oracle".to_owned(),
        creator: Principal::anonymous(),
        created_at_ns: 1_000_000_000,
        title: "Lifecycle Test".to_owned(),
        description: Description::plain("Full scenario test"),
        icon_url: None,
        banner_url: None,
        balance_domain: BalanceDomain::Settlement,
        trading_access: vec![],
        engine_id: None,
        forked_from: None,
    };

    setup_test_state(vec![(seller, 200_000), (buyer, 100_000)]);

    mint_complete_set_logic(seller, &series_id.clone(), &series.clone(), 10).expect("mint failed");
    verify_cash_balance(seller, 200_000); // Cash stays same

    execute_trade_checked(
        &series.clone(),
        ExecuteTradeParams {
            trade_id: TradeId::from("trade_1".to_owned()),
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

    verify_cash_balance(seller, 206_667); // 200,000 + 6,667 PnL from selling A at profit
    verify_cash_balance(buyer, 100_000); // Cash stays same

    verify_position_qty(seller, &series_id, Some(&outcome_a.clone()), 0);
    verify_position_qty(seller, &series_id, Some(&outcome_b.clone()), 10);
    verify_position_qty(seller, &series_id, Some(&outcome_c.clone()), 10);
    verify_position_qty(buyer, &series_id, Some(&outcome_a.clone()), 10);

    settle_series_checked(&series, &SettlementInput::Outcome(outcome_a.clone()));

    verify_cash_balance(buyer, 160_000); // 100k initial + (100k payoff - 40k margin)
    verify_cash_balance(seller, 140_001); // 206,667 post-trade - 66,666 margin release

    verify_position_qty(buyer, &series_id, Some(&outcome_a.clone()), 0);
    verify_position_qty(seller, &series_id, Some(&outcome_b.clone()), 0);
    verify_position_qty(seller, &series_id, Some(&outcome_c.clone()), 0);
}

#[test]
fn categorical_short_settlement() {
    let seller = create_test_user(1);
    let buyer = create_test_user(2);
    let series_id = SeriesId::from("short_test".to_owned());
    let outcome_a = OutcomeId::from("A".to_owned());
    let outcome_b = OutcomeId::from("B".to_owned());

    let series = Series {
        series_id: series_id.clone(),
        underlying: "EVENT".to_owned(),
        expiry_ns: 2_000_000_000,
        payoff_type: PayoffType::Categorical,
        strike: None,
        price_precision: 8,
        payout_unit: PayoutUnit::usd(),
        outcomes: Some(vec![
            Outcome {
                id: outcome_a.clone(),
                title: "A".to_owned(),
                description: None,
                icon_url: None,
            },
            Outcome {
                id: outcome_b.clone(),
                title: "B".to_owned(),
                description: None,
                icon_url: None,
            },
        ]),
        oracle_source: "oracle".to_owned(),
        creator: Principal::anonymous(),
        created_at_ns: 1_000_000_000,
        title: "Short Test".to_owned(),
        description: Description::plain("Short test"),
        icon_url: None,
        banner_url: None,
        balance_domain: BalanceDomain::Settlement,
        trading_access: vec![],
        engine_id: None,
        forked_from: None,
    };

    setup_test_state(vec![(seller, 100_000), (buyer, 100_000)]);

    execute_trade_checked(
        &series.clone(),
        ExecuteTradeParams {
            trade_id: TradeId::from("short_trade".to_owned()),
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

    // After trade, cash stays same
    verify_cash_balance(buyer, 100_000);
    verify_cash_balance(seller, 100_000);

    // Settle with Outcome B winning (so A loses)
    settle_series_checked(&series, &SettlementInput::Outcome(outcome_b.clone()));

    // Results:
    // Buyer (Long A): receives 0. Final = 100k - 40k = 60k.
    // Seller (Short A): receives 1.0 payoff. Final = 100k + (100k - 60k) = 140k.
    verify_cash_balance(buyer, 60_000);
    verify_cash_balance(seller, 140_000);
}

#[test]
fn categorical_short_loss() {
    let seller = create_test_user(1);
    let buyer = create_test_user(2);
    let series_id = SeriesId::from("short_loss_test".to_owned());
    let outcome_a = OutcomeId::from("A".to_owned());
    let outcome_b = OutcomeId::from("B".to_owned());

    let series = Series {
        series_id: series_id.clone(),
        underlying: "EVENT".to_owned(),
        expiry_ns: 2_000_000_000,
        payoff_type: PayoffType::Categorical,
        strike: None,
        price_precision: 8,
        payout_unit: PayoutUnit::usd(),
        outcomes: Some(vec![
            Outcome {
                id: outcome_a.clone(),
                title: "A".to_owned(),
                description: None,
                icon_url: None,
            },
            Outcome {
                id: outcome_b.clone(),
                title: "B".to_owned(),
                description: None,
                icon_url: None,
            },
        ]),
        oracle_source: "oracle".to_owned(),
        creator: Principal::anonymous(),
        created_at_ns: 1_000_000_000,
        title: "Short Loss Test".to_owned(),
        description: Description::plain("Short loss test"),
        icon_url: None,
        banner_url: None,
        balance_domain: BalanceDomain::Settlement,
        trading_access: vec![],
        engine_id: None,
        forked_from: None,
    };

    setup_test_state(vec![(seller, 100_000), (buyer, 100_000)]);

    execute_trade_checked(
        &series.clone(),
        ExecuteTradeParams {
            trade_id: TradeId::from("short_loss_trade".to_owned()),
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
    settle_series_checked(&series, &SettlementInput::Outcome(outcome_a.clone()));

    // Results:
    // Buyer (Long A): receives 1.0 payoff. Final = 100k + (100k - 40k) = 160k.
    // Seller (Short A): receives 0. Final = 100k - 60k = 40k.
    verify_cash_balance(buyer, 160_000);
    verify_cash_balance(seller, 40_000);
}
