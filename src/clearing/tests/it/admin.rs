use candid::Principal;
use clearing::{
    api::admin::{params::UpdateCollateralAssetParams, results::GetFundsResult},
    types::state::Config,
};
use shared::types::{evm::NativeEvmAsset, Asset, CollateralAssetConfig, Series};

use crate::utils::{
    assertions::{assert_ok_value, assert_unauthorized},
    mock::TEST_REGISTRY_CANISTER,
    test_environment::TestSetup,
    trade_helper::TradeHelperTrait,
    PicCanisterTrait,
};

#[test]
fn set_registry_canister() {
    let env = TestSetup::default();

    // The registry is already set in setup_env(), let's change it to something else
    let new_registry = Principal::from_text(TEST_REGISTRY_CANISTER).unwrap();
    assert_ne!(new_registry, env.registry.canister_id());

    // Update the registry principal as controller
    env.clearing
        .update::<(), _>(env.controller, "set_registry_canister", (new_registry,))
        .expect("set_registry_canister failed");

    // Verify the change was applied correctly
    let registry_principal: Principal = assert_ok_value(env.clearing.query::<Principal, _>(
        env.controller,
        "get_registry_canister",
        (),
    ));

    assert_eq!(registry_principal, new_registry);
}

#[test]
fn set_registry_canister_rejects_non_controller() {
    let env = TestSetup::default();

    let new_registry = Principal::from_text(TEST_REGISTRY_CANISTER).unwrap();

    let res = env
        .clearing
        .update::<(), _>(env.user, "set_registry_canister", (new_registry,));

    assert_unauthorized(&res);
}

#[test]
fn update_config() {
    let env = TestSetup::default();

    let initial_config: Config = assert_ok_value(env.clearing.query(env.controller, "config", ()));

    let new_config = Config {
        insurance_fund_fee_ratio: initial_config.insurance_fund_fee_ratio + 1,
        protocol_fee_ratio: initial_config.protocol_fee_ratio + 1,
        evm_rpc: Principal::anonymous(),
        signer_canister: Principal::anonymous(),
        internal_ledger: initial_config.internal_ledger.clone(),
        version: 0,
    };

    env.clearing
        .update::<(), _>(env.controller, "update_config", (new_config.clone(),))
        .expect("update_config failed");

    let updated_config: Config = assert_ok_value(env.clearing.query(env.controller, "config", ()));

    assert_eq!(
        updated_config.insurance_fund_fee_ratio,
        initial_config.insurance_fund_fee_ratio + 1
    );
    assert_eq!(
        updated_config.protocol_fee_ratio,
        initial_config.protocol_fee_ratio + 1
    );
}

#[test]
fn update_config_rejects_non_controller() {
    let env = TestSetup::default();

    let config = Config {
        insurance_fund_fee_ratio: 20,
        protocol_fee_ratio: 10,
        evm_rpc: Principal::anonymous(),
        signer_canister: Principal::anonymous(),
        internal_ledger: CollateralAssetConfig {
            asset_id: "vUSD".to_owned(),
            asset: Asset::Icrc(Principal::anonymous()),
            symbol: "vUSD".to_owned(),
            decimals: 8,
            is_enabled: true,
            oracle_id: None,
        },
        version: 0,
    };

    let random_user = Principal::from_slice(&[42, 1, 1]);
    let result = env
        .clearing
        .update::<(), _>(random_user, "update_config", (config,));

    assert_unauthorized(&result);
}

#[test]
fn update_collateral_asset() {
    let env = TestSetup::default();

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
            oracle_id: Some("eth_oracle".to_owned()),
        },
    };

    assert_ok_value(env.clearing.update::<(), _>(
        env.controller,
        "update_collateral_asset",
        (params,),
    ));

    let assets: Vec<CollateralAssetConfig> =
        assert_ok_value(env.clearing.query::<Vec<CollateralAssetConfig>, _>(
            env.controller,
            "list_collateral_assets",
            (),
        ));

    assert_eq!(assets.len(), 3);
    assert_eq!(assets[0].asset_id, "ETH");
    assert_eq!(assets[0].symbol, "ETH");
    assert!(assets[0].is_enabled);
}

#[test]
fn update_asset_metrics() {
    let env = TestSetup::default();

    env.setup_evm_asset("ETH", "ETH", 18, 3_000_000_000, 6, 500);
}

#[test]
fn get_funds_empty() {
    let env = TestSetup::default();

    let funds: GetFundsResult = assert_ok_value(env.clearing.query::<GetFundsResult, _>(
        env.controller,
        "get_funds",
        (),
    ));

    assert!(funds.insurance_fund.is_empty());
    assert!(funds.treasury.is_empty());
}

#[test]
fn list_series_empty() {
    let env = TestSetup::default();

    let series: Vec<Series> = assert_ok_value(env.clearing.query::<Vec<Series>, _>(
        env.controller,
        "list_series",
        (),
    ));

    assert!(series.is_empty());
}
