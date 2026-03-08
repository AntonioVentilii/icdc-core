use candid::{Nat, Principal};
use ic_cdk_macros::{query, update};
use shared::types::{AssetId, CollateralAssetConfig};

use super::{
    params::{FundType, UpdateCollateralAssetParams, WithdrawFundParams},
    results::{AdminError, AdminResult, GetFundsResult},
};
use crate::{
    assets::{
        asset::{handler::get_handler, params::AssetTransferParams},
        types::AssetAmount,
    },
    guards::caller_is_controller,
    memory::{COLLATERAL_ASSETS, CONFIG, INSURANCE_FUND, REGISTRY_CANISTER, TREASURY},
    types::{account::AssetAccount, state::ClearingConfig},
};

/// Sets the principal of the Series Registry canister.
///
/// This principal is used to discover and validate derivative series.
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn set_registry_canister(registry: Principal) {
    REGISTRY_CANISTER.with(|r| {
        *r.borrow_mut() = registry;
    });
}

/// Updates the global configuration for the Clearing canister.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn update_config(config: ClearingConfig) {
    CONFIG.with(|c| {
        *c.borrow_mut() = config;
    });
}

/// Returns the current balances of the Insurance Fund and Treasury.
///
/// This method is gated to canister controllers.
#[query(guard = "caller_is_controller")]
pub fn get_funds() -> GetFundsResult {
    let insurance_fund = INSURANCE_FUND.with(|f| f.borrow().clone());
    let treasury = TREASURY.with(|f| f.borrow().clone());

    GetFundsResult {
        insurance_fund,
        treasury,
    }
}

/// Withdraws assets from the Insurance Fund or Treasury to an external wallet.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub async fn withdraw_fund(params: WithdrawFundParams) -> AdminResult<Nat> {
    let res: Result<Nat, AdminError> = (async {
        let WithdrawFundParams {
            fund_type,
            asset_id,
            amount,
            to,
        } = params;

        let config = COLLATERAL_ASSETS.with(|assets| {
            assets
                .borrow()
                .get(&asset_id)
                .cloned()
                .ok_or(AdminError::TransferFailed("Unsupported asset".to_string()))
        })?;

        let asset = config.asset;

        let store = match fund_type {
            FundType::Insurance => &INSURANCE_FUND,
            FundType::Treasury => &TREASURY,
        };

        // 1. Check and deduct balance
        store.with(|f| {
            let mut f = f.borrow_mut();
            let current = f.get(&asset_id).cloned().unwrap_or(0);
            if current < amount {
                return Err(AdminError::InsufficientFunds);
            }
            f.insert(asset_id.clone(), current - amount);
            Ok(())
        })?;

        // 2. Perform ledger transfer
        let handler = match get_handler(&asset) {
            Ok(h) => h,
            Err(e) => {
                // Revert deduction
                store.with(|f| {
                    let mut f = f.borrow_mut();
                    let current = f.get(&asset_id).cloned().unwrap_or(0);
                    f.insert(asset_id.clone(), current + amount);
                });
                return Err(AdminError::TransferFailed(format!("{:?}", e)));
            }
        };

        let transfer_res = handler
            .transfer(AssetTransferParams {
                asset: &asset,
                from: AssetAccount::CanisterMain,
                to: AssetAccount::external_principal(to),
                amount: AssetAmount::Fixed(amount),
                created_at_time_ns: Some(ic_cdk::api::time()),
            })
            .await;

        match transfer_res {
            Ok(block) => Ok(Nat::from(block)),
            Err(e) => {
                // Revert deduction
                store.with(|f| {
                    let mut f = f.borrow_mut();
                    let current = f.get(&asset_id).cloned().unwrap_or(0);
                    f.insert(asset_id, current + amount);
                });
                Err(AdminError::TransferFailed(format!("{:?}", e)))
            }
        }
    })
    .await;

    res.into()
}

/// Adds or updates a collateral asset configuration.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn update_collateral_asset(params: UpdateCollateralAssetParams) {
    let config = params.config;
    COLLATERAL_ASSETS.with(|assets| {
        assets.borrow_mut().insert(config.asset_id.clone(), config);
    });
}

/// Returns a list of all supported collateral assets.
///
/// This method is gated to canister controllers.
#[query(guard = "caller_is_controller")]
pub fn list_collateral_assets() -> Vec<CollateralAssetConfig> {
    COLLATERAL_ASSETS.with(|assets| assets.borrow().values().cloned().collect())
}
