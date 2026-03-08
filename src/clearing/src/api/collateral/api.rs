use candid::Nat;
use ic_cdk_macros::update;
use shared::types::{asset::errors::AssetError, AssetId};

use super::{
    errors::{DepositCollateralError, WithdrawCollateralError},
    params::{DepositCollateralParams, WithdrawCollateralParams},
    results::{DepositCollateralResult, WithdrawCollateralResult},
};
use crate::{
    assets::{
        asset::{
            handler::get_handler,
            params::{AssetTransferFromParams, AssetTransferParams},
        },
        types::AssetAmount,
    },
    guards::caller_is_not_anonymous,
    memory::{ACCOUNT_STATES, COLLATERAL_ASSETS, DEPOSIT_PLANS, WITHDRAWAL_PLANS},
    types::{
        account::AssetAccount,
        margin::AccountState,
        plans::{DepositPlan, DepositPlanParams, PlanStatus, WithdrawalPlan, WithdrawalPlanParams},
        user::User,
    },
};

/// Deposits collateral into the user's account state.
///
/// This is a multi-phase operation:
/// 1. Building a [`DepositPlan`] for idempotency.
/// 2. Executing the asynchronous ledger transfer (`transfer_from`).
/// 3. Finalising the internal collateral balances.
#[update(guard = "caller_is_not_anonymous")]
pub async fn deposit_collateral(params: DepositCollateralParams) -> DepositCollateralResult {
    let result: Result<(), DepositCollateralError> = (async {
        let user: User = ic_cdk::caller().into();

        let DepositCollateralParams {
            amount,
            asset_id,
            deposit_id,
        } = params;

        // Verify the asset is supported and enabled
        let config = COLLATERAL_ASSETS.with(|assets| {
            assets
                .borrow()
                .get(&asset_id)
                .cloned()
                .ok_or(DepositCollateralError::Asset(AssetError::UnsupportedAsset))
        })?;

        if !config.is_enabled {
            return Err(DepositCollateralError::Asset(AssetError::UnsupportedAsset));
        }

        // ---------- Phase A: Build plan (no awaits) ----------
        let mut plan = DepositPlan::get_or_create(DepositPlanParams {
            deposit_id: deposit_id.clone(),
            user,
            asset_id: asset_id.clone(),
            amount: amount.clone(),
        });

        if plan.status == PlanStatus::Finalised {
            return Ok(());
        }

        let key = (user, deposit_id.clone());

        // ---------- Phase B: Execute transfer (async, resumable) ----------
        if plan.receipt.is_none() {
            plan.status = PlanStatus::Executing;
            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));

            let handler = get_handler(&config.asset).map_err(DepositCollateralError::Asset)?;

            let amount_u128: u128 = amount
                .0
                .clone()
                .try_into()
                .map_err(|_| DepositCollateralError::MathOverflow)?;

            let res = handler
                .transfer_from(AssetTransferFromParams {
                    asset: &config.asset,
                    spender: AssetAccount::CanisterMain,
                    from: AssetAccount::external_principal(user.principal()),
                    to: AssetAccount::UserClearing(user),
                    amount: AssetAmount::Fixed(amount_u128),
                    created_at_time_ns: plan.idempotency_ns.to_created_at_time_ns(),
                })
                .await;

            match res {
                Ok(block_index) => {
                    plan.receipt = Some(Nat::from(block_index).into());
                }
                Err(e) => {
                    DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));
                    return Err(DepositCollateralError::Asset(e));
                }
            }

            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));
        }

        // ---------- Phase C: Finalise (no awaits, idempotent) ----------
        if plan.receipt.is_some() && plan.status != PlanStatus::Finalised {
            let amount_u128: u128 = amount
                .0
                .clone()
                .try_into()
                .map_err(|_| DepositCollateralError::MathOverflow)?;

            ACCOUNT_STATES.with(|accounts| {
                let mut accounts = accounts.borrow_mut();
                let state = accounts
                    .entry(user)
                    .or_insert_with(|| AccountState::new(user));
                let current = state
                    .collateral_balances
                    .get(&asset_id)
                    .copied()
                    .unwrap_or(0);
                state
                    .collateral_balances
                    .insert(asset_id.clone(), current + amount_u128);
            });

            plan.status = PlanStatus::Finalised;
            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan));
        }

        Ok(())
    })
    .await;

    result.into()
}

