use std::collections::BTreeMap;

use ic_cdk_macros::update;

use crate::{
    assets::{
        asset::{
            handler::get_handler,
            params::{AssetTransferFromParams, AssetTransferParams},
        },
        types::AssetAmount,
    },
    guards::caller_is_not_anonymous,
    memory::{DEPOSIT_PLANS, MARGIN_ACCOUNTS, WITHDRAWAL_PLANS},
    types::{
        account::LedgerAccount,
        errors::{DepositCollateralError, LedgerError, WithdrawCollateralError},
        margin::MarginAccount,
        params::{DepositCollateralParams, WithdrawCollateralParams},
        plans::{DepositPlan, DepositPlanParams, PlanStatus, WithdrawalPlan, WithdrawalPlanParams},
        results::{DepositCollateralResult, WithdrawCollateralResult},
        user::User,
    },
    utils::asset::is_supported_asset,
};

/// Deposits collateral into the user's margin account.
///
/// This is a multi-phase operation:
/// 1. Building a [`DepositPlan`] for idempotency.
/// 2. Executing the asynchronous ledger transfer (`transfer_from`).
/// 3. Finalising the internal margin account balances.
///
/// # Arguments
/// * `params` - The deposit details including amount, asset, and a unique deposit ID.
///
/// # Returns
/// * [`DepositCollateralResult::Ok`] if the deposit was successfully planned or executed.
/// * [`DepositCollateralResult::Err`] if the asset is unsupported or a transfer error occurs.
#[update(guard = "caller_is_not_anonymous")]
pub async fn deposit_collateral(params: DepositCollateralParams) -> DepositCollateralResult {
    let result: Result<(), DepositCollateralError> = (async {
        let user: User = ic_cdk::caller().into();

        let DepositCollateralParams {
            amount,
            asset,
            deposit_id,
        } = params;

        if !is_supported_asset(&asset) {
            return Err(DepositCollateralError::Ledger(
                LedgerError::UnsupportedLedger,
            ));
        }

        // ---------- Phase A: Build plan (no awaits) ----------
        let mut plan = DepositPlan::get_or_create(DepositPlanParams {
            deposit_id: deposit_id.clone(),
            user,
            asset: asset.clone(),
            amount: amount.clone(),
        });

        if plan.status == PlanStatus::Finalised {
            return Ok(());
        }

        let key = (user, deposit_id.clone());

        // ---------- Phase B: Execute transfer (async, resumable) ----------
        if plan.receipt.is_none() {
            // Mark executing (durably) before the await.
            plan.status = PlanStatus::Executing;

            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));

            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));

            let handler = get_handler(&asset).map_err(DepositCollateralError::Ledger)?;

            let amount_u128: u128 = amount
                .0
                .clone()
                .try_into()
                .map_err(|_| DepositCollateralError::MathOverflow)?;

            let res = handler
                .transfer_from(AssetTransferFromParams {
                    asset: &asset,
                    spender: LedgerAccount::CanisterMain,
                    from: LedgerAccount::External(user.principal(), None),
                    to: LedgerAccount::UserClearing(user),
                    amount: AssetAmount::Fixed(amount_u128),
                    created_at_time: plan.idempotency.to_created_at_time(),
                })
                .await;

            match res {
                Ok(block_index) => {
                    plan.receipt = Some(candid::Nat::from(block_index).into());
                }
                Err(e) => {
                    // Keep plan persisted so retry resumes safely.
                    DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));
                    return Err(DepositCollateralError::Ledger(e));
                }
            }

            // Persist progress AFTER success/duplicate, before doing anything else.
            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));
        }

        // ---------- Phase C: Finalise (no awaits, idempotent) ----------
        if plan.receipt.is_some() && plan.status != PlanStatus::Finalised {
            let amount_u128: u128 = amount
                .0
                .clone()
                .try_into()
                .map_err(|_| DepositCollateralError::MathOverflow)?;

            MARGIN_ACCOUNTS.with(|accounts| {
                let mut accounts = accounts.borrow_mut();
                let account = accounts.entry(user).or_insert(MarginAccount {
                    user,
                    balances: BTreeMap::new(),
                    required_margin: 0,
                });
                let current = account.get_balance(&asset);
                account.set_balance(asset.clone(), current + amount_u128);
            });

            plan.status = PlanStatus::Finalised;

            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan));
        }

        Ok(())
    })
    .await;

    result.into()
}

