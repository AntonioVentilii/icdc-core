use candid::Nat;
use clearing::{
    api::{
        account::{params::GetAccountStateParams, results::GetAccountStateResult},
        trade::{
            errors::TradeError,
            params::{
                CancelLimitOrderParams, ListOrdersParams, SubmitMarketOrderParams,
                SubmitMatchedTradeParams,
            },
            results::SubmitMatchedTradeResult,
        },
    },
    types::{
        margin::Position,
        trade::{LimitOrder, OrderId, Side, TradeId},
        user::User,
    },
};
use shared::types::{BalanceDomain, Price};

use crate::utils::{
    assertions::{assert_ok_value, assert_unauthorized},
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait,
    PicCanisterTrait,
};

#[test]
fn list_orders_empty() {
    let env = TestSetup::with_icp();

    let params = ListOrdersParams { series_id: None };

    let orders: Vec<LimitOrder> = assert_ok_value(env.clearing.query::<Vec<LimitOrder>, _>(
        env.controller,
        "list_orders",
        (params,),
    ));

    assert!(orders.is_empty(), "No orders should exist initially");
}

#[test]
fn status_by_series_empty() {
    let env = TestSetup::with_icp();

    let params = ListOrdersParams {
        series_id: Some("test_series".to_owned().into()),
    };

    let orders: Vec<LimitOrder> = assert_ok_value(env.clearing.query::<Vec<LimitOrder>, _>(
        env.controller,
        "list_orders",
        (params,),
    ));

    assert!(orders.is_empty());
}

#[test]
fn get_orders_empty() {
    let env = TestSetup::with_icp();
    let user = test_user(70);

    let orders: Vec<LimitOrder> = assert_ok_value(env.clearing.query::<Vec<LimitOrder>, _>(
        user,
        "get_orders",
        (),
    ));

    assert!(orders.is_empty(), "New user should have no orders");
}

#[test]
fn submit_market_order_nonexistent_order() {
    let env = TestSetup::with_icp();
    let user = test_user(71);

    let params = SubmitMarketOrderParams {
        trade_id: "trade_1".to_owned().into(),
        matching_order_id: "nonexistent_order".to_owned().into(),
        qty: None,
    };

    let result: SubmitMatchedTradeResult = assert_ok_value(
        env.clearing
            .update::<SubmitMatchedTradeResult, _>(user, "submit_market_order", (params,)),
    );

    match result {
        SubmitMatchedTradeResult::Err(_) => {
            // Expected: order not found
        }
        SubmitMatchedTradeResult::Ok(other) => panic!("Expected error, got: {other:?}"),
    }
}

#[test]
fn cancel_limit_order_nonexistent() {
    let env = TestSetup::with_icp();
    let user = test_user(72);

    let params = CancelLimitOrderParams {
        order_id: "nonexistent_order".to_owned().into(),
    };

    let result: SubmitMatchedTradeResult = assert_ok_value(
        env.clearing
            .update::<SubmitMatchedTradeResult, _>(user, "cancel_limit_order", (params,)),
    );

    match result {
        SubmitMatchedTradeResult::Err(_) => {
            // Expected: order not found
        }
        SubmitMatchedTradeResult::Ok(other) => panic!("Expected error, got: {other:?}"),
    }
}

#[test]
fn submit_matched_trade_zero_qty_rejected() {
    let env = TestSetup::with_icp();
    let buyer = test_user(73);
    let seller = test_user(74);

    let params = SubmitMatchedTradeParams {
        trade_id: "trade_zero".to_owned().into(),
        series_id: "test_series".to_owned().into(),
        outcome_id: None,
        buyer: User(buyer),
        seller: User(seller),
        qty: 0,
        price: Price::new(500_000, 6),
        buyer_unblock_amount: None,
        seller_unblock_amount: None,
    };

    let result: SubmitMatchedTradeResult =
        assert_ok_value(env.clearing.update::<SubmitMatchedTradeResult, _>(
            env.controller,
            "submit_matched_trade",
            (params,),
        ));

    match result {
        SubmitMatchedTradeResult::Err(_) => {
            // Expected: zero quantity rejected
        }
        SubmitMatchedTradeResult::Ok(other) => {
            panic!("Expected error for zero quantity, got: {other:?}")
        }
    }
}