/// Withdraws collateral from the user's account state to an external address.
///
/// This implements the "Deterministic Withdrawal Policy":
/// 1. Calculate current account equity in USD.
/// 2. Verify equity >= reserved_margin_usd (risk check).
/// 3. If ok, proceed with asynchronous ledger transfer.
#[update(guard = "caller_is_not_anonymous")]
pub async fn withdraw_collateral(params: WithdrawCollateralParams) -> WithdrawCollateralResult {
    let result: Result<(), WithdrawCollateralError> = (async {
        let user: User = ic_cdk::caller().into();

        let WithdrawCollateralParams {
            amount,
            asset_id,
            withdrawal_id,
        } = params;

        let config = COLLATERAL_ASSETS.with(|assets| {
            assets
                .borrow()
                .get(&asset_id)
                .cloned()
                .ok_or(WithdrawCollateralError::Asset(AssetError::UnsupportedAsset))
        })?;

        // ---------- Phase A: Build plan (durable, no awaits) ----------
        let mut plan = WithdrawalPlan::get_or_create(WithdrawalPlanParams {
            withdrawal_id: withdrawal_id.clone(),
            user,
            asset_id: asset_id.clone(),
            amount: amount.clone(),
            to_account: (user.principal(), None),
        });

        if plan.status == PlanStatus::Finalised {
            return Ok(());
        }

        let key = (user, withdrawal_id.clone());

        // ---------- Phase B: Risk Check and Internal Debit ----------
        if plan.reserved_amount.is_none() {
            let amount_u128: u128 = amount
                .0
                .clone()
                .try_into()
                .map_err(|_| WithdrawCollateralError::MathOverflow)?;

            let (equity_usd, reserved_margin_usd) = ACCOUNT_STATES.with(|accounts| {
                let accounts = accounts.borrow();
                let state = accounts.get(&user).ok_or(
                    WithdrawCollateralError::InsufficientExcessMargin {
                        available: 0,
                        requested: amount_u128, // This is simplified for the error type
                    },
                )?;

                // Need to compute equity. Since we need CollateralAssetConfig map, let's get it.
                let configs = COLLATERAL_ASSETS.with(|c| c.borrow().clone());
                let equity = state.calculate_equity_usd(&configs);
                Ok::<(u128, u128), WithdrawCollateralError>((equity, state.reserved_margin_usd))
            })?;

            // Risk check: equity - withdrawal_value_usd must be >= reserved_margin_usd
            let withdrawal_value_usd = (amount_u128 as f64 * config.price_usd.to_f64()) as u128;

            if equity_usd < reserved_margin_usd
                || equity_usd - reserved_margin_usd < withdrawal_value_usd
            {
                return Err(WithdrawCollateralError::InsufficientExcessMargin {
                    available: equity_usd.saturating_sub(reserved_margin_usd),
                    requested: withdrawal_value_usd,
                });
            }

            // Debit internal balance
            ACCOUNT_STATES.with(|accounts| {
                let mut accounts = accounts.borrow_mut();
                if let Some(state) = accounts.get_mut(&user) {
                    let current = state
                        .collateral_balances
                        .get(&asset_id)
                        .copied()
                        .unwrap_or(0);
                    state
                        .collateral_balances
                        .insert(asset_id.clone(), current.saturating_sub(amount_u128));
                }
            });

            plan.reserved_amount = Some(amount_u128);
            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));
        }

        // ---------- Phase C: Execute ledger transfer ----------
        if plan.receipt.is_none() {
            plan.status = PlanStatus::Executing;
            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));

            let handler = get_handler(&config.asset).map_err(WithdrawCollateralError::Asset)?;

            let amount_u128: u128 = plan.reserved_amount.unwrap();

            let res = handler
                .transfer(AssetTransferParams {
                    asset: &config.asset,
                    from: AssetAccount::UserClearing(user),
                    to: AssetAccount::external_icrc(plan.to_account.0, plan.to_account.1),
                    amount: AssetAmount::Fixed(amount_u128),
                    created_at_time_ns: plan.idempotency_ns.to_created_at_time_ns(),
                })
                .await;

            match res {
                Ok(block_index) => {
                    plan.receipt = Some(Nat::from(block_index).into());
                }
                Err(e) => {
                    // Compensation: refund internal balance on failure
                    if let Some(reserved) = plan.reserved_amount.take() {
                        ACCOUNT_STATES.with(|accounts| {
                            let mut accounts = accounts.borrow_mut();
                            if let Some(state) = accounts.get_mut(&user) {
                                let current = state
                                    .collateral_balances
                                    .get(&asset_id)
                                    .copied()
                                    .unwrap_or(0);
                                state
                                    .collateral_balances
                                    .insert(asset_id.clone(), current + reserved);
                            }
                        });
                    }
                    WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));
                    return Err(WithdrawCollateralError::Asset(e));
                }
            }
            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));
        }

        // ---------- Phase D: Finalise ----------
        if plan.receipt.is_some() && plan.status != PlanStatus::Finalised {
            plan.status = PlanStatus::Finalised;
            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan));
        }

        Ok(())
    })
    .await;

    result.into()
}
