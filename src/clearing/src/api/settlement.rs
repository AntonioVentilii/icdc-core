use ic_cdk_macros::update;
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{TransferArg, TransferError},
};
use num_traits::ToPrimitive;
use shared::types::Asset;

use crate::{
    guards::caller_is_controller,
    memory::{MARGIN_ACCOUNTS, POSITIONS, SERIES, SETTLEMENT_PLANS},
    traits::ClearingAccountExt,
    types::{
        errors::ClearingError,
        params::SettleSeriesParams,
        plan::{PlanStatus, SettlementPlan},
        results::SettleSeriesResult,
        user::User,
    },
};

#[update(guard = "caller_is_controller")]
pub async fn settle_series(params: SettleSeriesParams) -> SettleSeriesResult {
    let result: Result<(), ClearingError> = (async {
        let SettleSeriesParams {
            series_id,
            settlement_price,
        } = params;

        // ---------- Phase A: build or resume plan ----------
        let mut plan = if let Some(existing) =
            SETTLEMENT_PLANS.with(|m| m.borrow().get(&series_id).cloned())
        {
            if existing.status == PlanStatus::Finalised {
                return Ok(());
            }

            if existing.settlement_price != settlement_price {
                // TODO: specific error variant for this case
                return Err(ClearingError::TransferFailed(
                    "settlement already in progress with different settlement_price".to_string(),
                ));
            }

            existing
        } else {
            // --- Build plan from scratch ---
            let settlement_asset_val = SERIES
                .with(|s| {
                    s.borrow()
                        .get(&series_id)
                        .map(|ser| ser.settlement_asset.to_asset())
                })
                .ok_or(ClearingError::UnsupportedLedger)?;

            let Asset::Icrc(ledger_id) = settlement_asset_val.clone();

            let (fee_nat,): (candid::Nat,) = ic_cdk::call(ledger_id, "icrc1_fee", ())
                .await
                .map_err(|(code, msg)| {
                    ClearingError::FetchingFeeFailed(format!("icrc1_fee RB: {:?}: {}", code, msg))
                })?;

            // TODO: consider fee in settlement logic later
            let _fee_u128: u128 = fee_nat.0.to_u128().ok_or(ClearingError::FeeMathOverflow)?;

            let positions_to_settle: Vec<(User, i128)> = POSITIONS.with(|positions| {
                let mut positions = positions.borrow_mut();

                let users: Vec<User> = positions
                    .keys()
                    .filter(|(_, sid)| *sid == series_id)
                    .map(|(u, _)| *u)
                    .collect();

                let mut settlement_data = Vec::new();

                for user in users {
                    if let Some(pos) = positions.remove(&(user, series_id.clone())) {
                        settlement_data.push((user, pos.net_qty));
                    }
                }

                settlement_data
            });

            // Compute payers/receivers + accounting updates
            let mut payers: Vec<(User, u128)> = Vec::new();
            let mut receivers: Vec<(User, u128)> = Vec::new();
            let mut accounting_updates: Vec<(User, i8, u128)> = Vec::new();

            for (user, net_qty) in positions_to_settle.iter().copied() {
                let payoff_i128: i128 = net_qty
                    .checked_mul(settlement_price as i128)
                    .ok_or(ClearingError::PayoffMathOverflow)?;

                let amount_u128: u128 = payoff_i128.unsigned_abs();

                if payoff_i128 < 0 {
                    payers.push((user, amount_u128));
                    accounting_updates.push((user, -1, amount_u128));
                } else if payoff_i128 > 0 {
                    // No fee subtraction for now
                    // TODO: consider fee subtraction from receiver side in future
                    receivers.push((user, amount_u128));
                    accounting_updates.push((user, 1, amount_u128));
                } else {
                    accounting_updates.push((user, 0, 0));
                }
            }

            SettlementPlan::get_or_create(
                series_id.clone(),
                settlement_price,
                settlement_asset_val,
                positions_to_settle,
                payers,
                receivers,
                accounting_updates,
            )
        };

        if plan.status == PlanStatus::Finalised {
            return Ok(());
        }

        // Persist executing before any awaits
        plan.status = PlanStatus::Executing;
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));

        // ---------- Phase B1: collect from payers ----------
        while (plan.payer_cursor as usize) < plan.payers.len() {
            let idx_u32 = plan.payer_cursor;
            let idx = idx_u32 as usize;

            if plan.payer_receipts[idx].is_some() {
                plan.payer_cursor += 1;
                SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
                continue;
            }

            let (user, amount_u128) = plan.payers[idx];

            let created_at_time = plan.idempotency.to_created_at_time();

            // Payer pays canister: User Subaccount -> Pool
            let args = TransferArg {
                from_subaccount: Some(user.clearing_subaccount()),
                to: Account {
                    owner: ic_cdk::id(),
                    subaccount: None, // Canister main account pool
                },
                amount: candid::Nat::from(amount_u128),
                fee: None,
                memo: None,
                created_at_time,
            };

            let Asset::Icrc(ledger_id) = plan.settlement_asset.clone();

            let (res,): (Result<candid::Nat, TransferError>,) =
                ic_cdk::call(ledger_id, "icrc1_transfer", (args,))
                    .await
                    .map_err(|(code, msg)| {
                        ClearingError::TransferFailed(format!("RB: {:?}: {}", code, msg))
                    })?;

            match res {
                Ok(block) => plan.payer_receipts[idx] = Some(block.into()),
                Err(TransferError::Duplicate { duplicate_of }) => {
                    plan.payer_receipts[idx] = Some(duplicate_of.into())
                }
                Err(e) => {
                    SETTLEMENT_PLANS
                        .with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
                    return Err(ClearingError::TransferFailed(format!("{:?}", e)));
                }
            }

            plan.payer_cursor += 1;
            SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
        }

        // ---------- Phase B2: pay receivers ----------
        while (plan.receiver_cursor as usize) < plan.receivers.len() {
            let idx_u32 = plan.receiver_cursor;
            let idx = idx_u32 as usize;

            if plan.receiver_receipts[idx].is_some() {
                plan.receiver_cursor += 1;
                SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
                continue;
            }

            let (user, amount_u128) = plan.receivers[idx];

            let created_at_time = plan.idempotency.to_created_at_time();

            // Canister pays receiver: Pool -> User Subaccount
            let args = TransferArg {
                from_subaccount: None, // Canister main account pool
                to: user.clearing_account(),
                amount: candid::Nat::from(amount_u128),
                fee: None,
                memo: None,
                created_at_time,
            };

            let Asset::Icrc(ledger_id) = plan.settlement_asset.clone();

            let (res,): (Result<candid::Nat, TransferError>,) =
                ic_cdk::call(ledger_id, "icrc1_transfer", (args,))
                    .await
                    .map_err(|(code, msg)| {
                        ClearingError::TransferFailed(format!("RB: {:?}: {}", code, msg))
                    })?;

            match res {
                Ok(block) => plan.receiver_receipts[idx] = Some(block.into()),
                Err(TransferError::Duplicate { duplicate_of }) => {
                    plan.receiver_receipts[idx] = Some(duplicate_of.into())
                }
                Err(e) => {
                    SETTLEMENT_PLANS
                        .with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
                    return Err(ClearingError::TransferFailed(format!("{:?}", e)));
                }
            }

            plan.receiver_cursor += 1;
            SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
        }

        // ---------- Phase C: apply internal accounting once ----------
        if !plan.accounting_applied {
            let settlement_asset_val = plan.settlement_asset.clone();

            MARGIN_ACCOUNTS.with(|accounts| {
                let mut accounts = accounts.borrow_mut();

                for (user, sign, amount_u128) in plan.accounting_updates.iter().copied() {
                    if let Some(account) = accounts.get_mut(&user) {
                        let current = account.get_balance(&settlement_asset_val);

                        match sign {
                            1 => account
                                .set_balance(settlement_asset_val.clone(), current + amount_u128),
                            -1 => account
                                .set_balance(settlement_asset_val.clone(), current - amount_u128),
                            _ => {} // sign == 0 => no balance change
                        }

                        account.required_margin = 0;
                    }
                }
            });

            plan.accounting_applied = true;
            SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
        }

        // ---------- Phase D: finalise ----------
        plan.status = PlanStatus::Finalised;
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id, plan));

        Ok(())
    })
    .await;

    result.into()
}
