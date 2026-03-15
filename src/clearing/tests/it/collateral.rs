use candid::{decode_one, encode_one, Nat, Principal};
use clearing::{
    api::{
        account::{params::GetAccountStateParams, results::GetAccountStateResult},
        admin::params::UpdateCollateralAssetParams,
        collateral::{params::DepositCollateralParams, results::DepositCollateralResult},
    },
    types::user::DepositId,
};
use icrc_ledger_types::icrc2::approve::{ApproveArgs, ApproveError};
use shared::{
    constants::{ICP_LEDGER, VUSD_LEDGER},
    types::{
        evm::NativeEvmAsset, Asset, BalanceDomain, CollateralAssetConfig, CollateralAssetInfo,
    },
};

use crate::utils::{
    assertions::{assert_decimal_eq, assert_ok_value},
    pic_canister::PicCanisterTrait as _,
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait,
};

#[test]
fn get_collateral_assets_empty() {
    let env = TestSetup::default();
    let user = test_user(50);

    let assets: Vec<CollateralAssetInfo> = assert_ok_value(
        env.clearing
            .query::<Vec<CollateralAssetInfo>, _>(user, "get_collateral_assets", ()),
    );

    assert!(
        assets.is_empty(),
        "No collateral assets should be registered initially"
    );
}

#[test]
fn get_collateral_assets_after_registration() {
    let env = TestSetup::default();
    let user = test_user(51);

    let params = UpdateCollateralAssetParams {
        config: CollateralAssetConfig {
            asset_id: "ETH".to_owned(),
            asset: Asset::NativeEvm(NativeEvmAsset {
                chain_id: 1,
                decimals: 18,
            }),
            symbol: "ETH".to_owned(),
            decimals: 18,
            is_enabled: true,
            oracle_id: None,
        },
    };

    assert_ok_value(env.clearing.update::<(), _>(
        env.controller,
        "update_collateral_asset",
        (params,),
    ));

    let assets: Vec<CollateralAssetInfo> = assert_ok_value(
        env.clearing
            .query::<Vec<CollateralAssetInfo>, _>(user, "get_collateral_assets", ()),
    );

    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].config.asset_id, "ETH");
    assert!(assets[0].config.is_enabled);
    assert!(assets[0].metrics.is_none());
}

#[test]
fn get_collateral_assets_with_metrics() {
    let env = TestSetup::default();
    let user = test_user(52);

    env.setup_evm_asset("ETH", "ETH", 18, 3_000_000_000, 6, 500);

    let assets: Vec<CollateralAssetInfo> = assert_ok_value(
        env.clearing
            .query::<Vec<CollateralAssetInfo>, _>(user, "get_collateral_assets", ()),
    );

    assert_eq!(assets.len(), 1);

    let metrics = assets[0].metrics.as_ref().expect("Metrics should be set");
    assert_decimal_eq(&metrics.price_usd, 3_000_000_000, 6);

    assert_eq!(metrics.haircut_bps, 500);
}

#[test]
fn deposit_collateral_unsupported_asset() {
    let env = TestSetup::default();
    let user = test_user(53);

    let params = DepositCollateralParams {
        deposit_id: DepositId("dep_1".to_owned()),
        domain: None,
        asset_id: "NONEXISTENT".to_owned(),
        amount: Nat::from(1_000_000_u64),
    };

    let result: DepositCollateralResult = assert_ok_value(
        env.clearing
            .update::<DepositCollateralResult, _>(user, "deposit_collateral", (params,)),
    );

    match result {
        DepositCollateralResult::Err(_) => {}
        DepositCollateralResult::Ok => panic!("Expected error for unsupported asset"),
    }
}

