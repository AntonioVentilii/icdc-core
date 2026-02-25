use ic_cdk_macros::update;

use crate::{
    assets::{
        asset::{handler::get_handler, params::AssetTransferParams},
        types::AssetAmount,
    },
    guards::caller_is_controller,
    memory::{MARGIN_ACCOUNTS, POSITIONS, SERIES, SETTLEMENT_PLANS},
    types::{
        account::LedgerAccount,
        errors::{LedgerError, SettlementError},
        params::SettleSeriesParams,
        plan::{PlanStatus, SettlementPlan},
        results::SettleSeriesResult,
        user::User,
    },
};

#[update(guard = "caller_is_controller")]
pub async fn settle_series(params: SettleSeriesParams) -> SettleSeriesResult {
    let result: Result<(), SettlementError> = (async {
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
                return Err(SettlementError::Ledger(LedgerError::TransferFailed(
                    "settlement already in progress with different settlement_price".to_string(),
                )));
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
                .ok_or(SettlementError::Ledger(LedgerError::UnsupportedLedger))?;

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
                    .ok_or(SettlementError::PayoffMathOverflow)?;

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

        // ---------- Phase B1: collect from participants ----------
        // We collect from ALL participants to pool collateral in the canister's main account.
        while (plan.payer_cursor) < plan.positions.len() {
            let idx = plan.payer_cursor;

            if plan.payer_receipts[idx].is_some() {
                plan.payer_cursor += 1;
                SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
                continue;
            }

            let (user, _) = plan.positions[idx];

            let created_at_time = plan.idempotency.to_created_at_time();

            let handler = get_handler(&plan.settlement_asset).map_err(SettlementError::Ledger)?;

            let res = handler
                .transfer(AssetTransferParams {
                    asset: &plan.settlement_asset,
                    from: LedgerAccount::UserClearing(user),
                    to: LedgerAccount::CanisterMain,
                    amount: AssetAmount::DeductAll, // Pool all collateral for this series
                    created_at_time,
                })
                .await;

            match res {
                Ok(block) => plan.payer_receipts[idx] = Some(candid::Nat::from(block).into()),
                Err(e) => {
                    SETTLEMENT_PLANS
                        .with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
                    return Err(SettlementError::Ledger(e));
                }
            }

            plan.payer_cursor += 1;
            SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
        }

        // ---------- Phase B2: pay receivers ----------
        while (plan.receiver_cursor) < plan.receivers.len() {
            let idx_u32 = plan.receiver_cursor;
            let idx = idx_u32;

            if plan.receiver_receipts[idx].is_some() {
                plan.receiver_cursor += 1;
                SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
                continue;
            }

            let (user, amount_u128) = plan.receivers[idx];

            let created_at_time = plan.idempotency.to_created_at_time();

            let handler = get_handler(&plan.settlement_asset).map_err(SettlementError::Ledger)?;

            let res = handler
                .transfer(AssetTransferParams {
                    asset: &plan.settlement_asset,
                    from: LedgerAccount::CanisterMain,
                    to: LedgerAccount::UserClearing(user),
                    amount: AssetAmount::Fixed(amount_u128),
                    created_at_time,
                })
                .await;

            match res {
                Ok(block) => plan.receiver_receipts[idx] = Some(candid::Nat::from(block).into()),
                Err(e) => {
                    SETTLEMENT_PLANS
                        .with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
                    return Err(SettlementError::Ledger(e));
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
