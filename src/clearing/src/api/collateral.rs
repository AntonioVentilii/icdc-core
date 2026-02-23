use std::collections::BTreeMap;

use ic_cdk_macros::update;
use icrc_ledger_types::{
    icrc1::{
        account::Account,
        transfer::{TransferArg, TransferError},
    },
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};
use shared::types::Asset;

use crate::{
    guards::caller_is_not_anonymous,
    memory::{DEPOSIT_PLANS, MARGIN_ACCOUNTS, WITHDRAWAL_PLANS},
    types::{
        error::ClearingError,
        margin::MarginAccount,
        params::{DepositCollateralParams, WithdrawCollateralParams},
        plan::{DepositPlan, PlanStatus, WithdrawalPlan},
        results::{DepositCollateralResult, WithdrawCollateralResult},
        user::User,
    },
    utils::asset::is_supported_asset,
};

#[update(guard = "caller_is_not_anonymous")]
pub async fn deposit_collateral(params: DepositCollateralParams) -> DepositCollateralResult {
    let result: Result<(), ClearingError> = (async {
        let user: User = ic_cdk::caller().into();

        let DepositCollateralParams {
            amount,
            asset,
            deposit_id,
        } = params;

        if !is_supported_asset(&asset) {
            return Err(ClearingError::UnsupportedLedger);
        }

        let Asset::Icrc(ledger_id) = asset;

        // ---------- Phase A: Build plan (no awaits) ----------
        // If plan exists, we resume (or no-op if finalised).
        let mut plan =
            DepositPlan::get_or_create(deposit_id.clone(), user, asset.clone(), amount.clone());

        // Already done → idempotent success.
        if plan.status == PlanStatus::Finalised {
            return Ok(());
        }

        // ---------- Phase B: Execute transfer (async, resumable) ----------
        if plan.receipt.is_none() {
            // Mark executing (durably) before the await.
            plan.status = PlanStatus::Executing;

            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(deposit_id.clone(), plan.clone()));

            // TODO: is the sender correct? And the receiver?
            let args = TransferFromArgs {
                spender_subaccount: None,
                from: Account {
                    owner: user.principal(),
                    subaccount: None,
                },
                to: plan.to_account,
                amount: amount.clone(),
                fee: None,
                memo: None,
                created_at_time: plan.idempotency.to_created_at_time(),
            };

            let (res,): (Result<candid::Nat, TransferFromError>,) =
                ic_cdk::call(ledger_id, "icrc2_transfer_from", (args,))
                    .await
                    .map_err(|(code, msg)| {
                        ClearingError::TransferFailed(format!("RB: {:?}: {}", code, msg))
                    })?;

            match res {
                Ok(block_index) => {
                    plan.receipt = Some(block_index.into());
                }
                Err(TransferFromError::Duplicate { duplicate_of }) => {
                    // Treat Duplicate as success
                    plan.receipt = Some(duplicate_of.into())
                }
                Err(e) => {
                    // Keep plan persisted so retry resumes safely.
                    DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(deposit_id.clone(), plan.clone()));
                    return Err(ClearingError::TransferFailed(format!("{:?}", e)));
                }
            }

            // Persist progress AFTER success/duplicate, before doing anything else.
            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(deposit_id.clone(), plan.clone()));
        }

        // ---------- Phase C: Finalise (no awaits, idempotent) ----------
        if plan.receipt.is_some() && plan.status != PlanStatus::Finalised {
            let amount_u128: u128 = amount.clone().0.try_into().unwrap_or(0);

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

            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(deposit_id.clone(), plan));
        }

        Ok(())
    })
    .await;

    result.into()
}

#[update(guard = "caller_is_not_anonymous")]
pub async fn withdraw_collateral(params: WithdrawCollateralParams) -> WithdrawCollateralResult {
    let result: Result<(), ClearingError> = (async {
        let user: User = ic_cdk::caller().into();

        let WithdrawCollateralParams {
            amount,
            asset,
            withdrawal_id,
        } = params;

        if !is_supported_asset(&asset) {
            return Err(ClearingError::UnsupportedLedger);
        }

        let Asset::Icrc(ledger_id) = asset;

        let amount_u128: u128 = amount
            .0
            .clone()
            .try_into()
            .map_err(|_| ClearingError::WithdrawCollateralMathOverflow)?;

        // Phase A: Build plan (durable, no awaits)
        let mut plan = WithdrawalPlan::get_or_create(
            withdrawal_id.clone(),
            user,
            asset.clone(),
            amount.clone(),
        );

        if plan.status == PlanStatus::Finalised {
            return Ok(());
        }

        // Phase B: execute transfer (async, resumable + ledger idempotency)
        if plan.receipt.is_none() {
            // Persist that we’re executing before the await
            plan.status = PlanStatus::Executing;

            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(withdrawal_id.clone(), plan.clone()));

            let args = TransferArg {
                from_subaccount: Some(plan.from_subaccount),
                to: plan.to_account,
                amount: plan.amount.clone(),
                fee: None,
                memo: None,
                created_at_time: plan.idempotency.to_created_at_time(),
            };

            let (res,): (Result<candid::Nat, TransferError>,) =
                ic_cdk::call(ledger_id, "icrc1_transfer", (args,))
                    .await
                    .map_err(|(code, msg)| {
                        ClearingError::TransferFailed(format!("RB: {:?}: {}", code, msg))
                    })?;

            match res {
                Ok(block_index) => {
                    plan.receipt = Some(block_index.into());
                }
                Err(TransferError::Duplicate { duplicate_of }) => {
                    plan.receipt = Some(duplicate_of.into());
                }
                Err(e) => {
                    // Persist plan so retries resume and ledger dedupe can work
                    WITHDRAWAL_PLANS
                        .with(|m| m.borrow_mut().insert(withdrawal_id.clone(), plan.clone()));
                    return Err(ClearingError::TransferFailed(format!("{:?}", e)));
                }
            }

            // Persist after successful transfer / duplicate
            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(withdrawal_id.clone(), plan.clone()));
        }

        // Phase C: finalise (no awaits, idempotent)
        if plan.receipt.is_some() && plan.status != PlanStatus::Finalised {
            // IMPORTANT: enforce internal risk/margin check HERE (no awaits)
            // (Put back your margin eligibility logic, but based on internal balances + required
            // margin.)
            MARGIN_ACCOUNTS.with(|accounts| {
                let mut accounts = accounts.borrow_mut();
                let account = accounts.entry(user).or_insert(MarginAccount {
                    user,
                    balances: BTreeMap::new(),
                    required_margin: 0,
                });
                let current = account.get_balance(&asset);
                if current < amount_u128 {
                    return Err(ClearingError::InsufficientExcessMargin {
                        current: candid::Nat::from(current),
                        requested: amount.clone(),
                        required: amount.clone(), // replace later with true required margin logic
                    });
                }
                account.set_balance(asset.clone(), current - amount_u128);
                Ok(())
            })?;

            plan.status = PlanStatus::Finalised;

            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(withdrawal_id, plan));
        };

        Ok(())
    })
    .await;

    result.into()
}
