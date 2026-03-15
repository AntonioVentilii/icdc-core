use candid::{decode_one, encode_one, Nat, Principal};
use clearing::{
    api::{
        account::{params::GetAccountStateParams, results::GetAccountStateResult},
        admin::{params::*, results::*},
        trade::{errors::TradeError, params::*, results::*},
    },
    types::{trade::Side, user},
};
use shared::{
    constants::{ICP_LEDGER, VUSD_LEDGER},
    types::{
        AssetMetrics, BalanceDomain, DecimalValue, Description, FiatUnit, Outcome, PayoffType,
        PayoutUnit, Price, SeriesId,
    },
};

use crate::utils::{
    assertions::assert_ok_value,
    pic_canister::PicCanisterTrait as _,
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait,
};

#[test]
fn vusd_haircut_immunity() {
    let env = TestSetup::default();
    let user = test_user(54);

    env.setup_vusd();

    env.clearing
        .update::<(), _>(
            env.controller,
            "update_asset_metrics",
            (UpdateAssetMetricsParams {
                asset_id: "vUSD".to_owned(),
                metrics: AssetMetrics {
                    price_usd: DecimalValue::new(1_000_000, 6),
                    latest_transfer_fee: Some(0),
                    haircut_bps: 9000,
                    insurance_fee_ratio: None,
                    protocol_fee_ratio: None,
                    last_updated_ns: None,
                },
            },),
        )
        .expect("Failed to set vUSD metrics");

    let deposit_amount = Nat::from(10_000_000_000_u128); // 100 vUSD
    env.deposit_collateral(
        user,
        "vUSD",
        deposit_amount,
        Some(BalanceDomain::Settlement),
    );

    let state_res: GetAccountStateResult = assert_ok_value(env.clearing.update(
        user,
        "get_account_state",
        (GetAccountStateParams {
            refresh: None,
            domain: Some(BalanceDomain::Settlement),
        },),
    ));

    if let GetAccountStateResult::Ok(resp) = state_res {
        assert_eq!(
            resp.state.get_cash_balance_usd(BalanceDomain::Settlement),
            100_000_000
        );
        assert_eq!(resp.total_equity_usd, 100_000_000);
    } else {
        panic!("Failed to get account state");
    }
}

#[test]
fn vusd_ledger_controllers() {
    let env = TestSetup::default();
    let ledger_id = Principal::from_text(VUSD_LEDGER).unwrap();
    let controllers = env.pic.get_controllers(ledger_id);
    assert!(controllers.contains(&env.controller));
    assert!(controllers.contains(&env.clearing.canister_id()));
}

#[derive(candid::CandidType)]
struct AddSeriesParams {
    underlying: String,
    balance_domain: BalanceDomain,
    expiry_ns: u64,
    payoff_type: PayoffType,
    strike: Option<Price>,
    price_precision: u8,
    payout_unit: PayoutUnit,
    oracle_source: String,
    title: String,
    description: Description,
    outcomes: Option<Vec<Outcome>>,
    icon_url: Option<String>,
    banner_url: Option<String>,
}

#[derive(candid::CandidType, serde::Deserialize, Debug)]
enum AddSeriesResult {
    Ok(SeriesId),
    Err(String),
}