/// Withdraws collateral from the user's margin account to an external address.
///
/// This is a multi-phase operation:
/// 1. Building a [`WithdrawalPlan`] for idempotency.
/// 2. Reserving the internal balance to prevent double-spending or risk violations.
/// 3. Executing the asynchronous ledger transfer (`transfer`).
/// 4. Finalising the plan status.
///
/// # Arguments
/// * `params` - The withdrawal details including amount, asset, and a unique withdrawal ID.
///
/// # Returns
/// * [`WithdrawalCollateralResult::Ok`] if the withdrawal was successfully planned or executed.
/// * [`WithdrawalCollateralResult::Err`] if margin is insufficient or a transfer error occurs.
#[update(guard = "caller_is_not_anonymous")]
pub async fn withdraw_collateral(params: WithdrawCollateralParams) -> WithdrawCollateralResult {
    let result: Result<(), WithdrawCollateralError> = (async {
        let user: User = ic_cdk::caller().into();

        let WithdrawCollateralParams {
            amount,
            asset,
            withdrawal_id,
        } = params;

        if !is_supported_asset(&asset) {
            return Err(WithdrawCollateralError::Ledger(
                LedgerError::UnsupportedLedger,
            ));
        }

        // ---------- Phase A: Build plan (durable, no awaits) ----------
        let mut plan = WithdrawalPlan::get_or_create(WithdrawalPlanParams {
            withdrawal_id: withdrawal_id.clone(),
            user,
            asset: asset.clone(),
            amount: amount.clone(),
            to_account: (user.principal(), None),
        });

        if plan.status == PlanStatus::Finalised {
            return Ok(());
        }

        let key = (user, withdrawal_id.clone());

        // ---------- Phase B: Reserve/debit INTERNAL balance BEFORE any await ----------
        // Ensures we never send funds out unless the user is eligible (risk check).
        if plan.reserved_amount.is_none() {
            let amount_u128: u128 = amount
                .0
                .clone()
                .try_into()
                .map_err(|_| WithdrawCollateralError::MathOverflow)?;

            MARGIN_ACCOUNTS.with(|accounts| {
                let mut accounts = accounts.borrow_mut();

                let account = accounts.entry(user).or_insert(MarginAccount {
                    user,
                    balances: BTreeMap::new(),
                    required_margin: 0,
                });

                let current = account.get_balance(&asset);

                if current < amount_u128 {
                    return Err(WithdrawCollateralError::InsufficientExcessMargin {
                        current,
                        requested: amount_u128,
                    });
                }

                account.set_balance(asset.clone(), current - amount_u128);

                Ok(())
            })?;

            plan.reserved_amount = Some(amount_u128);

            // Persist reservation so retries don’t double-debit.
            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));
        }

        // ---------- Phase C: Execute transfer (async, resumable + ledger idempotency) ----------
        if plan.receipt.is_none() {
            // Persist that we’re executing before the await
            plan.status = PlanStatus::Executing;

            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));

            let handler = get_handler(&asset).map_err(WithdrawCollateralError::Ledger)?;

            let amount_u128: u128 = plan
                .amount
                .0
                .clone()
                .try_into()
                .map_err(|_| WithdrawCollateralError::MathOverflow)?;

            let res = handler
                .transfer(AssetTransferParams {
                    asset: &asset,
                    from: LedgerAccount::UserClearing(user),
                    to: LedgerAccount::External(plan.to_account.0, plan.to_account.1),
                    amount: AssetAmount::Fixed(amount_u128),
                    created_at_time: plan.idempotency.to_created_at_time(),
                })
                .await;

            match res {
                Ok(block_index) => {
                    plan.receipt = Some(candid::Nat::from(block_index).into());
                }
                Err(e) => {
                    // Refund reserved balance on failure (compensation).
                    if let Some(reserved) = plan.reserved_amount.take() {
                        MARGIN_ACCOUNTS.with(|accounts| {
                            let mut accounts = accounts.borrow_mut();
                            let account = accounts.entry(user).or_insert(MarginAccount {
                                user,
                                balances: BTreeMap::new(),
                                required_margin: 0,
                            });
                            let current = account.get_balance(&asset);
                            account.set_balance(asset.clone(), current + reserved);
                        });
                    }

                    // Persist updated plan (reservation cleared) so retries behave correctly.
                    WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));

                    return Err(WithdrawCollateralError::Ledger(e));
                }
            }

            // Persist after successful transfer / duplicate
            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));
        }

        // ---------- Phase D: Finalise (no awaits, idempotent) ----------
        if plan.receipt.is_some() && plan.status != PlanStatus::Finalised {
            plan.status = PlanStatus::Finalised;

            // At this point, funds are already debited internally (reserved_amount is
            // “consumed”). Keep it as-is for auditability, or set it to None if you
            // prefer.

            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key, plan));
        }

        Ok(())
    })
    .await;

    result.into()
}