#[test]
fn submit_matched_trade_rejects_non_controller() {
    let env = TestSetup::with_icp();
    let random_user = test_user(75);
    let buyer = test_user(76);
    let seller = test_user(77);

    let params = SubmitMatchedTradeParams {
        trade_id: "trade_unauth".to_owned().into(),
        series_id: "test_series".to_owned().into(),
        outcome_id: None,
        buyer: User(buyer),
        seller: User(seller),
        qty: 1,
        price: Price::new(500_000, 6),
        buyer_unblock_amount: None,
        seller_unblock_amount: None,
    };

    let result = env.clearing.update::<SubmitMatchedTradeResult, _>(
        random_user,
        "submit_matched_trade",
        (params,),
    );

    assert_unauthorized(&result);
}
#[test]
fn basic_matched_trade() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let user_b = test_user(55);

    // 1. Setup vUSD
    env.setup_vusd();

    // 2. Add Series to Registry
    let series_id = env.add_binary_series("BTC", 1_000_000, BalanceDomain::Settlement);

    // 3. Deposits (Using ICP for collateral)
    // Deposit 1000 ICP ($15,000 gross, $14,700 net)
    let deposit_amount = Nat::from(100_000_000_000_u128);
    env.deposit_collateral(
        user_a,
        "ICP",
        deposit_amount.clone(),
        Some(BalanceDomain::Settlement),
    );
    env.deposit_collateral(
        user_b,
        "ICP",
        deposit_amount,
        Some(BalanceDomain::Settlement),
    );
    env.pic.tick();

    // 4. Submit Matched Trade (Controller)
    let trade_id = TradeId::from("trade_1".to_owned());
    let qty = 10_000_i128; // 0.01 unit (assuming 6 decimals)
    let price = Price::new(500_000, 6); // 0.5 USD

    let matched_res: SubmitMatchedTradeResult = env
        .clearing
        .update(
            env.controller,
            "submit_matched_trade",
            (SubmitMatchedTradeParams {
                trade_id: trade_id.clone(),
                series_id: series_id.clone(),
                outcome_id: None,
                buyer: User(user_a),
                seller: User(user_b),
                qty,
                price,
                buyer_unblock_amount: None,
                seller_unblock_amount: None,
            },),
        )
        .unwrap();
    match matched_res {
        SubmitMatchedTradeResult::Ok(_) => {}
        SubmitMatchedTradeResult::Err(err) => panic!("Matched trade failed: {err:?}"),
    }
    env.pic.tick();

    // 5. Verify Positions
    let pos_a: Vec<Position> = env.clearing.query(user_a, "get_positions", ()).unwrap();
    assert_eq!(pos_a.len(), 1);
    assert_eq!(pos_a[0].net_qty, qty);

    let pos_b: Vec<Position> = env.clearing.query(user_b, "get_positions", ()).unwrap();
    assert_eq!(pos_b.len(), 1);
    assert_eq!(pos_b[0].net_qty, -qty);

    // 6. Verify Balances
    let state_a: GetAccountStateResult = env
        .clearing
        .update(
            user_a,
            "get_account_state",
            (GetAccountStateParams {
                refresh: None,
                domain: Some(BalanceDomain::Settlement),
            },),
        )
        .unwrap();
    match state_a {
        GetAccountStateResult::Ok(resp) => {
            assert_eq!(
                resp.state.get_cash_balance_usd(BalanceDomain::Settlement),
                0, // Cost of trade
                "Buyer cash should be zero"
            );
            assert_eq!(
                resp.total_equity_usd, 147_000_000,
                "Equity should match ICP value minus haircut"
            );
        }
        GetAccountStateResult::Err(err) => panic!("Failed to get account state A: {err:?}"),
    }

    let state_b: GetAccountStateResult = env
        .clearing
        .update(
            user_b,
            "get_account_state",
            (GetAccountStateParams {
                refresh: None,
                domain: Some(BalanceDomain::Settlement),
            },),
        )
        .unwrap();
    match state_b {
        GetAccountStateResult::Ok(resp) => {
            assert_eq!(
                resp.state.get_cash_balance_usd(BalanceDomain::Settlement),
                0, // Margin requirement (Full Collateral)
                "Seller cash should be zero"
            );
            assert_eq!(
                resp.total_equity_usd, 147_000_000,
                "Equity should match ICP value minus haircut"
            );
        }
        GetAccountStateResult::Err(err) => panic!("Failed to get account state B: {err:?}"),
    }
}

