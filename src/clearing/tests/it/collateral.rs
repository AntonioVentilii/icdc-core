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
use shared::types::{
    evm::NativeEvmAsset, Asset, BalanceDomain, CollateralAssetConfig, CollateralAssetInfo,
};

use crate::utils::{
    assertions::{assert_decimal_eq, assert_ok_value},
    constants::{CKUSDC_LEDGER, ICP_LEDGER},
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait,
    PicCanisterTrait,
};

#[test]
fn get_collateral_assets_empty() {
    let env = TestSetup::default();
    let user = test_user(50);

    let assets: Vec<CollateralAssetInfo> = assert_ok_value(
        env.clearing
            .query::<Vec<CollateralAssetInfo>, _>(user, "get_collateral_assets", ()),
    );

    assert_eq!(
        assets.len(),
        2,
        "ICP and ckUSDC should be registered by default"
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

    assert_eq!(assets.len(), 3);
    assert!(assets.iter().any(|a| a.config.asset_id == "ETH"));
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

    assert_eq!(assets.len(), 3);

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

    let icp_ledger = Principal::from_text(ICP_LEDGER).unwrap();

    // ICP is already registered by default, but we might want to update metrics if needed?
    // Actually, TestSetup::default() already sets metrics. So we can just remove this.

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

    let ckusdc_ledger = Principal::from_text(CKUSDC_LEDGER).unwrap();
    let ckusdc_approve_bytes = env
        .pic
        .update_call(
            ckusdc_ledger,
            user,
            "icrc2_approve",
            encode_one(approve_params).unwrap(),
        )
        .expect("ckUSDC Approval call failed");
    let ckusdc_approve_res: Result<Nat, ApproveError> =
        decode_one(&ckusdc_approve_bytes).expect("Failed to decode ckUSDC approval");
    ckusdc_approve_res.expect("ckUSDC Approval error");
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

    // 4. Deposit ckUSDC into Settlement
    env.deposit_collateral(
        user,
        "ckUSDC",
        Nat::from(10_000_000_u64), // 10 ckUSDC (decimal 6)
        Some(BalanceDomain::Settlement),
    );
    env.pic.tick();

    // 5. Verify Isolation
    let assets: Vec<CollateralAssetInfo> = assert_ok_value(
        env.clearing
            .query::<Vec<CollateralAssetInfo>, _>(user, "get_collateral_assets", ()),
    );
    assert!(!assets.is_empty());

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
        let ckusdc_bal = resp
            .state
            .get_balance(BalanceDomain::Settlement, &"ckUSDC".to_owned());
        assert_eq!(icp_bal, 100_000_000);
        assert_eq!(ckusdc_bal, 10_000_000);
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
        let ckusdc_bal = resp
            .state
            .get_balance(BalanceDomain::Playground, &"ckUSDC".to_owned());
        assert_eq!(icp_bal, 50_000_000);
        assert_eq!(ckusdc_bal, 0); // ckUSDC should not be in Playground
    } else {
        panic!("Failed to get account state for Playground: {state_play:?}");
    }
}
