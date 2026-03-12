use candid::Principal;
use shared::types::{Description, OutcomeId, PayoffType, PayoutUnit, Price, Series, SeriesId};

use crate::api::trade::api::{mint_complete_set_logic, redeem_complete_set_logic};
use crate::memory::{ACCOUNT_STATES, POSITIONS};
use crate::types::{margin::AccountState, user::User, trade::TradeId};

#[test]
fn test_categorical_mint_redeem_complete_set() {
    let user = User(Principal::anonymous());
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

    ACCOUNT_STATES.with(|acc| {
        let mut acc = acc.borrow_mut();
        acc.clear();
        let mut a = AccountState::new(user);
        a.cash_balance_usd = 10_000_000; // 10 USD
        acc.insert(user, a);
    });

    POSITIONS.with(|p| p.borrow_mut().clear());

    // Mint 5 sets -> costs 5 USD.
    let result = mint_complete_set_logic(user, series_id.clone(), series.clone(), 5);
    assert!(result.is_ok());

    ACCOUNT_STATES.with(|acc| {
        let acc = acc.borrow();
        let a = acc.get(&user).unwrap();
        assert_eq!(a.cash_balance_usd, 5_000_000); // 10 - 5 = 5 USD
    });

    POSITIONS.with(|p| {
        let p = p.borrow();
        assert_eq!(
            p.get(&(user, series_id.clone(), Some(outcome_a.clone())))
                .unwrap()
                .net_qty,
            5
        );
        assert_eq!(
            p.get(&(user, series_id.clone(), Some(outcome_b.clone())))
                .unwrap()
                .net_qty,
            5
        );
    });

    // Redeem 2 sets -> credits 2 USD.
    let result = redeem_complete_set_logic(user, series_id.clone(), series, 2);
    assert!(result.is_ok());

    ACCOUNT_STATES.with(|acc| {
        let acc = acc.borrow();
        let a = acc.get(&user).unwrap();
        assert_eq!(a.cash_balance_usd, 7_000_000); // 5 + 2 = 7 USD
    });

    POSITIONS.with(|p| {
        let p = p.borrow();
        assert_eq!(
            p.get(&(user, series_id.clone(), Some(outcome_a.clone())))
                .unwrap()
                .net_qty,
            3
        );
        assert_eq!(
            p.get(&(user, series_id.clone(), Some(outcome_b.clone())))
                .unwrap()
                .net_qty,
            3
        );
    });
}