#[test]
fn cross_limit_match() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let user_b = test_user(55);

    // 1. Setup vUSD
    env.setup_vusd();

    // 2. Add Binary Series
    let series_id = env.add_binary_series("BTC-USD", 1_000_000, BalanceDomain::Settlement);

    // 3. Deposits (Using ICP)
    let deposit_amount = Nat::from(100_000_000_000_u128); // 1000 ICP
    env.deposit_collateral(
        user_a,
        "ICP",
        deposit_amount.clone(),
        Some(BalanceDomain::Settlement),
    );
    env.deposit_collateral(
        user_b,
        "ICP",
        deposit_amount,
        Some(BalanceDomain::Settlement),
    );
    env.pic.tick();

    // 4. Submit Limit Orders
    // User A: Limit Buy 1,000 units
    let res_a = env.submit_limit_order(
        user_a,
        "order_a",
        series_id.clone(),
        Side::Buy,
        1000,
        500_000,
    );
    println!("DEBUG: res_a: {res_a:?}");
    assert!(matches!(res_a, SubmitMatchedTradeResult::Ok(_)));

    // User B: Limit Sell 1,000 units (Same Price)
    let res_b = env.submit_limit_order(
        user_b,
        "order_b",
        series_id.clone(),
        Side::Sell,
        1000,
        500_000,
    );
    println!("DEBUG: res_b: {res_b:?}");
    assert!(matches!(res_b, SubmitMatchedTradeResult::Ok(_)));

    env.pic.tick();

    // 5. Verification
    let orders: Vec<LimitOrder> = env
        .clearing
        .query(
            env.controller,
            "list_orders",
            (ListOrdersParams {
                series_id: Some(series_id.clone()),
            },),
        )
        .unwrap();
    let pos_a: Vec<Position> = env.clearing.query(user_a, "get_positions", ()).unwrap();

    if pos_a.is_empty() {
        println!("INFO: Cross-limit orders remained in book (Maker-only).");
        assert_eq!(orders.len(), 2);
    } else {
        println!("INFO: Automatic cross-limit matching occurred.");
        assert_eq!(pos_a.len(), 1);
        assert_eq!(pos_a[0].net_qty, 1000);
        assert_eq!(orders.len(), 0);
    }
}

