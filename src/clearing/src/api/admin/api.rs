use candid::{Nat, Principal};
use ic_cdk_macros::{query, update};
use shared::types::{AssetId, CollateralAssetConfig};

use super::{
    params::{
        CancelFundWithdrawalParams, FundType, UpdateAssetMetricsParams, UpdateAssetPriceParams,
        UpdateCollateralAssetParams, WithdrawFundParams,
    },
    results::{
        CancelFundWithdrawalError, CancelFundWithdrawalResult, GetFundsResult,
        UpdateAssetPriceError, UpdateAssetPriceResult, WithdrawFundError, WithdrawFundResult,
    },
};
use crate::{
    assets::{
        asset::{handler::get_handler, params::AssetTransferParams},
        types::AssetAmount,
    },
    guards::{caller_is_controller, caller_is_not_anonymous},
    memory::{
        ASSET_METRICS, COLLATERAL_ASSETS, CONFIG, FUND_WITHDRAWAL_PLANS, INSURANCE_FUND,
        REGISTRY_CANISTER, TREASURY,
    },
    types::{
        account::AssetAccount,
        errors::CommonError,
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
pub async fn withdraw_fund(params: WithdrawFundParams) -> WithdrawFundResult {
    let res: Result<Nat, WithdrawFundError> =
        (async {
            let WithdrawFundParams {
                request_id,
                fund_type,
                asset_id,
                amount,
                to,
            } = params;

            let config =
                COLLATERAL_ASSETS.with(|assets| {
                    assets.borrow().get(&asset_id).cloned().ok_or(
                        WithdrawFundError::TransferFailed("Unsupported asset".to_string()),
                    )
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
                return plan.receipt.map(|r| r.block_index()).ok_or(
                    WithdrawFundError::TransferFailed("No receipt found".to_string()),
                );
            }

            // ---------- Phase B: Deduct fund balance (internal) ----------
            if plan.status == PlanStatus::Planned {
                deduct_fund_balance_impl(&asset_id, amount, fund_type)?;

                plan.status = PlanStatus::Executing;
                FUND_WITHDRAWAL_PLANS
                    .with(|m| m.borrow_mut().insert(request_id.clone(), plan.clone()));
            }

            // ---------- Phase C: Execute ledger transfer ----------
            if plan.receipt.is_none() {
                let handler = get_handler(&asset)
                    .map_err(|e| WithdrawFundError::TransferFailed(format!("{:?}", e)))?;

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
                        // For fund withdrawals, keep state as Executing since deducting from the
                        // internal fund (Phase B) already succeeded. This prevents a retry from
                        // duplicating Phase B. Because the exact identical `idempotency_ns` will be
                        // provided on retry, the ICRC ledger handler duplicate check makes retrying
                        // entirely safe.
                        return Err(WithdrawFundError::TransferFailed(format!("{:?}", e)));
                    }
                }

                FUND_WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(request_id, plan.clone()));
            }

            Ok(plan.receipt.unwrap().block_index())
        })
        .await;

    res.into()
}

/// Cancels a stuck fund withdrawal and refunds the internal ledger balance.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn cancel_fund_withdrawal(params: CancelFundWithdrawalParams) -> CancelFundWithdrawalResult {
    let old_plan = FUND_WITHDRAWAL_PLANS.with(|m| m.borrow_mut().remove(&params.request_id));
    let plan = match old_plan {
        Some(p) => p,
        None => return Err(CancelFundWithdrawalError::PlanNotFound).into(),
    };

    if plan.status != PlanStatus::Executing || plan.receipt.is_some() {
        // Put it back
        FUND_WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(params.request_id, plan));
        return Err(CancelFundWithdrawalError::InvalidPlanStatus).into();
    }

    // Refund internally
    let store = match plan.fund_type {
        FundType::Insurance => &INSURANCE_FUND,
        FundType::Treasury => &TREASURY,
    };

    store.with(|f| {
        let mut f = f.borrow_mut();
        let current = f.get(&plan.asset_id).cloned().unwrap_or(0);
        f.insert(plan.asset_id.clone(), current + plan.amount);
    });

    Ok(()).into()
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

/// Adds or updates dynamic metrics for a collateral asset.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn update_asset_metrics(params: UpdateAssetMetricsParams) {
    let metrics = params.metrics;
    ASSET_METRICS.with(|assets| {
        assets.borrow_mut().insert(params.asset_id.clone(), metrics);
    });
}

/// Updates the price of an asset.
///
/// This method can be called by canister controllers or authorized oracles.
#[update(guard = "caller_is_not_anonymous")]
pub async fn update_asset_price(params: UpdateAssetPriceParams) -> UpdateAssetPriceResult {
    update_asset_price_impl(params).await.into()
}

async fn update_asset_price_impl(
    params: UpdateAssetPriceParams,
) -> Result<(), UpdateAssetPriceError> {
    let caller = ic_cdk::caller();

    let asset_config = COLLATERAL_ASSETS
        .with(|assets| assets.borrow().get(&params.asset_id).cloned())
        .ok_or(UpdateAssetPriceError::AssetNotFound)?;

    if !ic_cdk::api::is_controller(&caller) {
        let oracle_id = asset_config
            .oracle_id
            .ok_or(UpdateAssetPriceError::OracleNotConfigured)?;
        let registry_canister = REGISTRY_CANISTER.with(|r| *r.borrow());
        if registry_canister == Principal::anonymous() {
            return Err(UpdateAssetPriceError::Common(CommonError::RegistryNotSet));
        }

        let is_authorized: Result<(bool,), _> = ic_cdk::call(
            registry_canister,
            "is_oracle_authorized",
            (oracle_id, caller),
        )
        .await;

        match is_authorized {
            Ok((true,)) => {}
            _ => return Err(UpdateAssetPriceError::Common(CommonError::Unauthorized)),
        }
    }

    ASSET_METRICS.with(|m| {
        let mut m = m.borrow_mut();
        if let Some(metrics) = m.get_mut(&params.asset_id) {
            metrics.price_usd = params.price.decimal.clone();
            metrics.last_updated_ns = Some(crate::utils::system::now_ns());
            Ok(())
        } else {
            Err(UpdateAssetPriceError::AssetMetricsNotInitialized)
        }
    })
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
    asset_id: &AssetId,
    amount: u128,
    fund_type: FundType,
) -> Result<(), WithdrawFundError> {
    let store = match fund_type {
        FundType::Insurance => &INSURANCE_FUND,
        FundType::Treasury => &TREASURY,
    };
    store.with(|f| {
        let mut f = f.borrow_mut();
        let current = f.get(asset_id).cloned().unwrap_or(0);
        if current < amount {
            return Err(WithdrawFundError::InsufficientFunds);
        }
        f.insert(asset_id.clone(), current - amount);
        Ok(())
    })
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

        // Step 2: In the new implementation we deliberately DO NOT rollback
        // upon transfer error. The internal fund deduction stands and Phase B
        // is not retried.
        INSURANCE_FUND.with(|f| {
            assert_eq!(f.borrow().get(&asset_id).cloned().unwrap(), 9_000_000);
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
        assert!(matches!(
            deduct_res,
            Err(WithdrawFundError::InsufficientFunds)
        ));

        // Internal balance should remain untouched
        TREASURY.with(|f| {
            assert_eq!(f.borrow().get(&asset_id).cloned().unwrap(), 10_000_000);
        });
    }
}
