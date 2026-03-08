use candid::{Nat, Principal};
use ic_cdk_macros::{query, update};
use shared::types::CollateralAssetConfig;

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
    types::{account::AssetAccount, state::Config},
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
pub fn update_config(config: Config) {
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
            request_id,
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

        // ---------- Phase A: Build or resume plan ----------
        let mut plan = crate::types::plans::FundWithdrawalPlan::get_or_create(
            crate::types::plans::FundWithdrawalPlanParams {
                request_id: request_id.clone(),
                fund_type,
                asset_id: asset_id.clone(),
                amount,
                to,
            },
        );

        if plan.status == crate::types::plans::PlanStatus::Finalised {
            return plan
                .receipt
                .map(|r| r.block_index())
                .ok_or(AdminError::TransferFailed("No receipt found".to_string()));
        }

        // ---------- Phase B: Deduct fund balance (internal) ----------
        if plan.status == crate::types::plans::PlanStatus::Planned {
            store.with(|f| {
                let mut f = f.borrow_mut();
                let current = f.get(&asset_id).cloned().unwrap_or(0);
                if current < amount {
                    return Err(AdminError::InsufficientFunds);
                }
                f.insert(asset_id.clone(), current - amount);
                Ok(())
            })?;

            plan.status = crate::types::plans::PlanStatus::Executing;
            crate::memory::FUND_WITHDRAWAL_PLANS
                .with(|m| m.borrow_mut().insert(request_id.clone(), plan.clone()));
        }

        // ---------- Phase C: Execute ledger transfer ----------
        if plan.receipt.is_none() {
            let handler =
                get_handler(&asset).map_err(|e| AdminError::TransferFailed(format!("{:?}", e)))?;

            let transfer_res = handler
                .transfer(AssetTransferParams {
                    asset: &asset,
                    from: AssetAccount::CanisterMain,
                    to: AssetAccount::external_principal(to),
                    amount: AssetAmount::Fixed(amount),
                    created_at_time_ns: plan.idempotency_ns.to_created_at_time_ns(),
                })
                .await;

            match transfer_res {
                Ok(block) => {
                    plan.receipt = Some(crate::types::payment::PaymentReceipt::IcrcBlockIndex(
                        Nat::from(block),
                    ));
                    plan.status = crate::types::plans::PlanStatus::Finalised;
                }
                Err(e) => {
                    // Revert deduction
                    store.with(|f| {
                        let mut f = f.borrow_mut();
                        let current = f.get(&asset_id).cloned().unwrap_or(0);
                        f.insert(asset_id, current + amount);
                    });

                    // Marks as planned so it can be retried (or kept as executing if we want to be
                    // stricter) For fund withdrawals, we can allow retries if
                    // the transfer failed to even start.
                    plan.status = crate::types::plans::PlanStatus::Planned;
                    crate::memory::FUND_WITHDRAWAL_PLANS
                        .with(|m| m.borrow_mut().insert(request_id, plan));

                    return Err(AdminError::TransferFailed(format!("{:?}", e)));
                }
            }

            crate::memory::FUND_WITHDRAWAL_PLANS
                .with(|m| m.borrow_mut().insert(request_id, plan.clone()));
        }

        Ok(plan.receipt.unwrap().block_index())
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

/// Debug: returns the principal of the registry canister.
#[query(guard = "caller_is_controller")]
pub fn debug_get_registry_canister() -> Principal {
    REGISTRY_CANISTER.with(|r| *r.borrow())
}