#[test]
fn complex_available_margin_check() {
    let env = TestSetup::default();
    let user_a = test_user(54);
    let user_b = test_user(55);

    env.setup_vusd();
    env.pic.tick();

    // Register ICP
    let icp_ledger = Principal::from_text(ICP_LEDGER).unwrap();
    let _: RegisterIcrcAssetResult = env
        .clearing
        .update(
            env.controller,
            "register_icrc_asset",
            (RegisterIcrcAssetParams {
                asset_id: "ICP".to_owned(),
                ledger_id: icp_ledger,
                haircut_bps: 200,
                oracle_id: None,
                is_enabled: true,
            },),
        )
        .unwrap();

    env.clearing
        .update::<(), _>(
            env.controller,
            "update_asset_metrics",
            (UpdateAssetMetricsParams {
                asset_id: "ICP".to_owned(),
                metrics: AssetMetrics {
                    price_usd: DecimalValue::new(15_000_000, 6),
                    latest_transfer_fee: Some(10_000),
                    haircut_bps: 200,
                    insurance_fee_ratio: None,
                    protocol_fee_ratio: None,
                    last_updated_ns: None,
                },
            },),
        )
        .unwrap();
    env.pic.tick();

    // 2. Deposits for User A
    // Deposit $50 vUSD
    env.deposit_collateral(
        user_a,
        "vUSD",
        Nat::from(5_000_000_000_u128),
        Some(BalanceDomain::Settlement),
    );
    env.pic.tick();

    // Verify vUSD is there
    let state_0: GetAccountStateResult = assert_ok_value(env.clearing.update(
        user_a,
        "get_account_state",
        (GetAccountStateParams {
            refresh: None,
            domain: Some(BalanceDomain::Settlement),
        },),
    ));
    if let GetAccountStateResult::Ok(resp) = state_0 {
        assert_eq!(
            resp.state.get_cash_balance_usd(BalanceDomain::Settlement),
            50_000_000,
            "vUSD cash missing after first deposit"
        );
    }

    // Deposit 10 ICP ($150 gross, $147 net)
    env.deposit_collateral(
        user_a,
        "ICP",
        Nat::from(1_000_000_000_u128),
        Some(BalanceDomain::Settlement),
    );
    env.pic.tick();

    // 4. Create a Binary Series
    let series_params = AddSeriesParams {
        underlying: "BTC".to_owned(),
        balance_domain: BalanceDomain::Settlement,
        expiry_ns: 2_000_000_000_000_000_000_u64,
        payoff_type: PayoffType::Binary,
        strike: Some(Price::new(1_000_000, 6)),
        price_precision: 6,
        payout_unit: PayoutUnit::Fiat(FiatUnit::Usd),
        oracle_source: "Oracle".to_owned(),
        title: "Title".to_owned(),
        description: Description::plain("Desc"),
        outcomes: None,
        icon_url: None,
        banner_url: None,
    };

    let res_bytes = env
        .pic
        .update_call(
            env.registry.canister_id(),
            env.controller,
            "add_series",
            encode_one(series_params).unwrap(),
        )
        .expect("Registry add_series call failed");
    let res: AddSeriesResult = decode_one(&res_bytes).unwrap();
    let series_id = match res {
        AddSeriesResult::Ok(id) => id,
        AddSeriesResult::Err(e) => panic!("Add series failed: {e}"),
    };
    env.pic.tick();

    // 3. User B needs money to match
    env.deposit_collateral(
        user_b,
        "vUSD",
        Nat::from(100_000_000_000_u128),
        Some(BalanceDomain::Settlement),
    );
    env.pic.tick();

    // 5. Open a position for User A (Maker)
    // Buy 100 units at $0.50. Needs $100 margin.
    let matched_res: SubmitMatchedTradeResult = env
        .clearing
        .update(
            env.controller,
            "submit_matched_trade",
            (SubmitMatchedTradeParams {
                trade_id: "trade_1".to_owned().into(),
                series_id: series_id.clone(),
                outcome_id: None,
                buyer: user::User(user_a),
                seller: user::User(user_b),
                qty: 100,
                price: Price::new(500_000, 6),
                buyer_unblock_amount: None,
                seller_unblock_amount: None,
            },),
        )
        .unwrap();
    assert!(matches!(matched_res, SubmitMatchedTradeResult::Ok(_)));
    env.pic.tick();

    // 6. Check Equity
    let state_final: GetAccountStateResult = assert_ok_value(env.clearing.update(
        user_a,
        "get_account_state",
        (GetAccountStateParams {
            refresh: None,
            domain: Some(BalanceDomain::Settlement),
        },),
    ));
    if let GetAccountStateResult::Ok(resp) = state_final {
        assert_eq!(
            resp.state.get_cash_balance_usd(BalanceDomain::Settlement),
            0,
            "Cash balance should be fully utilized for margin"
        );
        assert_eq!(
            resp.total_equity_usd, 197_000_000,
            "Total equity should remain stable"
        );
        assert_eq!(
            resp.available_margin_usd, 147_000_000,
            "Available margin should be 147_000_000"
        );
    }

    // 7. Limit check
    let res_large: SubmitMatchedTradeResult = env
        .clearing
        .update(
            user_a,
            "submit_limit_order",
            (SubmitLimitOrderParams {
                order_id: "large_order".to_owned().into(),
                series_id: series_id.clone(),
                outcome_id: None,
                side: Side::Buy,
                qty: 100_000,
                price: Price::new(500_000, 6),
            },),
        )
        .unwrap();

    match res_large {
        SubmitMatchedTradeResult::Err(TradeError::InsufficientMargin { .. }) => {}
        other => panic!("Expected InsufficientMargin, got: {other:?}"),
    }
}