#[test]
fn deposit_and_domain_isolation() {
    let env = TestSetup::default();
    let user = test_user(54);

    // 1. Register Assets (Icrc)
    let icp_ledger = Principal::from_text(ICP_LEDGER).unwrap();
    let vusd_ledger = Principal::from_text(VUSD_LEDGER).unwrap();

    // Register and set metrics for assets
    env.setup_icrc_asset("ICP", icp_ledger, 15_000_000_000, 9, 200, Some(10_000));
    env.setup_icrc_asset("vUSD", vusd_ledger, 1_000_000_000, 9, 0, Some(0));

    // 1.5 Approve Clearing to spend user's tokens
    let approve_params = ApproveArgs {
        from_subaccount: None,
        spender: env.clearing.canister_id().into(),
        amount: Nat::from(2_000_000_000_u64), // enough for all deposits + fees
        expected_allowance: None,
        expires_at: None,
        fee: None,
        memo: None,
        created_at_time: None,
    };

    let icp_approve_bytes = env
        .pic
        .update_call(
            icp_ledger,
            user,
            "icrc2_approve",
            encode_one(approve_params.clone()).unwrap(),
        )
        .expect("ICP Approval call failed");
    let icp_approve_res: Result<Nat, ApproveError> =
        decode_one(&icp_approve_bytes).expect("Failed to decode ICP approval");
    icp_approve_res.expect("ICP Approval error");
    env.pic.tick();

    let vusd_approve_bytes = env
        .pic
        .update_call(
            vusd_ledger,
            user,
            "icrc2_approve",
            encode_one(approve_params).unwrap(),
        )
        .expect("vUSD Approval call failed");
    let vusd_approve_res: Result<Nat, ApproveError> =
        decode_one(&vusd_approve_bytes).expect("Failed to decode vUSD approval");
    vusd_approve_res.expect("vUSD Approval error");
    env.pic.tick();

    // 2. Deposit ICP into Settlement
    let dep_1 = DepositCollateralParams {
        deposit_id: DepositId("DEP_ICP_SETTLE".to_owned()),
        asset_id: "ICP".to_owned(),
        amount: Nat::from(100_000_000_u64), // 1 ICP
        domain: Some(BalanceDomain::Settlement),
    };
    let res1 = env
        .clearing
        .update::<DepositCollateralResult, _>(user, "deposit_collateral", (dep_1,))
        .expect("Failed to call deposit_collateral");
    match res1 {
        DepositCollateralResult::Ok => {}
        DepositCollateralResult::Err(e) => panic!("Deposit 1 failed: {e:?}"),
    }
    env.pic.tick();

    // 3. Deposit ICP into Playground
    let dep_2 = DepositCollateralParams {
        deposit_id: DepositId("DEP_ICP_PLAY".to_owned()),
        asset_id: "ICP".to_owned(),
        amount: Nat::from(50_000_000_u64), // 0.5 ICP
        domain: Some(BalanceDomain::Playground),
    };
    let res2 = env
        .clearing
        .update::<DepositCollateralResult, _>(user, "deposit_collateral", (dep_2,))
        .expect("Failed to call deposit_collateral");
    match res2 {
        DepositCollateralResult::Ok => {}
        DepositCollateralResult::Err(e) => panic!("Deposit 2 failed: {e:?}"),
    }
    env.pic.tick();

    // 4. Deposit vUSD into Settlement
    let dep_3 = DepositCollateralParams {
        deposit_id: DepositId("DEP_VUSD_SETTLE".to_owned()),
        asset_id: "vUSD".to_owned(),
        amount: Nat::from(1_000_000_000_u64), // 10 vUSD (decimal 8)
        domain: Some(BalanceDomain::Settlement),
    };
    let res3 = env
        .clearing
        .update::<DepositCollateralResult, _>(user, "deposit_collateral", (dep_3,))
        .expect("Failed to call deposit_collateral");
    match res3 {
        DepositCollateralResult::Ok => {}
        DepositCollateralResult::Err(e) => panic!("Deposit 3 failed: {e:?}"),
    }
    env.pic.tick();

    // 5. Verify Isolation
    let assets: Vec<CollateralAssetInfo> = assert_ok_value(
        env.clearing
            .query::<Vec<CollateralAssetInfo>, _>(user, "get_collateral_assets", ()),
    );
    assert!(assets.len() >= 2);

    // Check Settlement balances
    let state_settle = env
        .clearing
        .update::<GetAccountStateResult, _>(
            user,
            "get_account_state",
            (GetAccountStateParams {
                refresh: None,
                domain: Some(BalanceDomain::Settlement),
            },),
        )
        .expect("Failed to call get_account_state");

    if let GetAccountStateResult::Ok(resp) = state_settle {
        let icp_bal = resp
            .state
            .get_balance(BalanceDomain::Settlement, &"ICP".to_owned());
        let vusd_bal = resp.state.get_cash_balance_usd(BalanceDomain::Settlement);
        assert_eq!(icp_bal, 100_000_000);
        assert_eq!(vusd_bal, 10_000_000); // $10 with 6 decimals
    } else {
        panic!("Failed to get account state for Settlement: {state_settle:?}");
    }

    // Check Playground balances
    let state_play: GetAccountStateResult = assert_ok_value(env.clearing.update(
        user,
        "get_account_state",
        (GetAccountStateParams {
            refresh: None,
            domain: Some(BalanceDomain::Playground),
        },),
    ));

    if let GetAccountStateResult::Ok(resp) = state_play {
        let icp_bal = resp
            .state
            .get_balance(BalanceDomain::Playground, &"ICP".to_owned());
        let vusd_bal = resp.state.get_cash_balance_usd(BalanceDomain::Playground);
        assert_eq!(icp_bal, 50_000_000);
        assert_eq!(vusd_bal, 0); // vUSD should not be in Playground
    } else {
        panic!("Failed to get account state for Playground: {state_play:?}");
    }
}
