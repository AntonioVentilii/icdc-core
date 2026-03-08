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
    memory::{
        COLLATERAL_ASSETS, CONFIG, FUND_WITHDRAWAL_PLANS, INSURANCE_FUND, REGISTRY_CANISTER,
        TREASURY,
    },
    types::{
        account::AssetAccount,
        payment::PaymentReceipt,
        plans::{FundWithdrawalPlan, FundWithdrawalPlanParams, PlanStatus},
        state::Config,
    },
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

        // ---------- Phase A: Build or resume plan ----------
        let mut plan = FundWithdrawalPlan::get_or_create(FundWithdrawalPlanParams {
            request_id: request_id.clone(),
            fund_type,
            asset_id: asset_id.clone(),
            amount,
            to,
        });

        if plan.status == PlanStatus::Finalised {
            return plan
                .receipt
                .map(|r| r.block_index())
                .ok_or(AdminError::TransferFailed("No receipt found".to_string()));
        }

        // ---------- Phase B: Deduct fund balance (internal) ----------
        if plan.status == PlanStatus::Planned {
            deduct_fund_balance_impl(&asset_id, amount, fund_type)?;

            plan.status = PlanStatus::Executing;
            FUND_WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(request_id.clone(), plan.clone()));
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
                    plan.receipt = Some(PaymentReceipt::IcrcBlockIndex(Nat::from(block)));
                    plan.status = PlanStatus::Finalised;
                }
                Err(e) => {
                    // Revert deduction (Atomic Rollback)
                    rollback_fund_deduction_impl(&asset_id, amount, fund_type);

                    // Marks as planned so it can be retried (or kept as executing if we want to be
                    // stricter) For fund withdrawals, we can allow retries if
                    // the transfer failed to even start.
                    plan.status = PlanStatus::Planned;
                    FUND_WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(request_id, plan));

                    return Err(AdminError::TransferFailed(format!("{:?}", e)));
                }
            }

            FUND_WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(request_id, plan.clone()));
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

pub(crate) fn deduct_fund_balance_impl(
    asset_id: &shared::types::AssetId,
    amount: u128,
    fund_type: FundType,
) -> Result<(), AdminError> {
    let store = match fund_type {
        FundType::Insurance => &INSURANCE_FUND,
        FundType::Treasury => &TREASURY,
    };
    store.with(|f| {
        let mut f = f.borrow_mut();
        let current = f.get(asset_id).cloned().unwrap_or(0);
        if current < amount {
            return Err(AdminError::InsufficientFunds);
        }
        f.insert(asset_id.clone(), current - amount);
        Ok(())
    })
}

pub(crate) fn rollback_fund_deduction_impl(
    asset_id: &shared::types::AssetId,
    amount: u128,
    fund_type: FundType,
) {
    let store = match fund_type {
        FundType::Insurance => &INSURANCE_FUND,
        FundType::Treasury => &TREASURY,
    };
    store.with(|f| {
        let mut f = f.borrow_mut();
        let current = f.get(asset_id).cloned().unwrap_or(0);
        f.insert(asset_id.clone(), current + amount);
    });
}

#[cfg(test)]
mod tests {
    use shared::types::AssetId;

    use super::*;

    #[test]
    fn test_withdraw_fund_resilience_on_transfer_failure() {
        let asset_id = AssetId::from("vUSD".to_string());
        let amount = 1_000_000; // $1

        // Initialize insurance fund with $10
        INSURANCE_FUND.with(|f| {
            let mut f = f.borrow_mut();
            f.clear();
            f.insert(asset_id.clone(), 10_000_000);
        });

        // Step 1: Deduct
        let deduct_res = deduct_fund_balance_impl(&asset_id, amount, FundType::Insurance);
        assert!(deduct_res.is_ok());

        INSURANCE_FUND.with(|f| {
            assert_eq!(f.borrow().get(&asset_id).cloned().unwrap(), 9_000_000);
        });

        // Step 2: Rollback (simulating transfer failure)
        rollback_fund_deduction_impl(&asset_id, amount, FundType::Insurance);

        // Internal balance should be restored
        INSURANCE_FUND.with(|f| {
            assert_eq!(f.borrow().get(&asset_id).cloned().unwrap(), 10_000_000);
        });
    }

    #[test]
    fn test_withdraw_fund_insufficient_funds() {
        let asset_id = AssetId::from("vUSD".to_string());
        let amount = 100_000_000; // $100

        // Initialize treasury with $10
        TREASURY.with(|f| {
            let mut f = f.borrow_mut();
            f.clear();
            f.insert(asset_id.clone(), 10_000_000);
        });

        // Try to deduct $100
        let deduct_res = deduct_fund_balance_impl(&asset_id, amount, FundType::Treasury);
        assert!(matches!(deduct_res, Err(AdminError::InsufficientFunds)));

        // Internal balance should remain untouched
        TREASURY.with(|f| {
            assert_eq!(f.borrow().get(&asset_id).cloned().unwrap(), 10_000_000);
        });
    }
}
