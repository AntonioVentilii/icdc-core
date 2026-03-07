use candid::Principal;
use ic_cdk::api::is_controller;
use ic_cdk_macros::update;
use shared::types::Asset;

use super::{errors::SettlementError, params::SettleSeriesParams, results::SettleSeriesResult};
use crate::{
    assets::{
        asset::{handler::get_handler, params::AssetTransferParams},
        types::AssetAmount,
    },
    guards::caller_is_not_anonymous,
    memory::{MARGIN_ACCOUNTS, POSITIONS, REGISTRY_CANISTER, SERIES, SETTLEMENT_PLANS},
    payoffs::get_settlement_value,
    types::{
        account::LedgerAccount,
        errors::{CommonError, LedgerError},
        plans::{PlanStatus, SettlementPlan, SettlementPlanParams},
        user::User,
    },
};

/// Settles a derivative series at a specific price.
///
/// This is a complex background operation consisting of:
/// 1. Creating or resuming a [`SettlementPlan`].
/// 2. Collecting collateral from users with net losses.
/// 3. Paying out collateral to users with net profits.
/// 4. Finalising internal margin account balances and releasing locked collateral.
///
/// This method is gated to canister controllers or the designated [`oracle_principal`] for the
/// series. It is intended to be called by an off-chain oracle or automation.
#[update(guard = "caller_is_not_anonymous")]
pub async fn settle_series(params: SettleSeriesParams) -> SettleSeriesResult {
    let result: Result<(), SettlementError> = (async {
        let SettleSeriesParams {
            series_id,
            settlement_price,
        } = params;

        // ---------- Authorization ----------
        let caller = ic_cdk::caller();

        let ser = SERIES.with(|s| {
            s.borrow()
                .get(&series_id)
                .cloned()
                .ok_or(SettlementError::Common(CommonError::Unauthorized))
        })?;

        if !is_controller(&caller) {
            let registry_canister = REGISTRY_CANISTER.with(|r| *r.borrow());

            if registry_canister == Principal::anonymous() {
                return Err(SettlementError::Common(CommonError::RegistryNotSet));
            }

            let (is_authorized,): (bool,) = ic_cdk::call(
                registry_canister,
                "is_oracle_authorized",
                (ser.oracle_source.clone(), caller),
            )
            .await
            .map_err(|(code, msg)| {
                SettlementError::Common(CommonError::Internal(format!(
                    "Registry call failed: {:?} - {}",
                    code, msg
                )))
            })?;

            if !is_authorized {
                return Err(SettlementError::Common(CommonError::Unauthorized));
            }
        }

        // ---------- Phase A: build or resume plan ----------
        let mut plan = if let Some(existing) =
            SETTLEMENT_PLANS.with(|m| m.borrow().get(&series_id).cloned())
        {
            if existing.status == PlanStatus::Finalised {
                return Ok(());
            }

            if existing.settlement_price != settlement_price {
                // TODO: specific error variant for this case
                return Err(SettlementError::Ledger(LedgerError::TransferError(
                    "settlement already in progress with different settlement_price".to_string(),
                )));
            }

            existing
        } else {
            let (positions_to_settle, settlement_asset_val) = SERIES.with(|s| {
                let ser = s
                    .borrow()
                    .get(&series_id)
                    .cloned()
                    .ok_or(SettlementError::Ledger(LedgerError::UnsupportedLedger))?;

                POSITIONS.with(|positions| {
                    let mut positions = positions.borrow_mut();

                    let users: Vec<User> = positions
                        .keys()
                        .filter(|(_, sid)| sid == &series_id)
                        .map(|(u, _)| *u)
                        .collect();

                    let mut settlement_data = Vec::new();
                    for user in users {
                        if let Some(pos) = positions.remove(&(user, series_id.clone())) {
                            settlement_data.push((user, pos.net_qty, pos.locked_collateral));
                        }
                    }
                    Ok::<(Vec<(User, i128, u128)>, Asset), SettlementError>((
                        settlement_data,
                        ser.settlement_asset.to_asset(),
                    ))
                })
            })?;

            let handler = get_handler(&settlement_asset_val).map_err(SettlementError::Ledger)?;
            let fee = handler
                .get_fee(&settlement_asset_val)
                .await
                .map_err(SettlementError::Ledger)?;

            // Compute net payers/receivers + accounting updates
            let mut payers: Vec<(User, u128)> = Vec::new();
            let mut receivers: Vec<(User, u128)> = Vec::new();
            let mut accounting_updates: Vec<(User, i8, u128, u128)> = Vec::new(); // (user, sign, profit_loss, margin_to_release)

            for (user, net_qty, locked_collateral) in positions_to_settle.iter().copied() {
                let ser = SERIES.with(|s| {
                    s.borrow()
                        .get(&series_id)
                        .cloned()
                        .expect("Series must exist during settlement")
                });

                let payoff_u128 = get_settlement_value(&ser, &settlement_price, net_qty);

                let cashflow: i128 = (payoff_u128 as i128) - (locked_collateral as i128);

                if cashflow < 0 {
                    let amount = cashflow.unsigned_abs();
                    payers.push((user, amount));
                    accounting_updates.push((user, -1, amount, locked_collateral));
                } else if cashflow > 0 {
                    let amount = cashflow.unsigned_abs();
                    receivers.push((user, amount));
                    accounting_updates.push((user, 1, amount, locked_collateral));
                } else {
                    accounting_updates.push((user, 0, 0, locked_collateral));
                }
            }

            SettlementPlan::get_or_create(SettlementPlanParams {
                series_id: series_id.clone(),
                settlement_price,
                settlement_asset: settlement_asset_val,
                fee,
                positions: positions_to_settle
                    .iter()
                    .map(|(u, q, _)| (*u, *q))
                    .collect(),
                payers,
                receivers,
                accounting_updates,
            })
        };

        if plan.status == PlanStatus::Finalised {
            return Ok(());
        }

        // Persist executing before any awaits
        plan.status = PlanStatus::Executing;
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));

        // ---------- Phase B1: collect from net losers ----------
        while (plan.payer_cursor) < plan.payers.len() {
            let idx = plan.payer_cursor;

            if plan.payer_receipts[idx].is_some() {
                plan.payer_cursor += 1;
                SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
                continue;
            }

            let (user, amount_u128) = plan.payers[idx];

            let created_at_time_ns = plan.idempotency_ns.to_created_at_time_ns();

            let handler = get_handler(&plan.settlement_asset).map_err(SettlementError::Ledger)?;

            let res = handler
                .transfer(AssetTransferParams {
                    asset: &plan.settlement_asset,
                    from: LedgerAccount::UserClearing(user),
                    to: LedgerAccount::CanisterMain,
                    amount: AssetAmount::Fixed(amount_u128),
                    created_at_time_ns,
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

            let (user, mut amount_u128) = plan.receivers[idx];

            let created_at_time_ns = plan.idempotency_ns.to_created_at_time_ns();

            let handler = get_handler(&plan.settlement_asset).map_err(SettlementError::Ledger)?;
            let fee = handler
                .get_fee(&plan.settlement_asset)
                .await
                .map_err(SettlementError::Ledger)?;

            if amount_u128 <= fee {
                // Too small to transfer, mark as skipped (0 block index)
                plan.receiver_receipts[idx] = Some(candid::Nat::from(0u64).into());
                plan.receiver_cursor += 1;
                SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
                continue;
            }

            amount_u128 -= fee;

            let res = handler
                .transfer(AssetTransferParams {
                    asset: &plan.settlement_asset,
                    from: LedgerAccount::CanisterMain,
                    to: LedgerAccount::UserClearing(user),
                    amount: AssetAmount::Fixed(amount_u128),
                    created_at_time_ns,
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

        // ---------- Phase C: apply internal accounting updates ----------
        if !plan.accounting_applied {
            let settlement_asset_val = plan.settlement_asset.clone();
            let fee = plan.fee;

            MARGIN_ACCOUNTS.with(|accounts| {
                let mut accounts = accounts.borrow_mut();

                while plan.accounting_cursor < plan.accounting_updates.len() {
                    let idx = plan.accounting_cursor;
                    let (user, sign, amount_u128, margin_release) = plan.accounting_updates[idx];

                    if let Some(account) = accounts.get_mut(&user) {
                        let current = account.get_balance(&settlement_asset_val);

                        match sign {
                            1 => {
                                // Winner: profit - fee
                                let net_increase = amount_u128.saturating_sub(fee);
                                account.set_balance(
                                    settlement_asset_val.clone(),
                                    current + net_increase,
                                );
                            }
                            -1 => {
                                // Loser: debt + fee
                                account.set_balance(
                                    settlement_asset_val.clone(),
                                    current.saturating_sub(amount_u128 + fee),
                                );
                            }
                            _ => {} // sign == 0 => no balance change
                        }

                        account.required_margin =
                            account.required_margin.saturating_sub(margin_release);
                    }

                    plan.accounting_cursor += 1;
                    // Persist cursor progression regularly
                    if plan.accounting_cursor % 10 == 0 {
                        SETTLEMENT_PLANS
                            .with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
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
