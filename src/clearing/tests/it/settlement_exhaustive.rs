use candid::{Nat, Principal};
use clearing::{
    api::{
        account::{params::GetAccountStateParams, results::GetAccountStateResult},
        settlement::{params::SettleSeriesParams, results::SettleSeriesResult},
        trade::{params::SubmitMatchedTradeParams, results::SubmitMatchedTradeResult},
    },
    types::{margin::Position, trade::TradeId},
};
use shared::types::{BalanceDomain, Price, SeriesId, SettlementInput};

use crate::utils::{
    pic_canister::PicCanisterTrait as _,
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait as _,
};

#[test]
fn exhaustive_settlement_journey() {
    let env = TestSetup::default();
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

    // 3. Deposits (10,000 USD each)
    let deposit_amount = Nat::from(1_000_000_000_000_u128); // 10,000 * 10^8
    for user in &users {
        env.deposit_collateral(
            *user,
            "vUSD",
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

    // Positions S1: A=+50, B=-50, C=-50, D=0, E=+50

    // 5. Cross-Trading Series 2
    // C buys 200 from D at 0.4
    submit_trade("t4", s2.clone(), user_c, user_d, 200, 400_000);
    // D buys 100 from E at 0.3
    submit_trade("t5", s2.clone(), user_d, user_e, 100, 300_000);

    // Positions S2: C=+200, D=-100, E=-100

    env.pic.tick();

    // Verify positions before settlement
    let get_positions = |user: Principal| -> Vec<Position> {
        env.clearing.query(user, "get_positions", ()).unwrap()
    };

    for user in &users {
        let ps = get_positions(*user);
        if *user == user_d {
            assert_eq!(ps.len(), 1); // Only S2
        } else if *user == user_a || *user == user_b {
            assert_eq!(ps.len(), 1); // A & B have S1? Wait.
                                     // A: S1=+50. B: S1=-50. Correct.
        } else {
            assert_eq!(ps.len(), 2); // C, E have both
        }
    }

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

    // Verify S1 positions are gone for everyone
    for user in &users {
        let ps = get_positions(*user);
        for p in ps {
            assert!(p.series_id != s1, "Series 1 position should be removed");
        }
    }

    // Verify S1 Payouts (Check Cash Balances)
    // Formula: Final Cash = Initial (10,000) + PnL
    // A: PnL = (80-50)*100 (trade 1) + (70-80)*50 (trade 3) = 3000 - 500 = 2500 -> 25.00 USD
    // B: PnL = (50-80)*100 (trade 1) + (80-60)*50 (trade 2) = -3000 + 1000 = -2000 -> -20.00 USD
    // C: PnL = (60-80)*50 (trade 2) + Series 2 = -1000 + S2_PnL. (S1 part is -10.00 USD)
    // D: PnL = 0 + S2_PnL
    // E: PnL = (80-70)*50 (trade 3) + S2_PnL = +500 + S2_PnL. (S1 part is +5.00 USD)

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

    // A and B only had S1
    assert_eq!(get_cash(user_a), 10_024_940_000); // 10,024.94 (40.0 payoff - 0.06 fee)
    assert_eq!(get_cash(user_b), 9_979_985_000); // 9,979.985 (10.0 payoff - 0.015 fee)

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

    // Verify NO positions left for anyone
    for user in &users {
        let ps = get_positions(*user);
        assert!(ps.is_empty(), "All positions should be cleared");
    }

    // Final Cash Check (Initial 10,000 + S1_PnL + S2_PnL - Fees)
    assert_eq!(get_cash(user_c), 9_949_925_000);
    assert_eq!(get_cash(user_d), 10_029_880_000);
    assert_eq!(get_cash(user_e), 10_014_820_000);

    // Sum of all PnL should be 0 minus total fees
    let total_final_cash: i128 = users.iter().map(|u| get_cash(*u)).sum();
    // Total fees: 450,000 (0.15 for 100 units + 0.30 for 200 units)
    assert_eq!(total_final_cash, 50_000_000_000 - 450_000);
}
