use candid::Nat;
use clearing::{
    api::{
        account::{params::GetAccountStateParams, results::GetAccountStateResult},
        trade::{errors::TradeError, params::*, results::*},
    },
    types::{margin::Position, user::User},
};
use shared::types::{BalanceDomain, Price};

use crate::utils::{
    assertions::assert_ok_value,
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait,
    PicCanisterTrait,
};

#[test]
fn atomicity_one_sided_insolvency() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let user_b = test_user(55);

    env.setup_vusd();
    env.pic.tick();

    // 1. Give User A enough money (10 ICP = $150 gross, $147 net)
    env.deposit_collateral(
        user_a,
        "ICP",
        Nat::from(1_000_000_000_u128),
        Some(BalanceDomain::Settlement),
    );
    // User B has $0 on the clearing canister (even if he has more on the ledger,
    // we only care about what's deposited for this test).
    env.pic.tick();

    // 2. Create a Series
    let series_id = env.add_binary_series("BTC", 1_000_000, BalanceDomain::Settlement);

    // 3. Attempt a trade where User B is insolvent
    // Buy 100 at $1.00 = $100 margin each.
    // User A (Buyer) has $100, User B (Seller) has $0 in clearing.
    let params = SubmitMatchedTradeParams {
        trade_id: "trade_fail".to_owned().into(),
        series_id: series_id.clone(),
        outcome_id: None,
        buyer: User(user_a),
        seller: User(user_b),
        qty: 100,
        price: Price::new(500_000, 6), // $0.50
        buyer_unblock_amount: None,
        seller_unblock_amount: None,
    };

    let res: SubmitMatchedTradeResult = env
        .clearing
        .update(env.controller, "submit_matched_trade", (params,))
        .unwrap();

    // Verify error for User B (Seller)
    match &res {
        SubmitMatchedTradeResult::Err(TradeError::InsufficientMargin { user, .. }) => {
            assert_eq!(*user, User(user_b));
        }
        other => panic!("Expected InsufficientMargin for user_b, got: {other:?}"),
    }

    // 4. Verify ZERO state changes for User A (no cash deduction, no position)
    let state_a: GetAccountStateResult = assert_ok_value(env.clearing.update(
        user_a,
        "get_account_state",
        (GetAccountStateParams {
            refresh: None,
            domain: Some(BalanceDomain::Settlement),
        },),
    ));

    if let GetAccountStateResult::Ok(resp) = state_a {
        assert_eq!(
            resp.state.get_cash_balance_usd(BalanceDomain::Settlement),
            0,
            "User A cash should NOT be deducted (it was 0 anyway, since we used ICP)"
        );
        assert_eq!(
            resp.total_equity_usd, 1_470_000,
            "User A equity should NOT change"
        );
    } else {
        panic!("Failed to get account state for User A");
    }

    // 5. Verify no positions created for either party
    let positions_a: Vec<Position> =
        assert_ok_value(env.clearing.query(user_a, "get_positions", ()));
    assert!(positions_a.is_empty(), "User A should have no positions");

    let positions_b: Vec<Position> =
        assert_ok_value(env.clearing.query(user_b, "get_positions", ()));
    assert!(positions_b.is_empty(), "User B should have no positions");
}

#[test]
fn atomicity_self_trading() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);

    env.setup_vusd();
    env.pic.tick();

    env.deposit_collateral(
        user_a,
        "ICP",
        Nat::from(1_000_000_000_u128), // 10 ICP = $150 ($147 net)
        Some(BalanceDomain::Settlement),
    );
    env.pic.tick();

    let series_id = env.add_binary_series("BTC", 1_000_000, BalanceDomain::Settlement);

    // Self-trading: Buy 10 at $1.00
    let params = SubmitMatchedTradeParams {
        trade_id: "trade_self".to_owned().into(),
        series_id: series_id.clone(),
        outcome_id: None,
        buyer: User(user_a),
        seller: User(user_a),
        qty: 10,
        price: Price::new(1_000_000, 6),
        buyer_unblock_amount: None,
        seller_unblock_amount: None,
    };

    let _: SubmitMatchedTradeResult = assert_ok_value(env.clearing.update(
        env.controller,
        "submit_matched_trade",
        (params,),
    ));

    let positions: Vec<Position> = assert_ok_value(env.clearing.query(user_a, "get_positions", ()));

    assert!(
        positions.iter().all(|p| p.net_qty == 0),
        "Self-trading should result in net zero qty"
    );
}

#[test]
fn atomicity_invalid_params() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let user_b = test_user(55);

    env.setup_vusd();
    env.pic.tick();

    let series_id = env.add_binary_series("BTC", 1_000_000, BalanceDomain::Settlement);

    // Case 1: Zero Qty
    let params_zero_qty = SubmitMatchedTradeParams {
        trade_id: "trade_zero_qty".to_owned().into(),
        series_id: series_id.clone(),
        outcome_id: None,
        buyer: User(user_a),
        seller: User(user_b),
        qty: 0,
        price: Price::new(1_000_000, 6),
        buyer_unblock_amount: None,
        seller_unblock_amount: None,
    };

    let res_zero: SubmitMatchedTradeResult = env
        .clearing
        .update(env.controller, "submit_matched_trade", (params_zero_qty,))
        .unwrap();
    assert!(matches!(res_zero, SubmitMatchedTradeResult::Err(_)));

    // Case 2: Zero Price
    let params_zero_price = SubmitMatchedTradeParams {
        trade_id: "trade_zero_price".to_owned().into(),
        series_id: series_id.clone(),
        outcome_id: None,
        buyer: User(user_a),
        seller: User(user_b),
        qty: 10,
        price: Price::new(0, 6),
        buyer_unblock_amount: None,
        seller_unblock_amount: None,
    };

    let res_price: SubmitMatchedTradeResult = env
        .clearing
        .update(env.controller, "submit_matched_trade", (params_zero_price,))
        .unwrap();

    assert!(matches!(res_price, SubmitMatchedTradeResult::Err(_)));
}
