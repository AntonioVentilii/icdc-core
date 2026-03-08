use candid::Principal;
use ic_cdk::api::is_controller;
use ic_cdk_macros::update;
use shared::{
    constants::VUSD_ASSET_ID,
    types::{Price, Series, SeriesId},
};

use super::{errors::SettlementError, params::SettleSeriesParams, results::SettleSeriesResult};
use crate::{
    guards::caller_is_not_anonymous,
    memory::{
        ACCOUNT_STATES, COLLATERAL_ASSETS, CONFIG, INSURANCE_FUND, POSITIONS, REGISTRY_CANISTER,
        SERIES, SETTLEMENT_PLANS, TREASURY,
    },
    payoffs::{fees::calculate_settlement_fee, get_settlement_value},
    types::{
        errors::CommonError,
        plans::{PlanStatus, SettlementPlan, SettlementPlanParams, SettlementPosition},
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
            prepare_settlement_impl(
                &ser,
                &series_id,
                &settlement_price,
                insurance_fund_fee_ratio,
                protocol_fee_ratio,
            )?
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

                while plan.accounting_cursor < plan.positions.len() {
                    let idx = plan.accounting_cursor;
                    let pos = &plan.positions[idx];

                    if let Some(account) = accounts.get_mut(&pos.user) {
                        // 1. Update cash balance (PnL)
                        account.cash_balance_usd += pos.cashflow_usd;

                        // 2. Release margin
                        account.reserved_margin_usd = account
                            .reserved_margin_usd
                            .saturating_sub(pos.reserved_margin_usd);
                    }

                    plan.accounting_cursor += 1;

                    // Idiomatic chunking: we process in slices to avoid instruction limits.
                    // If we've processed 100 items, we yield and wait for the next call.
                    if plan.accounting_cursor % 100 == 0 {
                        break;
                    }
                }

                if plan.accounting_cursor == plan.positions.len() {
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

/// Core synchronous logic for building a settlement plan and removing positions.
///
/// This is extracted for unit testing atomicity and solvency checks.
pub(crate) fn prepare_settlement_impl(
    ser: &Series,
    series_id: &SeriesId,
    settlement_price: &Price,
    insurance_fund_fee_ratio: u16,
    protocol_fee_ratio: u16,
) -> Result<SettlementPlan, SettlementError> {
    let mut total_insurance_fee: u128 = 0;
    let mut total_protocol_fee: u128 = 0;

    // Gather positions for settlement and compute payoffs/fees.
    let positions_to_settle = POSITIONS.with(|positions| {
        let positions = positions.borrow();

        let mut results = Vec::new();

        for ((user, sid), pos) in positions.iter() {
            if sid != series_id {
                continue;
            }

            let payoff_u128 = get_settlement_value(ser, settlement_price, pos.net_qty);

            let i_fee = calculate_settlement_fee(payoff_u128, insurance_fund_fee_ratio);

            let p_fee = calculate_settlement_fee(payoff_u128, protocol_fee_ratio);

            let cashflow: i128 = (payoff_u128 as i128) - (i_fee as i128) - (p_fee as i128);

            total_insurance_fee += i_fee;
            total_protocol_fee += p_fee;

            results.push(SettlementPosition {
                user: *user,
                net_qty: pos.net_qty,
                reserved_margin_usd: pos.reserved_margin_usd,
                cashflow_usd: cashflow,
            });
        }
        results
    });

    // Perform solvency check before any state modifications.
    check_settlement_solvency(ser, settlement_price, &positions_to_settle)?;

    // Now that solvency is verified, atomically remove positions and build the plan.
    POSITIONS.with(|positions| {
        let mut positions = positions.borrow_mut();
        for pos in &positions_to_settle {
            positions.remove(&(pos.user, series_id.clone()));
        }
    });

    Ok(SettlementPlan::get_or_create(SettlementPlanParams {
        series_id: series_id.clone(),
        settlement_price: settlement_price.clone(),
        fee: total_protocol_fee,
        insurance_fee: total_insurance_fee,
        positions: positions_to_settle,
    }))
}

/// Validates aggregate solvency for a settlement batch.
///
/// Under the new architecture, we only need to ensure the system is solvent
/// in aggregate (sum of payoffs <= total collateral value in system).
/// Individual user insolvency is handled by the liquidator.
fn check_settlement_solvency(
    series: &Series,
    price: &Price,
    positions: &[SettlementPosition],
) -> Result<(), SettlementError> {
    let mut total_payoff: u128 = 0;

    for pos in positions {
        let payoff = get_settlement_value(series, price, pos.net_qty);
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

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{Description, PayoffType, PayoutUnit, SeriesId};

    use super::*;
    use crate::{
        memory::{ACCOUNT_STATES, COLLATERAL_ASSETS, POSITIONS},
        types::{margin::AccountState, user::User},
        Position,
    };

    #[test]
    fn test_settle_series_atomicity_on_solvency_failure() {
        let user_p = Principal::from_slice(&[1]);
        let user = User(user_p);
        let series_id = SeriesId::from("test_ser".to_string());

        let series = Series {
            series_id: series_id.clone(),
            underlying: "BTC".to_string(),
            expiry_ns: 2000000000,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(100, 0)),
            price_precision: 0,
            payout_unit: PayoutUnit::usd(),
            oracle_source: "oracle".to_string(),
            creator: Principal::anonymous(),
            created_at_ns: 1000000000,
            title: "Test".to_string(),
            description: Description::plain("Test Description"),
        };

        let settlement_price = Price::new(200, 0); // payoff 100 per unit (200 - 100)

        POSITIONS.with(|pos| {
            let mut pos = pos.borrow_mut();
            pos.clear();
            pos.insert(
                (user, series_id.clone()),
                Position {
                    user,
                    series_id: series_id.clone(),
                    net_qty: 1,
                    reserved_margin_usd: 10,
                },
            );
        });

        // Set total system equity to 50 USD (less than 100 payoff)
        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            acc.clear();
            let mut a = AccountState::new(user);
            a.cash_balance_usd = 50_000_000; // 50 USD (6 decimals)
            acc.insert(user, a);
        });

        // Mock collateral assets for calculate_equity_usd
        COLLATERAL_ASSETS.with(|c| c.borrow_mut().clear());

        let result = prepare_settlement_impl(&series, &series_id, &settlement_price, 0, 0);

        assert!(result.is_err());
        if let Err(SettlementError::SolvencyViolation {
            total_payoff,
            total_collateral_usd,
        }) = result
        {
            assert_eq!(total_payoff, 100_000_000); // 100 USD (6 decimals)
            assert_eq!(total_collateral_usd, 50_000_000); // 50 USD
        } else {
            panic!("Expected SolvencyViolation, got {:?}", result);
        }

        // Verify position STILL EXISTS (atomicity check)
        POSITIONS.with(|pos| {
            assert!(pos.borrow().contains_key(&(user, series_id)));
        });
    }

    #[test]
    fn test_settle_series_normal_flow() {
        let user_p = Principal::from_slice(&[1]);
        let user = User(user_p);
        let series_id = SeriesId::from("test_ser".to_string());

        let series = Series {
            series_id: series_id.clone(),
            underlying: "BTC".to_string(),
            expiry_ns: 2000000000,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(100, 0)),
            price_precision: 0,
            payout_unit: PayoutUnit::usd(),
            oracle_source: "oracle".to_string(),
            creator: Principal::anonymous(),
            created_at_ns: 1000000000,
            title: "Test".to_string(),
            description: Description::plain("Test Description"),
        };

        let settlement_price = Price::new(150, 0); // payoff 50 per unit

        POSITIONS.with(|pos| {
            let mut pos = pos.borrow_mut();
            pos.clear();
            pos.insert(
                (user, series_id.clone()),
                Position {
                    user,
                    series_id: series_id.clone(),
                    net_qty: 1,
                    reserved_margin_usd: 10,
                },
            );
        });

        // Set total system equity to 1000 USD
        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            acc.clear();
            let mut a = AccountState::new(user);
            a.cash_balance_usd = 1_000_000_000;
            acc.insert(user, a);
        });

        let result = prepare_settlement_impl(&series, &series_id, &settlement_price, 10, 5); // 0.1% insurance, 0.05% protocol

        assert!(result.is_ok());
        let plan = result.unwrap();

        // payoff = 50. i_fee = 50 * 0.001 = 0.05. p_fee = 50 * 0.0005 = 0.025
        // fees are in USD units (6 decimals).
        // 50 USD = 50_000_000. i_fee = 50_000. p_fee = 25_000.
        assert_eq!(plan.insurance_fee_usd, 50_000);
        assert_eq!(plan.fee_usd, 25_000);
        assert_eq!(plan.positions[0].cashflow_usd, 50_000_000 - 50_000 - 25_000);

        // Verify position WAS REMOVED
        POSITIONS.with(|pos| {
            assert!(!pos.borrow().contains_key(&(user, series_id)));
        });
    }
}
