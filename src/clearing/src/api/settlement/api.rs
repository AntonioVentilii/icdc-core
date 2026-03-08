use candid::Principal;
use ic_cdk::api::is_controller;
use ic_cdk_macros::update;
use shared::{
    constants::VUSD_ASSET_ID,
    types::{Price, Series},
};

use super::{errors::SettlementError, params::SettleSeriesParams, results::SettleSeriesResult};
use crate::{
    guards::caller_is_not_anonymous,
    memory::{
        ACCOUNT_STATES, COLLATERAL_ASSETS, CONFIG, INSURANCE_FUND, POSITIONS, REGISTRY_CANISTER,
        SERIES, SETTLEMENT_PLANS, TREASURY,
    },
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
    let (insurance_fund_fee_ratio, protocol_fee_ratio) = CONFIG.with(|c| {
        let c = c.borrow();
        (c.insurance_fund_fee_ratio, c.protocol_fee_ratio)
    });

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
                    existing: Box::new(existing.settlement_price),
                    requested: Box::new(settlement_price),
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
                            settlement_data.push((user, pos.net_qty, pos.reserved_margin_usd));
                            plan_data.push((user, pos.net_qty, pos.reserved_margin_usd));
                        }
                    }
                    Ok::<(Vec<(User, i128, u128)>, Vec<(User, i128, u128)>), SettlementError>((
                        settlement_data,
                        plan_data,
                    ))
                })
            })?;

            check_settlement_solvency(&ser, &settlement_price, &positions_to_settle)?;

            // Compute accounting updates using centralized fee utilities
            let mut accounting_updates: Vec<(User, i128)> = Vec::new();
            let mut total_insurance_fee: u128 = 0;
            let mut total_protocol_fee: u128 = 0;

            for (user, net_qty, _) in positions_to_settle.iter().copied() {
                let payoff_u128 = get_settlement_value(&ser, &settlement_price, net_qty);

                let i_fee = crate::payoffs::fees::calculate_insurance_fee(
                    payoff_u128,
                    insurance_fund_fee_ratio,
                );
                let p_fee =
                    crate::payoffs::fees::calculate_insurance_fee(payoff_u128, protocol_fee_ratio);

                total_insurance_fee += i_fee;
                total_protocol_fee += p_fee;

                // Cashflow is payoff - total fees
                let cashflow: i128 = (payoff_u128 as i128) - (i_fee as i128) - (p_fee as i128);
                accounting_updates.push((user, cashflow));
            }

            SettlementPlan::get_or_create(SettlementPlanParams {
                series_id: series_id.clone(),
                settlement_price: settlement_price.clone(),
                fee: total_protocol_fee,
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
            if plan.status == PlanStatus::Planned {
                plan.status = PlanStatus::Executing;
            }

            ACCOUNT_STATES.with(|accounts| {
                let mut accounts = accounts.borrow_mut();

                while plan.accounting_cursor < plan.accounting_updates.len() {
                    let idx = plan.accounting_cursor;
                    let (user, cashflow) = plan.accounting_updates[idx];

                    if let Some(account) = accounts.get_mut(&user) {
                        // 1. Update cash balance (PnL)
                        account.cash_balance_usd += cashflow;

                        // 2. Release margin (Index matches accounting_updates as both are built
                        //    from SAME users list in Phase A)
                        let old_margin = plan.positions[idx].2;
                        account.reserved_margin_usd =
                            account.reserved_margin_usd.saturating_sub(old_margin);
                    }

                    plan.accounting_cursor += 1;

                    // Idiomatic chunking: we process in slices to avoid instruction limits.
                    // If we've processed 100 items, we yield and wait for the next call.
                    if plan.accounting_cursor % 100 == 0 {
                        break;
                    }
                }

                if plan.accounting_cursor == plan.accounting_updates.len() {
                    plan.accounting_applied = true;

                    // Distribute collected fees (internal USD accounting)
                    let insurance_fee_total = plan.insurance_fee_usd;
                    let protocol_fee_total = plan.fee_usd;

                    TREASURY.with(|t| {
                        let mut t = t.borrow_mut();
                        let current = t.get(VUSD_ASSET_ID).copied().unwrap_or(0);
                        t.insert(VUSD_ASSET_ID.to_string(), current + protocol_fee_total);
                    });

                    INSURANCE_FUND.with(|i| {
                        let mut i = i.borrow_mut();
                        let current = i.get(VUSD_ASSET_ID).copied().unwrap_or(0);
                        i.insert(VUSD_ASSET_ID.to_string(), current + insurance_fee_total);
                    });
                }
            });

            SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));
        }

        // ---------- Phase C: finalise ----------
        if plan.accounting_applied {
            plan.status = PlanStatus::Finalised;
            SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id, plan));
        }

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
    positions: &[(User, i128, u128)],
) -> Result<(), SettlementError> {
    let mut total_payoff: u128 = 0;

    for (_, net_qty, _) in positions.iter().copied() {
        let payoff = get_settlement_value(series, price, net_qty);
        total_payoff = total_payoff
            .checked_add(payoff)
            .ok_or(SettlementError::MathOverflow)?;
    }

    // Verify system solvency by comparing aggregated payouts against system equity
    let total_system_equity_usd = ACCOUNT_STATES.with(|accounts| {
        let accounts = accounts.borrow();
        let configs = COLLATERAL_ASSETS.with(|c| c.borrow().clone());
        accounts
            .values()
            .map(|acc| acc.calculate_equity_usd(&configs))
            .sum::<u128>()
    });

    if total_payoff > total_system_equity_usd {
        return Err(SettlementError::SolvencyViolation {
            total_payoff,
            total_collateral_usd: total_system_equity_usd,
        });
    }

    Ok(())
}