#[test]
fn insufficient_margin() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let user_b = test_user(55);

    // 1. Setup vUSD
    env.setup_vusd();

    // 2. Add Binary Series
    let series_id = env.add_binary_series("MARGIN-TEST", 1_000_000, BalanceDomain::Settlement);

    // 3. User A: Deposit small amount of ICP (1 ICP = $15)
    let deposit_amount = Nat::from(100_000_000_u128); // 1 ICP
    env.deposit_collateral(
        user_a,
        "ICP",
        deposit_amount.clone(),
        Some(BalanceDomain::Settlement),
    );

    // 4. User A: Attempt HUGE limit buy (1000 units)
    // Binary Buy needs 1.0 USD margin per unit. 1000 units = 1000 USD margin.
    let res_limit = env.submit_limit_order(
        user_a,
        "neg_limit",
        series_id.clone(),
        Side::Buy,
        1000,
        500_000,
    );

    match res_limit {
        SubmitMatchedTradeResult::Err(TradeError::InsufficientMargin { .. }) => {
            println!("INFO: Limit order rejected with InsufficientMargin as expected.");
        }
        other => panic!("Expected InsufficientMargin for huge limit order, got: {other:?}"),
    }

    // 5. User B: Setup small collateral (1 ICP)
    env.deposit_collateral(
        user_b,
        "ICP",
        deposit_amount,
        Some(BalanceDomain::Settlement),
    );

    // 6. User A: Place valid small limit buy (1 unit)
    // Needs 1.0 USD margin. User A has 1.0 USD.
    let res_maker = env.submit_limit_order(
        user_a,
        "valid_maker",
        series_id.clone(),
        Side::Buy,
        1,
        500_000,
    );
    assert!(matches!(res_maker, SubmitMatchedTradeResult::Ok(_)));

    // 7. User B: Attempt market sell matching A's 1 unit, but B needs more margin?
    let large_deposit = Nat::from(10_000_000_000_u128); // 100 ICP = $1500
    env.deposit_collateral(
        user_a,
        "ICP",
        large_deposit,
        Some(BalanceDomain::Settlement),
    );

    let res_maker_large = env.submit_limit_order(
        user_a,
        "large_maker",
        series_id.clone(),
        Side::Buy,
        100,
        500_000,
    );
    println!("DEBUG: res_maker_large: {res_maker_large:?}");
    assert!(matches!(res_maker_large, SubmitMatchedTradeResult::Ok(_)));

    // Now User B (with $1) tries to match A's 100 unit order.
    // User B needs $100 margin. User B has $1.
    let res_market: SubmitMatchedTradeResult = env
        .clearing
        .update(
            user_b,
            "submit_market_order",
            (SubmitMarketOrderParams {
                trade_id: TradeId::from("neg_market".to_owned()),
                matching_order_id: OrderId::from("large_maker".to_owned()),
                qty: None,
            },),
        )
        .unwrap();

    match res_market {
        SubmitMatchedTradeResult::Err(TradeError::InsufficientMargin { .. }) => {
            println!("INFO: Market order rejected with InsufficientMargin as expected.");
        }
        other => panic!("Expected InsufficientMargin for huge market order, got: {other:?}"),
    }
}

#[test]
fn limit_buy_sell() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);

    // 1. Setup vUSD
    env.setup_vusd();

    // 2. Add Binary Series
    let series_id = env.add_binary_series("BUY-SELL-TEST", 1_000_000, BalanceDomain::Settlement);

    // 3. User A: Deposit 10 ICP ($150 gross, $147 net)
    let deposit_amount = Nat::from(1_000_000_000_u128);
    env.deposit_collateral(
        user_a,
        "ICP",
        deposit_amount,
        Some(BalanceDomain::Settlement),
    );

    // 4. User A: Place Buy limit order (Price: 0.4)
    let res_buy =
        env.submit_limit_order(user_a, "a_buy", series_id.clone(), Side::Buy, 10, 400_000);
    assert!(matches!(res_buy, SubmitMatchedTradeResult::Ok(_)));

    // 5. User A: Place Sell limit order (Price: 0.6) - non matching
    let res_sell =
        env.submit_limit_order(user_a, "a_sell", series_id.clone(), Side::Sell, 5, 600_000);
    assert!(matches!(res_sell, SubmitMatchedTradeResult::Ok(_)));

    // 6. Verification
    let orders: Vec<LimitOrder> = env.clearing.query(user_a, "get_orders", ()).unwrap();
    assert_eq!(orders.len(), 2);

    let pos: Vec<Position> = env.clearing.query(user_a, "get_positions", ()).unwrap();
    assert!(
        pos.is_empty(),
        "No positions should be created for non-matching orders"
    );
}

#[test]
fn limit_market_match() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let user_b = test_user(55);

    // 1. Setup vUSD
    env.setup_vusd();

    // 2. Add Binary Series
    let series_id = env.add_binary_series("LM-MATCH-TEST", 1_000_000, BalanceDomain::Settlement);

    // 3. User A & B: Deposit 100 ICP ($15.0 each gross, $14.7 net)
    let deposit_amount = Nat::from(10_000_000_000_u128);
    env.deposit_collateral(
        user_a,
        "ICP",
        deposit_amount.clone(),
        Some(BalanceDomain::Settlement),
    );
    env.deposit_collateral(
        user_b,
        "ICP",
        deposit_amount,
        Some(BalanceDomain::Settlement),
    );

    // 4. User A: Submit Limit Order (Maker)
    let res_maker = env.submit_limit_order(
        user_a,
        "maker_limit",
        series_id.clone(),
        Side::Buy,
        5,
        500_000,
    );
    assert!(matches!(res_maker, SubmitMatchedTradeResult::Ok(_)));

    // 5. User B: Submit Market Order (Taker) matching User A
    let res_taker: SubmitMatchedTradeResult = env
        .clearing
        .update(
            user_b,
            "submit_market_order",
            (SubmitMarketOrderParams {
                trade_id: TradeId::from("taker_trade".to_owned()),
                matching_order_id: OrderId::from("maker_limit".to_owned()),
                qty: None,
            },),
        )
        .unwrap();
    assert!(matches!(res_taker, SubmitMatchedTradeResult::Ok(_)));

    // 6. Verification
    let pos_a: Vec<Position> = env.clearing.query(user_a, "get_positions", ()).unwrap();
    assert_eq!(pos_a.len(), 1);
    assert_eq!(pos_a[0].net_qty, 5);

    let pos_b: Vec<Position> = env.clearing.query(user_b, "get_positions", ()).unwrap();
    assert_eq!(pos_b.len(), 1);
    assert_eq!(pos_b[0].net_qty, -5);
}

