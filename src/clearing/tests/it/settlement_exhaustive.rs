use candid::{Nat, Principal};
use clearing::{
    api::{
        account::{params::GetAccountStateParams, results::GetAccountStateResult},
        settlement::{params::SettleSeriesParams, results::SettleSeriesResult},
        trade::{params::SubmitMatchedTradeParams, results::SubmitMatchedTradeResult},
    },
    types::trade::TradeId,
};
use shared::types::{BalanceDomain, Price, SeriesId, SettlementInput};

use crate::utils::{
    pic_canister::PicCanisterTrait as _,
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait as _,
};

#[test]
fn exhaustive_settlement_journey() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let user_b = test_user(55);
    let user_c = test_user(56);
    let user_d = test_user(57);
    let user_e = test_user(58);

    let users = vec![user_a, user_b, user_c, user_d, user_e];

    // 1. Setup vUSD
    env.setup_vusd();

    // 2. Add 2 Series
    let s1 = env.add_binary_series("BTC-USD", 1_000_000, BalanceDomain::Settlement);
    let s2 = env.add_binary_series("ETH-USD", 1_000_000, BalanceDomain::Settlement);

    // 3. Deposits (Using ICP for all users: 1000 ICP = $15,000 each)
    let deposit_amount = Nat::from(100_000_000_000_u128);
    for user in &users {
        env.deposit_collateral(
            *user,
            "ICP",
            deposit_amount.clone(),
            Some(BalanceDomain::Settlement),
        );
    }
    env.pic.tick();

    // Helper for manual matched trade via controller
    let submit_trade = |trade_id_str: &str,
                        series_id: SeriesId,
                        buyer: Principal,
                        seller: Principal,
                        qty: i128,
                        price_val: u128| {
        let matched_res: SubmitMatchedTradeResult = env
            .clearing
            .update(
                env.controller,
                "submit_matched_trade",
                (SubmitMatchedTradeParams {
                    trade_id: TradeId::from(trade_id_str.to_owned()),
                    series_id,
                    outcome_id: None,
                    buyer: buyer.into(),
                    seller: seller.into(),
                    qty,
                    price: Price::new(price_val, 6),
                    buyer_unblock_amount: None,
                    seller_unblock_amount: None,
                },),
            )
            .unwrap();
        assert!(matches!(matched_res, SubmitMatchedTradeResult::Ok(_)));
    };

    // 4. Cross-Trading Series 1
    // A buys 100 from B at 0.5
    submit_trade("t1", s1.clone(), user_a, user_b, 100, 500_000);
    // B buys 50 from C at 0.6
    submit_trade("t2", s1.clone(), user_b, user_c, 50, 600_000);
    // E buys 50 from A at 0.7
    submit_trade("t3", s1.clone(), user_e, user_a, 50, 700_000);

    // 5. Cross-Trading Series 2
    // C buys 200 from D at 0.4
    submit_trade("t4", s2.clone(), user_c, user_d, 200, 400_000);
    // D buys 100 from E at 0.3
    submit_trade("t5", s2.clone(), user_d, user_e, 100, 300_000);

    env.pic.tick();

    // 6. Settle Series 1 at 0.8
    let res1: SettleSeriesResult = env
        .clearing
        .update(
            env.controller,
            "settle_series",
            (SettleSeriesParams {
                series_id: s1.clone(),
                settlement: SettlementInput::Price(Price::new(800_000, 6)),
            },),
        )
        .unwrap();
    assert!(matches!(res1, SettleSeriesResult::Ok));
    env.pic.tick();

    let get_cash = |user: Principal| -> i128 {
        let resp: GetAccountStateResult = env
            .clearing
            .update(
                user,
                "get_account_state",
                (GetAccountStateParams {
                    refresh: None,
                    domain: Some(BalanceDomain::Settlement),
                },),
            )
            .unwrap();
        match resp {
            GetAccountStateResult::Ok(r) => r.state.get_cash_balance_usd(BalanceDomain::Settlement),
            GetAccountStateResult::Err(e) => panic!("Account state error: {e:?}"),
        }
    };

    // A: PnL +25.00 USD. Fees -0.06. Final Cash = 24.94
    assert_eq!(get_cash(user_a), 249_400);
    // B: PnL -20.00 USD. Fees -0.015. Final Cash = -20.015
    assert_eq!(get_cash(user_b), -200_150);

    // 7. Settle Series 2 at 0.2
    let res2: SettleSeriesResult = env
        .clearing
        .update(
            env.controller,
            "settle_series",
            (SettleSeriesParams {
                series_id: s2.clone(),
                settlement: SettlementInput::Price(Price::new(200_000, 6)),
            },),
        )
        .unwrap();
    assert!(matches!(res2, SettleSeriesResult::Ok));
    env.pic.tick();

    // Final Cash Check (PnL - Fees)
    assert_eq!(get_cash(user_c), -500_750);
    assert_eq!(get_cash(user_d), 298_800);
    assert_eq!(get_cash(user_e), 148_200);

    // Sum of all PnL should be 0 minus total fees
    let total_final_cash: i128 = users.iter().map(|u| get_cash(*u)).sum();
    // Total fees: 0.45 USD
    assert_eq!(total_final_cash, -4_500);
}

