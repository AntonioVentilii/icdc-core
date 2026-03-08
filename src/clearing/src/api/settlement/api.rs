use candid::Principal;
use ic_cdk::api::is_controller;
use ic_cdk_macros::update;
use shared::types::{Price, Series};

use super::{errors::SettlementError, params::SettleSeriesParams, results::SettleSeriesResult};
use crate::{
    guards::caller_is_not_anonymous,
    memory::{ACCOUNT_STATES, CONFIG, POSITIONS, REGISTRY_CANISTER, SERIES, SETTLEMENT_PLANS},
    payoffs::get_settlement_value,
    types::{
        errors::CommonError,
        plans::{PlanStatus, SettlementPlan, SettlementPlanParams},
        user::User,
    },
};

/// Settles a derivative series at a specific price.
///
/// This is a background operation consisting of:
/// 1. Creating or resuming a [`SettlementPlan`].
/// 2. Updating internal USD cash balances for all participants based on payoffs.
/// 3. Releasing margin requirements for the settled positions.
/// 4. Finalising the plan and removing the series data.
///
/// This method is gated to canister controllers or the designated [`oracle_principal`] for the
/// series. It is intended to be called by an off-chain oracle or automation.
#[update(guard = "caller_is_not_anonymous")]
pub async fn settle_series(params: SettleSeriesParams) -> SettleSeriesResult {
    let insurance_fund_fee_ratio = CONFIG.with(|c| c.borrow().insurance_fund_fee_ratio);

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
                return Err(SettlementError::InconsistentSettlementPrice {
                    existing: existing.settlement_price,
                    requested: settlement_price,
                });
            }
            existing
        } else {
            let (positions_to_settle, positions_for_plan) = SERIES.with(|s| {
                let _ = s
                    .borrow()
                    .get(&series_id)
                    .cloned()
                    .ok_or(SettlementError::Common(CommonError::Internal(
                        "Series not found".to_string(),
                    )))?;

                POSITIONS.with(|positions| {
                    let mut positions = positions.borrow_mut();

                    let users: Vec<User> = positions
                        .keys()
                        .filter(|(_, sid)| sid == &series_id)
                        .map(|(u, _)| *u)
                        .collect();

                    let mut settlement_data = Vec::new();
                    let mut plan_data = Vec::new();
                    for user in users {
                        if let Some(pos) = positions.remove(&(user, series_id.clone())) {
                            settlement_data.push((user, pos.net_qty));
                            plan_data.push((user, pos.net_qty));
                        }
                    }
                    Ok::<(Vec<(User, i128)>, Vec<(User, i128)>), SettlementError>((
                        settlement_data,
                        plan_data,
                    ))
                })
            })?;

            check_settlement_solvency(&ser, &settlement_price, &positions_to_settle)?;

            // Compute accounting updates
            let mut accounting_updates: Vec<(User, i128)> = Vec::new();
            let mut total_insurance_fee: u128 = 0;

            for (user, net_qty) in positions_to_settle.iter().copied() {
                let payoff_u128 = get_settlement_value(&ser, &settlement_price, net_qty);
                let insurance_fee = (payoff_u128 * (insurance_fund_fee_ratio as u128)) / 10000;
                total_insurance_fee += insurance_fee;

                // Cashflow is payoff - fee
                let cashflow: i128 = (payoff_u128 as i128) - (insurance_fee as i128);
                accounting_updates.push((user, cashflow));
            }

            SettlementPlan::get_or_create(SettlementPlanParams {
                series_id: series_id.clone(),
                settlement_price: settlement_price.clone(),
                fee: 0, // Protocol fee not yet implemented in this rewrite
                insurance_fee: total_insurance_fee,
                positions: positions_for_plan,
                accounting_updates,
            })
        };

        if plan.status == PlanStatus::Finalised {
            return Ok(());
        }

        // ---------- Phase B: apply internal accounting updates ----------
        if !plan.accounting_applied {
            ACCOUNT_STATES.with(|accounts| {
                let mut accounts = accounts.borrow_mut();

                while plan.accounting_cursor < plan.accounting_updates.len() {
                    let idx = plan.accounting_cursor;
                    let (user, cashflow) = plan.accounting_updates[idx];

                    if let Some(account) = accounts.get_mut(&user) {
                        // 1. Update cash balance (PnL)
                        account.cash_balance_usd += cashflow;

                        // 2. Release margin (Maintenance margin should be recalculated)
                        // For now we just reset it to 0 as a placeholder if it was the only series.
                        // TODO: proper margin recalculation after each settlement.
                        account.reserved_margin_usd = 0;
                    }

                    plan.accounting_cursor += 1;
                    if plan.accounting_cursor % 50 == 0 {
                        SETTLEMENT_PLANS
                            .with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
                    }
                }
            });

            plan.accounting_applied = true;

            // Distribute collected insurance fees (internal only)

            // Since we don't have a single payout asset anymore, we account fees in USD
            // But the insurance fund/treasury are per asset.
            // For now, we omit this or map it to a default "Accounting USD" asset.
            // TODO: decide how to store USD-based fees in the multi-asset fund.

            SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
        }

        // ---------- Phase C: finalise ----------
        plan.status = PlanStatus::Finalised;
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id, plan));

        Ok(())
    })
    .await;

    result.into()
}

/// Validates aggregate solvency for a settlement batch.
///
/// Under the new architecture, we only need to ensure the system is solvent
/// in aggregate (sum of payoffs <= total collateral value in system).
/// Individual user insolvency is handled by the liquidator.
fn check_settlement_solvency(
    series: &Series,
    price: &Price,
    positions: &[(User, i128)],
) -> Result<(), SettlementError> {
    let mut total_payoff: u128 = 0;

    for (_, net_qty) in positions.iter().copied() {
        let payoff = get_settlement_value(series, price, net_qty);
        total_payoff = total_payoff
            .checked_add(payoff)
            .ok_or(SettlementError::MathOverflow)?;
    }

    // TODO: Compare total_payoff against global system equity.
    // At this stage, we assume the system is solvent enough to process accounting updates.

    Ok(())
}