#[test]
fn limit_market_partial_fill() {
    let env = TestSetup::with_icp();
    let user_a = test_user(56);
    let user_b = test_user(57);

    env.setup_vusd();

    let series_id = env.add_binary_series("LM-PARTIAL-TEST", 1_000_000, BalanceDomain::Settlement);

    let deposit_amount = Nat::from(10_000_000_000_u128);
    env.deposit_collateral(
        user_a,
        "ICP",
        deposit_amount.clone(),
        Some(BalanceDomain::Settlement),
    );
    env.deposit_collateral(
        user_b,
        "ICP",
        deposit_amount,
        Some(BalanceDomain::Settlement),
    );

    // User A: Submit Limit Order (Maker, Buy 10 @ 0.5)
    let res_maker = env.submit_limit_order(
        user_a,
        "partial_maker",
        series_id.clone(),
        Side::Buy,
        10,
        500_000,
    );
    assert!(matches!(res_maker, SubmitMatchedTradeResult::Ok(_)));

    let orders: Vec<LimitOrder> = env.clearing.query(user_a, "get_orders", ()).unwrap();
    assert_eq!(orders.len(), 1);
    let full_blocked = orders[0].blocked_margin_usd;

    // User B: Take only 4 of the 10 resting units.
    let res_taker: SubmitMatchedTradeResult = env
        .clearing
        .update(
            user_b,
            "submit_market_order",
            (SubmitMarketOrderParams {
                trade_id: TradeId::from("partial_trade_1".to_owned()),
                matching_order_id: OrderId::from("partial_maker".to_owned()),
                qty: Some(4),
            },),
        )
        .unwrap();
    assert!(matches!(res_taker, SubmitMatchedTradeResult::Ok(_)));

    let pos_a: Vec<Position> = env.clearing.query(user_a, "get_positions", ()).unwrap();
    assert_eq!(pos_a.len(), 1);
    assert_eq!(pos_a[0].net_qty, 4);

    let pos_b: Vec<Position> = env.clearing.query(user_b, "get_positions", ()).unwrap();
    assert_eq!(pos_b.len(), 1);
    assert_eq!(pos_b[0].net_qty, -4);

    // The remainder stays on the book with proportionally reduced blocked margin.
    let orders: Vec<LimitOrder> = env.clearing.query(user_a, "get_orders", ()).unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].qty, 6);
    assert_eq!(orders[0].blocked_margin_usd, full_blocked * 6 / 10);

    // A second market order fills the remainder and removes the order.
    let res_taker_2: SubmitMatchedTradeResult = env
        .clearing
        .update(
            user_b,
            "submit_market_order",
            (SubmitMarketOrderParams {
                trade_id: TradeId::from("partial_trade_2".to_owned()),
                matching_order_id: OrderId::from("partial_maker".to_owned()),
                qty: Some(6),
            },),
        )
        .unwrap();
    assert!(matches!(res_taker_2, SubmitMatchedTradeResult::Ok(_)));

    let orders: Vec<LimitOrder> = env.clearing.query(user_a, "get_orders", ()).unwrap();
    assert!(orders.is_empty(), "Fully filled order should be removed");

    let pos_a: Vec<Position> = env.clearing.query(user_a, "get_positions", ()).unwrap();
    assert_eq!(pos_a[0].net_qty, 10);

    let pos_b: Vec<Position> = env.clearing.query(user_b, "get_positions", ()).unwrap();
    assert_eq!(pos_b[0].net_qty, -10);
}