/// End-to-end forward (Linear) round-trip: register a `Linear` series, match a
/// forward at an agreed rate, settle above it, and assert the long earns a
/// signed profit while the short takes the mirror loss.
#[test]
fn linear_forward_settles_with_signed_pnl() {
    let env = TestSetup::with_icp();
    // Only test_user(54..=58) are pre-funded with ICP by the harness.
    let long = test_user(54);
    let short = test_user(55);

    env.setup_vusd();

    // Forward on USD/BRL with a $20.00 settlement cap.
    let s = env.add_linear_series("USDBRL", 20_000_000, BalanceDomain::Settlement);

    // Fund both sides (1000 ICP = $15,000 each; short margin = 100 * ($20 - $5) = $1,500).
    let deposit = Nat::from(100_000_000_000_u128);
    for user in [long, short] {
        env.deposit_collateral(
            user,
            "ICP",
            deposit.clone(),
            Some(BalanceDomain::Settlement),
        );
    }
    env.pic.tick();

    // `long` buys 100 from `short` at the agreed forward rate $5.00.
    let matched: SubmitMatchedTradeResult = env
        .clearing
        .update(
            env.controller,
            "submit_matched_trade",
            (SubmitMatchedTradeParams {
                trade_id: TradeId::from("lin_fwd".to_owned()),
                series_id: s.clone(),
                outcome_id: None,
                buyer: long.into(),
                seller: short.into(),
                qty: 100,
                price: Price::new(5_000_000, 6),
                buyer_unblock_amount: None,
                seller_unblock_amount: None,
            },),
        )
        .unwrap();
    assert!(matches!(matched, SubmitMatchedTradeResult::Ok(_)));
    env.pic.tick();

    // Settle at $8.00: long gains ~100 * ($8 - $5) = +$300, short loses the mirror.
    let res: SettleSeriesResult = env
        .clearing
        .update(
            env.controller,
            "settle_series",
            (SettleSeriesParams {
                series_id: s.clone(),
                settlement: SettlementInput::Price(Price::new(8_000_000, 6)),
            },),
        )
        .unwrap();
    assert!(matches!(res, SettleSeriesResult::Ok));
    env.pic.tick();

    let get_cash = |user: Principal| -> i128 {
        let resp: GetAccountStateResult = env
            .clearing
            .update(
                user,
                "get_account_state",
                (GetAccountStateParams {
                    refresh: None,
                    domain: Some(BalanceDomain::Settlement),
                },),
            )
            .unwrap();
        match resp {
            GetAccountStateResult::Ok(r) => r.state.get_cash_balance_usd(BalanceDomain::Settlement),
            GetAccountStateResult::Err(e) => panic!("Account state error: {e:?}"),
        }
    };

    let long_pnl = get_cash(long);
    let short_pnl = get_cash(short);

    // Delta-one payoff S_T - F: long profits, short takes the mirror loss.
    assert!(long_pnl > 0, "long should profit, got {long_pnl}");
    assert!(short_pnl < 0, "short should lose, got {short_pnl}");
    // Magnitude ~ $300.00 (= 3_000_000 in 4-dp USD), within settlement fees.
    assert!(
        (long_pnl - 3_000_000).abs() < 30_000,
        "long PnL should be ~ +$300, got {long_pnl}"
    );
    assert!(
        (short_pnl + 3_000_000).abs() < 30_000,
        "short PnL should be ~ -$300, got {short_pnl}"
    );
    // The two legs cancel except for fees paid out of the pool (zero-sum minus fees).
    assert!(
        long_pnl + short_pnl <= 0,
        "net across counterparties must be -fees, got {}",
        long_pnl + short_pnl
    );
}
