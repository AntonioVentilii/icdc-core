use candid::{Nat, Principal};
use clearing::{
    api::{
        account::{params::GetAccountStateParams, results::GetAccountStateResult},
        admin::{errors::RegisterIcrcAssetError, params::*, results::*},
        collateral::{params::DepositCollateralParams, results::DepositCollateralResult},
        trade::{params::SubmitMatchedTradeParams, results::*},
    },
    types::user::{self, DepositId},
};
use shared::types::{BalanceDomain, CollateralAssetInfo, Price};

use crate::utils::{
    assertions::assert_ok_value,
    constants::{VUSD_ASSET_ID, VUSD_LEDGER},
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait,
    PicCanisterTrait,
};

#[test]
fn vusd_cannot_be_deposited() {
    let env = TestSetup::default();
    let user = test_user(54);

    env.setup_vusd();

    // Try to deposit vUSD - should fail or be blocked by logic
    let deposit_res: DepositCollateralResult = env
        .clearing
        .update(
            user,
            "deposit_collateral",
            (DepositCollateralParams {
                deposit_id: DepositId("test_vusd_dep".to_owned()),
                asset_id: VUSD_ASSET_ID.to_owned(),
                amount: Nat::from(1_000_000_000_u64),
                domain: Some(BalanceDomain::Settlement),
            },),
        )
        .unwrap();

    assert!(matches!(deposit_res, DepositCollateralResult::Err(_)));
}

#[test]
fn vusd_ledger_controllers() {
    let env = TestSetup::default();
    let ledger_id = Principal::from_text(VUSD_LEDGER).unwrap();
    let controllers = env.pic.get_controllers(ledger_id);
    assert!(controllers.contains(&env.controller));
    assert!(controllers.contains(&env.clearing.canister_id()));
}

#[test]
fn complex_available_margin_check() {
    let env = TestSetup::default();
    let user_a = test_user(54);
    let user_b = test_user(55);

    env.setup_vusd();
    env.pic.tick();

    // 1. Deposits for User A (15 ICP = $225 gross, $220.5 net)
    env.deposit_collateral(
        user_a,
        "ICP",
        Nat::from(1_500_000_000_u128),
        Some(BalanceDomain::Settlement),
    );
    // 2. Deposits for User B (10 ICP = $150 gross, $147 net)
    env.deposit_collateral(
        user_b,
        "ICP",
        Nat::from(1_000_000_000_u128),
        Some(BalanceDomain::Settlement),
    );
    env.pic.tick();

    // 3. Create a Binary Series
    let series_id = env.add_binary_series("BTC", 1_000_000, BalanceDomain::Settlement);

    // 4. Open a position for User A (Maker)
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

    // 5. Check Equity and Margin for User A
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
            -50_000_000,
            "Cash balance should be negative $50"
        );
        assert_eq!(
            resp.total_equity_usd, 220_500_000,
            "Total equity should match ICP value minus haircut"
        );
        // Available margin: 220.5 (equity) - 50 (reserved) = 170.5
        assert_eq!(
            resp.available_margin_usd, 170_500_000,
            "Available margin calculation mismatch"
        );
    }
}

#[test]
fn vusd_registration_checks() {
    let env = TestSetup::default();
    let vusd_ledger = env.ledgers.get(VUSD_ASSET_ID).unwrap().canister_id();

    // 1. Cannot register vUSD with a haircut
    let res: RegisterIcrcAssetResult = env
        .clearing
        .update(
            env.controller,
            "register_icrc_asset",
            (RegisterIcrcAssetParams {
                asset_id: VUSD_ASSET_ID.to_owned(),
                ledger_id: vusd_ledger,
                haircut_bps: 100, // Non-zero haircut
                oracle_id: None,
                is_enabled: true,
            },),
        )
        .unwrap();

    assert!(matches!(
        res,
        RegisterIcrcAssetResult::Err(RegisterIcrcAssetError::VusdCannotBeCollateral)
    ));

    // 2. Verify vUSD is NOT in collateral assets list
    let collateral_assets: Vec<CollateralAssetInfo> = env
        .clearing
        .query(env.user, "get_collateral_assets", ())
        .unwrap();
    assert!(collateral_assets
        .iter()
        .all(|a| a.config.asset_id != VUSD_ASSET_ID));
}