#[test]
fn test_categorical_lifecycle_scenario() {
    let seller = User(Principal::from_slice(&[1]));
    let buyer = User(Principal::from_slice(&[2]));
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
        outcomes: Some(vec![outcome_a.clone(), outcome_b.clone(), outcome_c.clone()]),
        oracle_source: "oracle".to_string(),
        creator: Principal::anonymous(),
        created_at_ns: 1000000000,
        title: "Lifecycle Test".to_string(),
        description: Description::plain("Full scenario test"),
    };

    // 1. Setup Initial State: Seller ($20), Buyer ($10)
    ACCOUNT_STATES.with(|acc| {
        let mut acc = acc.borrow_mut();
        acc.clear();
        
        let mut s_acc = AccountState::new(seller);
        s_acc.cash_balance_usd = 20_000_000;
        acc.insert(seller, s_acc);

        let mut b_acc = AccountState::new(buyer);
        b_acc.cash_balance_usd = 10_000_000;
        acc.insert(buyer, b_acc);
    });
    POSITIONS.with(|p| p.borrow_mut().clear());

    // 2. Seller mints 10 sets (A, B, C) -> Costs $10
    mint_complete_set_logic(seller, series_id.clone(), series.clone(), 10).expect("mint failed");

    // Diagnostic Check: Verify positions actually exist
    POSITIONS.with(|p| {
        let p = p.borrow();
        let pos = p.get(&(seller, series_id.clone(), Some(outcome_a.clone())));
        assert!(pos.is_some(), "Position for outcome A should exist after minting");
        assert_eq!(pos.unwrap().net_qty, 10, "Should have 10 units after minting");
    });

    ACCOUNT_STATES.with(|acc| {
        let acc = acc.borrow();
        assert_eq!(acc.get(&seller).unwrap().cash_balance_usd, 10_000_000, "Seller balance after mint should be 10M");
    });

    // 3. Seller sells 10 units of A to Buyer @ $0.40 each (Total $4.00)
    use crate::trade::service::execute_trade_impl;
    use crate::trade::types::ExecuteTradeParams;
    use shared::types::Price;

    let trade_params = ExecuteTradeParams {
        trade_id: TradeId::from("trade_1".to_string()),
        series_id: series_id.clone(),
        outcome_id: Some(outcome_a.clone()),
        buyer, 
        seller,
        qty: 10,
        price: Price::new(40_000_000, 8), // $0.40
        buyer_unblock_amount: None,
        seller_unblock_amount: None,
    };

    execute_trade_impl(series.clone(), trade_params).expect("trade execution failed");

    // Verify State after Trade
    // Seller: $10 (prev) + $4 (sale) = $14
    // Buyer: $10 (init) - $4 (buy) = $6
    ACCOUNT_STATES.with(|acc| {
        let acc = acc.borrow();
        let s_bal = acc.get(&seller).unwrap().cash_balance_usd;
        let b_bal = acc.get(&buyer).unwrap().cash_balance_usd;
        assert_eq!(s_bal, 14_000_000, "Seller balance after trade should be 14M, got {}", s_bal);
        assert_eq!(b_bal, 6_000_000, "Buyer balance after trade should be 6M, got {}", b_bal);
    });

    // Check Positions
    // Seller: 0xA (sold), 10xB, 10xC
    // Buyer: 10xA
    POSITIONS.with(|p| {
        let p = p.borrow();
        assert_eq!(p.get(&(seller, series_id.clone(), Some(outcome_a.clone()))).unwrap().net_qty, 0);
        assert_eq!(p.get(&(seller, series_id.clone(), Some(outcome_b.clone()))).unwrap().net_qty, 10);
        assert_eq!(p.get(&(seller, series_id.clone(), Some(outcome_c.clone()))).unwrap().net_qty, 10);
        assert_eq!(p.get(&(buyer, series_id.clone(), Some(outcome_a.clone()))).unwrap().net_qty, 10);
    });

    // 4. Final Settlement: Outcome A is the winner
    use crate::api::settlement::api::{prepare_settlement_impl, apply_settlement_accounting_logic};
    use shared::types::SettlementInput;

    let settlement_input = SettlementInput::Outcome(outcome_a.clone());
    
    // a. Prepare settlement (moves from positions to plan)
    let mut plan = prepare_settlement_impl(&series, &series_id, &settlement_input, 0, 0).unwrap();
    
    // b. Apply accounting (cash balance updates)
    apply_settlement_accounting_logic(&mut plan);

    // 5. Verify Final Balances
    // Buyer: $6 (cash) + 10 * $1.00 (won A) = $16
    // Seller: $14 (cash) + 10 * $0 (lost B, C) = $14
    ACCOUNT_STATES.with(|acc| {
        let acc = acc.borrow();
        assert_eq!(acc.get(&buyer).unwrap().cash_balance_usd, 16_000_000);
        assert_eq!(acc.get(&seller).unwrap().cash_balance_usd, 14_000_000);
    });

    // Verify all positions for this series are CLEARED (removed from map or set to 0)
    POSITIONS.with(|p| {
        let p = p.borrow();
        let total_pos_for_series = p.iter()
            .filter(|((_, sid, _), _)| sid == &series_id)
            .filter(|(_, pos)| pos.net_qty != 0)
            .count();
        assert_eq!(total_pos_for_series, 0, "All positions for settled series should be cleared");
    });
}