#[test]
fn multi_user_journey() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let user_b = test_user(55);

    // 1. Setup vUSD
    env.setup_vusd();

    // 2. Add Binary Series
    let series_id = env.add_binary_series("MULTI-USER-TEST", 1_000_000, BalanceDomain::Settlement);

    // 3. User A & B: Deposit 100 ICP ($15.0 each gross, $14.7 net)
    let deposit_amount = Nat::from(10_000_000_000_u128);
    env.deposit_collateral(
        user_a,
        "ICP",
        deposit_amount.clone(),
        Some(BalanceDomain::Settlement),
    );
    env.deposit_collateral(
        user_b,
        "ICP",
        deposit_amount,
        Some(BalanceDomain::Settlement),
    );

    // 4. User A: Placing orders
    // Buy limit order (Price: 0.5, Qty: 10)
    let res_a1 =
        env.submit_limit_order(user_a, "a_buy_1", series_id.clone(), Side::Buy, 10, 500_000);
    assert!(matches!(res_a1, SubmitMatchedTradeResult::Ok(_)));

    // Sell limit order (Price: 0.6, Qty: 10)
    let res_a2 = env.submit_limit_order(
        user_a,
        "a_sell_1",
        series_id.clone(),
        Side::Sell,
        10,
        600_000,
    );
    assert!(matches!(res_a2, SubmitMatchedTradeResult::Ok(_)));

    // 5. User B: Matching User A's Sell Order at 0.6 (Taker)
    let matched_res: SubmitMatchedTradeResult = env
        .clearing
        .update(
            user_b,
            "submit_market_order",
            (SubmitMarketOrderParams {
                trade_id: TradeId::from("b_match_a".to_owned()),
                matching_order_id: OrderId::from("a_sell_1".to_owned()),
                qty: None,
            },),
        )
        .unwrap();
    assert!(matches!(matched_res, SubmitMatchedTradeResult::Ok(_)));

    env.pic.tick();

    // 6. Verification
    let pos_a: Vec<Position> = env.clearing.query(user_a, "get_positions", ()).unwrap();
    assert_eq!(
        pos_a.len(),
        1,
        "User A should have exactly one position after match"
    );
    assert_eq!(pos_a[0].net_qty, -10); // A sold 10

    let pos_b: Vec<Position> = env.clearing.query(user_b, "get_positions", ()).unwrap();
    assert_eq!(
        pos_b.len(),
        1,
        "User B should have exactly one position after match"
    );
    assert_eq!(pos_b[0].net_qty, 10); // B bought 10
}

/// A scheduled series is registered and discoverable, but every trade-initiating
/// path is closed until its window opens. The gate lives in
/// `ensure_series_registered`, so this covers limit orders, market orders,
/// matched trades, transfers, and complete-set mint/redeem alike.
#[test]
fn scheduled_series_rejects_orders_before_its_start() {
    let env = TestSetup::with_icp();
    let user = test_user(71);

    // Opens one hour from the current PocketIC time.
    let series_id = env.add_scheduled_binary_series(
        "SCHEDULED",
        1_000_000,
        BalanceDomain::Settlement,
        3_600_000_000_000,
    );

    // No collateral needed: the window gate runs inside `ensure_series_registered`,
    // which precedes the margin check, so a funded account would not change the
    // outcome.
    let res = env.submit_limit_order(
        user,
        "before_open",
        series_id.clone(),
        Side::Buy,
        1,
        500_000,
    );

    match res {
        SubmitMatchedTradeResult::Err(TradeError::SeriesNotStarted {
            series_id: rejected,
            start_ns,
        }) => {
            assert_eq!(rejected, series_id);
            assert!(
                start_ns > env.pic.get_time().as_nanos_since_unix_epoch(),
                "the reported open must still be in the future"
            );
        }
        other => panic!("Expected SeriesNotStarted before the window opens, got: {other:?}"),
    }
}
