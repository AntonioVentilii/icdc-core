use std::{collections::HashMap, time::Duration};

use candid::Principal;
use ic_cdk::api::is_controller;
use ic_cdk_macros::{query, update};
use ic_cdk_timers::{self, set_timer};
use shared::{
    constants::VUSD_ASSET_ID,
    types::{Series, SeriesId, SettlementInput},
};

use super::{errors::SettlementError, params::SettleSeriesParams, results::SettleSeriesResult};
use crate::{
    guards::{caller_is_controller, caller_is_not_anonymous},
    memory::{
        ACCOUNT_STATES, ASSET_METRICS, COLLATERAL_ASSETS, CONFIG, INSURANCE_FUND, POSITIONS,
        REGISTRY_CANISTER, SERIES, SETTLEMENT_PLANS, TREASURY,
    },
    payoffs::{fees::calculate_settlement_fee, get_settlement_value},
    types::{
        errors::CommonError,
        plans::{
            PlanStatus, SettlementPlan, SettlementPlanParams, SettlementPosition,
            SettlementStatusView,
        },
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
/// **Note on Chunking:** To avoid instruction limits, this method processes positions in batches
/// of 100 per call. If there are many positions, the method will return
/// [`SettleSeriesResult::Processing`]. The caller must repeatedly call `settle_series` with the
/// same parameters until it returns [`SettleSeriesResult::Ok`].
///
/// **Authorization:** Every call is gated to canister controllers or the designated oracle for
/// the series. Unauthorized callers are rejected even if a plan already exists.
#[update(guard = "caller_is_not_anonymous")]
pub async fn settle_series(params: SettleSeriesParams) -> SettleSeriesResult {
    let caller = ic_cdk::caller();

    // ---------- Full authorization on EVERY call ----------
    // Controllers are always authorized.
    if !is_controller(&caller) {
        let oracle_source = SERIES.with(|s| {
            s.borrow()
                .get(&params.series_id)
                .map(|ser| ser.oracle_source.clone())
        });

        // If the series no longer exists, check if there is an active plan
        // (the series is removed on finalization but the plan may still be
        // in-flight).  Fall back to the plan's oracle_source.
        let oracle_source = match oracle_source {
            Some(src) => src,
            None => {
                let plan_oracle = SETTLEMENT_PLANS.with(|m| {
                    m.borrow()
                        .get(&params.series_id)
                        .map(|p| p.oracle_source.clone())
                });
                match plan_oracle {
                    Some(src) => src,
                    None => {
                        return SettleSeriesResult::Err(SettlementError::Common(
                            CommonError::Unauthorized,
                        ));
                    }
                }
            }
        };

        let registry_canister = REGISTRY_CANISTER.with(|r| *r.borrow());
        if registry_canister == Principal::anonymous() {
            return SettleSeriesResult::Err(SettlementError::Common(CommonError::RegistryNotSet));
        }

        let is_authorized: Result<(bool,), _> = ic_cdk::call(
            registry_canister,
            "is_oracle_authorized",
            (oracle_source, caller),
        )
        .await;

        match is_authorized {
            Ok((true,)) => {} // authorized
            Ok((false,)) => {
                return SettleSeriesResult::Err(SettlementError::Common(CommonError::Unauthorized));
            }
            Err((code, msg)) => {
                return SettleSeriesResult::Err(SettlementError::Common(CommonError::Internal(
                    format!("Registry call failed: {:?} - {}", code, msg),
                )));
            }
        }
    }

    settle_series_inner(params).await
}

/// Returns the full settlement plan including per-position accounting details (admin only).
#[query(guard = "caller_is_controller")]
pub fn get_settlement_plan(series_id: SeriesId) -> Option<SettlementPlan> {
    SETTLEMENT_PLANS.with(|m| m.borrow().get(&series_id).cloned())
}

/// Returns the public settlement progress for a derivative series without exposing user positions.
#[query(guard = "caller_is_not_anonymous")]
pub fn get_settlement_status(series_id: SeriesId) -> Option<SettlementStatusView> {
    SETTLEMENT_PLANS.with(|m| m.borrow().get(&series_id).map(SettlementStatusView::from))
}

/// Admin function to manually resume a stuck settlement plan, e.g., if the timer was dropped during
/// upgrade.
#[update(guard = "caller_is_controller")]
pub async fn resume_settlement(series_id: SeriesId) -> SettleSeriesResult {
    let plan = SETTLEMENT_PLANS.with(|m| m.borrow().get(&series_id).cloned());
    if let Some(p) = plan {
        let params = SettleSeriesParams {
            series_id: p.series_id,
            settlement: p.settlement,
        };
        settle_series_inner(params).await
    } else {
        SettleSeriesResult::Err(SettlementError::Common(CommonError::Internal(
            "Plan not found".to_string(),
        )))
    }
}

/// Internal settlement logic — caller has already been authorized.
///
/// This function is also used by the timer-based self-resumption so that
/// background processing does not re-enter the IC update guard.
pub(crate) async fn settle_series_inner(params: SettleSeriesParams) -> SettleSeriesResult {
    let (insurance_fund_fee_ratio, protocol_fee_ratio) = CONFIG.with(|c| {
        let c = c.borrow();
        (c.insurance_fund_fee_ratio, c.protocol_fee_ratio)
    });

    let result: Result<SettleSeriesResult, SettlementError> = (async {
        let SettleSeriesParams {
            series_id,
            settlement,
        } = params;

        // ---------- Phase A: build or resume plan ----------
        let existing_plan = SETTLEMENT_PLANS.with(|m| m.borrow().get(&series_id).cloned());

        let mut plan = if let Some(existing) = existing_plan {
            if existing.status == PlanStatus::Finalised {
                return Ok(SettleSeriesResult::ok());
            }

            if existing.settlement != settlement {
                // For simplicity in the log check, we just return an error if mismatched.
                // Comparing SettlementInput (enum) is fine.
                return Err(SettlementError::Common(CommonError::InvalidInput(
                    "Inconsistent settlement data".to_string(),
                )));
            }
            existing
        } else {
            // We need the full series object for prepare_settlement_impl
            let series = SERIES.with(|s| {
                s.borrow()
                    .get(&series_id)
                    .cloned()
                    .ok_or(SettlementError::Common(CommonError::Unauthorized))
            })?;

            prepare_settlement_impl(
                &series,
                &series_id,
                &settlement,
                insurance_fund_fee_ratio,
                protocol_fee_ratio,
            )?
        };

        if plan.status == PlanStatus::Finalised {
            return Ok(SettleSeriesResult::ok());
        }

        // ---------- Phase B: apply internal accounting updates ----------
        if !plan.accounting_applied {
            if plan.status == PlanStatus::Planned {
                plan.status = PlanStatus::Executing;
            }

            let newly_applied = apply_settlement_accounting_logic(&mut plan);
            SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan.clone()));

            if newly_applied {
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
        }

        // ---------- Phase C: finalise ----------
        if plan.accounting_applied {
            plan.status = PlanStatus::Finalised;
            SETTLEMENT_PLANS.with(|m| m.borrow_mut().insert(series_id.clone(), plan));

            // Clean up: remove the series from active series list
            SERIES.with(|s| s.borrow_mut().remove(&series_id));

            Ok(SettleSeriesResult::ok())
        } else {
            // Automatic self-resumption: schedule a timer to continue processing
            // in the background. Calls settle_series_inner directly to bypass the
            // IC update guard (the original caller was already authorized).
            let params_clone = SettleSeriesParams {
                series_id: series_id.clone(),
                settlement: settlement.clone(),
            };

            set_timer(Duration::from_millis(50), move || {
                ic_cdk::spawn(async move {
                    let _ = settle_series_inner(params_clone).await;
                });
            });

            Ok(SettleSeriesResult::processing())
        }
    })
    .await;

    match result {
        Ok(res) => res,
        Err(e) => SettleSeriesResult::Err(e),
    }
}

/// Core synchronous logic for building a settlement plan and removing positions.
///
/// This is extracted for unit testing atomicity and solvency checks.
pub(crate) fn prepare_settlement_impl(
    ser: &Series,
    series_id: &SeriesId,
    settlement: &SettlementInput,
    insurance_fund_fee_ratio: u16,
    protocol_fee_ratio: u16,
) -> Result<SettlementPlan, SettlementError> {
    let mut total_insurance_fee: u128 = 0;
    let mut total_protocol_fee: u128 = 0;
    let mut total_net_payoff: u128 = 0;

    // Gather positions for settlement and compute payoffs/fees.
    let positions_to_settle = POSITIONS.with(
        |positions: &std::cell::RefCell<crate::types::margin::PositionsMap>| {
            let positions = positions.borrow();

            let mut results = Vec::new();

            for ((user, sid, outcome_id), pos) in positions.iter() {
                if sid != series_id {
                    continue;
                }

                let payoff_u128 = get_settlement_value(ser, pos, settlement);

                let i_fee = calculate_settlement_fee(payoff_u128, insurance_fund_fee_ratio);

                let p_fee = calculate_settlement_fee(payoff_u128, protocol_fee_ratio);

                let cashflow: i128 = (payoff_u128 as i128) - (i_fee as i128) - (p_fee as i128);

                total_insurance_fee += i_fee;
                total_protocol_fee += p_fee;

                // Only positive cashflows represent outflows from the system equity pool.
                if cashflow > 0 {
                    total_net_payoff += cashflow as u128;
                }

                results.push(SettlementPosition {
                    user: *user,
                    outcome_id: outcome_id.clone(),
                    net_qty: pos.net_qty,
                    reserved_margin_usd: pos.reserved_margin_usd,
                    cashflow_usd: cashflow,
                });
            }
            results
        },
    );

    // Perform solvency check before any state modifications.
    // Uses the aggregate net payoff (post-fee) — fees stay in the system
    // (TREASURY + INSURANCE_FUND) so they don't reduce system equity.
    // We pass the settlement positions so that the check can compute
    // expected post-settlement equity (accounting for max(0,...) clamping).
    check_settlement_solvency(total_net_payoff, &positions_to_settle)?;

    // Now that solvency is verified, atomically remove positions and build the plan.
    POSITIONS.with(
        |positions: &std::cell::RefCell<std::collections::BTreeMap<_, _>>| {
            let mut positions = positions.borrow_mut();
            for pos in &positions_to_settle {
                positions.remove(&(pos.user, series_id.clone(), pos.outcome_id.clone()));
            }
        },
    );

    Ok(SettlementPlan::get_or_create(SettlementPlanParams {
        series_id: series_id.clone(),
        settlement: settlement.clone(),
        oracle_source: ser.oracle_source.clone(),
        fee: total_protocol_fee,
        insurance_fee: total_insurance_fee,
        positions: positions_to_settle,
    }))
}

/// Validates aggregate solvency for a settlement batch.
///
/// Computes the **expected post-settlement** system equity by simulating the
/// cashflows on each account and clamping to zero.  This is more conservative
/// than using pre-settlement equity because `calculate_equity_usd` returns
/// `max(0, cash + collateral)` — a loser whose equity would go negative
/// contributes 0 post-settlement, but their full positive equity would have
/// been counted in a pre-settlement snapshot, inflating the apparent resources.
///
/// Fees are credited to the TREASURY and INSURANCE_FUND and therefore remain
/// inside the system.  Individual user insolvency is handled by the liquidator.
fn check_settlement_solvency(
    total_net_payoff: u128,
    positions: &[SettlementPosition],
) -> Result<(), SettlementError> {
    // Build a map of user -> total cashflow for quick lookup.
    let mut cashflow_by_user = HashMap::<User, i128>::new();
    for pos in positions {
        *cashflow_by_user.entry(pos.user).or_default() += pos.cashflow_usd;
    }

    // Compute expected post-settlement equity per account.
    let total_post_settlement_equity = ACCOUNT_STATES.with(|accounts| {
        let accounts = accounts.borrow();
        let configs = COLLATERAL_ASSETS.with(|c| c.borrow().clone());
        let metrics = ASSET_METRICS.with(|m| m.borrow().clone());

        accounts
            .values()
            .map(|acc| {
                let current_equity = acc.calculate_raw_equity_i128(&configs, &metrics);
                let cashflow = cashflow_by_user.get(&acc.user).copied().unwrap_or(0);
                let post_equity = current_equity + cashflow;
                if post_equity < 0 {
                    0u128
                } else {
                    post_equity as u128
                }
            })
            .sum::<u128>()
    });

    if total_net_payoff > total_post_settlement_equity {
        return Err(SettlementError::SolvencyViolation {
            total_net_payoff,
            total_collateral_usd: total_post_settlement_equity,
        });
    }

    Ok(())
}

pub(crate) fn apply_settlement_accounting_logic(plan: &mut SettlementPlan) -> bool {
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

            // Batched execution: yield to avoid instruction limits.
            // In synchronous logic (tests), we can ignore the limit,
            // but we'll stick to 100 to keep it consistent with the production behavior.
            if plan.accounting_cursor % 100 == 0 && ic_cdk::api::instruction_counter() > 0 {
                break;
            }
        }
    });

    let newly_applied = plan.accounting_cursor == plan.positions.len() && !plan.accounting_applied;
    if newly_applied {
        plan.accounting_applied = true;
    }
    newly_applied
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{Description, PayoffType, PayoutUnit, Price, SeriesId, SettlementInput};

    use super::*;
    use crate::{
        memory::{ACCOUNT_STATES, COLLATERAL_ASSETS, POSITIONS},
        types::{margin::AccountState, user::User},
        Position,
    };

    #[test]
    fn test_settle_series_atomicity_on_solvency_failure() {
        let user_p = Principal::from_slice(&[3]);
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
            outcomes: None,
            oracle_source: "oracle".to_string(),
            creator: Principal::anonymous(),
            created_at_ns: 1000000000,
            title: "Test".to_string(),
            description: Description::plain("Test Description"),
        };

        // Settlement price 200 → Call payoff = 200 - 100 = 100 USD per unit.
        let settlement_price = Price::new(200, 0);

        POSITIONS.with(|pos| {
            let mut pos = pos.borrow_mut();
            pos.clear();
            pos.insert(
                (user, series_id.clone(), None),
                Position {
                    user,
                    series_id: series_id.clone(),
                    outcome_id: None,
                    net_qty: 1,
                    reserved_margin_usd: 10,
                },
            );
        });

        // User already underwater from prior losses: cash = -10 USD.
        // Pre-settlement equity  = max(0, -10) = 0.
        // Cashflow = +100 (no fees).
        // Post-settlement equity = max(0, -10 + 100) = 90.
        // Net payoff = 100 > 90 → SolvencyViolation.
        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            acc.clear();
            let mut a = AccountState::new(user);
            a.cash_balance_usd = -10_000_000; // -10 USD
            acc.insert(user, a);
        });

        COLLATERAL_ASSETS.with(|c| c.borrow_mut().clear());

        let result = prepare_settlement_impl(
            &series,
            &series_id,
            &SettlementInput::Price(settlement_price),
            0,
            0,
        );

        assert!(result.is_err());
        if let Err(SettlementError::SolvencyViolation {
            total_net_payoff,
            total_collateral_usd,
        }) = result
        {
            assert_eq!(total_net_payoff, 100_000_000); // 100 USD
            assert_eq!(total_collateral_usd, 90_000_000); // 90 USD post-settlement
        } else {
            panic!("Expected SolvencyViolation, got {:?}", result);
        }

        // Verify position STILL EXISTS (atomicity check)
        POSITIONS.with(
            |pos: &std::cell::RefCell<std::collections::BTreeMap<_, _>>| {
                assert!(pos.borrow().contains_key(&(user, series_id, None)));
            },
        );
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
            outcomes: None,
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
                (user, series_id.clone(), None),
                Position {
                    user,
                    series_id: series_id.clone(),
                    outcome_id: None,
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

        let result = prepare_settlement_impl(
            &series,
            &series_id,
            &SettlementInput::Price(settlement_price),
            10,
            5,
        ); // 0.1% insurance, 0.05% protocol

        assert!(result.is_ok());
        let plan = result.unwrap();

        // payoff = 50. i_fee = 50 * 0.001 = 0.05. p_fee = 50 * 0.0005 = 0.025
        // fees are in USD units (6 decimals).
        // 50 USD = 50_000_000. i_fee = 50_000. p_fee = 25_000.
        assert_eq!(plan.insurance_fee_usd, 50_000);
        assert_eq!(plan.fee_usd, 25_000);
        assert_eq!(plan.positions[0].cashflow_usd, 50_000_000 - 50_000 - 25_000);

        // Verify position WAS REMOVED
        POSITIONS.with(
            |pos: &std::cell::RefCell<std::collections::BTreeMap<_, _>>| {
                assert!(!pos.borrow().contains_key(&(user, series_id, None)));
            },
        );
    }

    #[test]
    fn test_get_settlement_plan_returns_active_plan() {
        let series_id = SeriesId::from("plan_query_test".to_string());
        let price = Price::new(100, 0);

        // Insert a plan directly
        let _plan = SettlementPlan::get_or_create(SettlementPlanParams {
            series_id: series_id.clone(),
            settlement: SettlementInput::Price(price.clone()),
            oracle_source: "test_oracle".to_string(),
            fee: 1000,
            insurance_fee: 500,
            positions: vec![],
        });

        // Query it back
        let queried = get_settlement_plan(series_id.clone()).unwrap();
        assert_eq!(queried.series_id, series_id);
        if let SettlementInput::Price(p) = queried.settlement {
            assert_eq!(p, price);
        } else {
            panic!("Expected price settlement");
        }
        assert_eq!(queried.fee_usd, 1000);
        assert_eq!(queried.insurance_fee_usd, 500);
        assert_eq!(queried.status, PlanStatus::Planned);

        // Non-existent plan returns None
        let missing = get_settlement_plan(SeriesId::from("nonexistent".to_string()));
        assert!(missing.is_none());

        // Cleanup
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().remove(&series_id));
    }

    #[test]
    fn test_chunked_accounting_processes_in_batches_of_100() {
        let series_id = SeriesId::from("chunk_test".to_string());
        let num_positions = 150;

        // Create 150 users with account states
        let users: Vec<User> = (0..num_positions)
            .map(|i| {
                let bytes = (i as u32).to_be_bytes();
                User(Principal::from_slice(&bytes))
            })
            .collect();

        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            for user in &users {
                let mut a = AccountState::new(*user);
                a.cash_balance_usd = 1_000_000; // 1 USD
                a.reserved_margin_usd = 500_000; // 0.5 USD
                acc.insert(*user, a);
            }
        });

        // Build settlement positions
        let positions: Vec<SettlementPosition> = users
            .iter()
            .map(|u| SettlementPosition {
                user: *u,
                outcome_id: None,
                net_qty: 1,
                reserved_margin_usd: 500_000,
                cashflow_usd: 100_000, // +0.1 USD each
            })
            .collect();

        // Create a plan with 150 positions
        let mut plan = SettlementPlan::get_or_create(SettlementPlanParams {
            series_id: series_id.clone(),
            settlement: SettlementInput::Price(Price::new(100, 0)),
            oracle_source: "test".to_string(),
            fee: 0,
            insurance_fee: 0,
            positions,
        });

        assert_eq!(plan.accounting_cursor, 0);
        assert!(!plan.accounting_applied);

        // --- Simulate Phase B: first chunk (should process 100) ---
        plan.status = PlanStatus::Executing;
        ACCOUNT_STATES.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            while plan.accounting_cursor < plan.positions.len() {
                let idx = plan.accounting_cursor;
                let pos = &plan.positions[idx];
                if let Some(account) = accounts.get_mut(&pos.user) {
                    account.cash_balance_usd += pos.cashflow_usd;
                    account.reserved_margin_usd = account
                        .reserved_margin_usd
                        .saturating_sub(pos.reserved_margin_usd);
                }
                plan.accounting_cursor += 1;
                if plan.accounting_cursor.is_multiple_of(100) {
                    break;
                }
            }
            if plan.accounting_cursor == plan.positions.len() {
                plan.accounting_applied = true;
            }
        });

        // After first chunk: cursor at 100, not yet complete
        assert_eq!(plan.accounting_cursor, 100);
        assert!(!plan.accounting_applied);

        // Verify first 100 users were updated
        ACCOUNT_STATES.with(|acc| {
            let acc = acc.borrow();
            let first_user = &users[0];
            let state = acc.get(first_user).unwrap();
            assert_eq!(state.cash_balance_usd, 1_100_000); // 1.0 + 0.1
            assert_eq!(state.reserved_margin_usd, 0); // released

            // User 100 should NOT be updated yet
            let user_100 = &users[100];
            let state_100 = acc.get(user_100).unwrap();
            assert_eq!(state_100.cash_balance_usd, 1_000_000); // unchanged
            assert_eq!(state_100.reserved_margin_usd, 500_000); // unchanged
        });

        // --- Simulate Phase B: second chunk (should process remaining 50) ---
        ACCOUNT_STATES.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            while plan.accounting_cursor < plan.positions.len() {
                let idx = plan.accounting_cursor;
                let pos = &plan.positions[idx];
                if let Some(account) = accounts.get_mut(&pos.user) {
                    account.cash_balance_usd += pos.cashflow_usd;
                    account.reserved_margin_usd = account
                        .reserved_margin_usd
                        .saturating_sub(pos.reserved_margin_usd);
                }
                plan.accounting_cursor += 1;
                if plan.accounting_cursor.is_multiple_of(100) {
                    break;
                }
            }
            if plan.accounting_cursor == plan.positions.len() {
                plan.accounting_applied = true;
            }
        });

        // After second chunk: cursor at 150, complete
        assert_eq!(plan.accounting_cursor, 150);
        assert!(plan.accounting_applied);

        // Verify user 100 is now updated
        ACCOUNT_STATES.with(|acc| {
            let acc = acc.borrow();
            let user_100 = &users[100];
            let state = acc.get(user_100).unwrap();
            assert_eq!(state.cash_balance_usd, 1_100_000); // now updated
            assert_eq!(state.reserved_margin_usd, 0); // released
        });

        // Cleanup
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().remove(&series_id));
        ACCOUNT_STATES.with(|acc| acc.borrow_mut().clear());
    }

    #[test]
    fn test_settlement_price_immutability() {
        let series_id = SeriesId::from("immutable_price_test".to_string());

        // Create a plan with price 100
        let plan = SettlementPlan::get_or_create(SettlementPlanParams {
            series_id: series_id.clone(),
            settlement: SettlementInput::Price(Price::new(100, 0)),
            oracle_source: "oracle".to_string(),
            fee: 0,
            insurance_fee: 0,
            positions: vec![],
        });

        // The plan is locked with price 100
        if let SettlementInput::Price(p) = plan.settlement {
            assert_eq!(p, Price::new(100, 0));
        } else {
            panic!("Expected price settlement");
        }

        // A subsequent get_or_create with the same series_id returns the original plan
        // (price is locked, cannot be changed)
        let plan2 = SettlementPlan::get_or_create(SettlementPlanParams {
            series_id: series_id.clone(),
            settlement: SettlementInput::Price(Price::new(200, 0)), // different price
            oracle_source: "oracle".to_string(),
            fee: 999,
            insurance_fee: 999,
            positions: vec![],
        });

        // Plan is returned unchanged — the original price (100) is preserved
        if let SettlementInput::Price(p) = plan2.settlement {
            assert_eq!(p, Price::new(100, 0));
        } else {
            panic!("Expected price settlement");
        }
        assert_eq!(plan2.fee_usd, 0); // original fee, not 999

        // Cleanup
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().remove(&series_id));
    }

    /// Validates that the solvency check uses net payoffs (post-fee), not gross.
    ///
    /// Scenario: gross payoff = 100 USD, fees = 2% → net payoff = 98 USD.
    /// System equity = 99.9 USD.  With the old gross check this would fail;
    /// with the corrected net check it succeeds because 98 < 99.9.
    #[test]
    fn test_solvency_check_uses_net_payoffs() {
        let user_p = Principal::from_slice(&[42]);
        let user = User(user_p);
        let series_id = SeriesId::from("net_payoff_test".to_string());

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
            description: Description::plain("Net payoff solvency test"),
            outcomes: None,
        };

        // Settlement price 200 → gross payoff = 200 - 100 = 100 USD = 100_000_000
        let settlement_price = Price::new(200, 0);

        POSITIONS.with(|pos| {
            let mut pos = pos.borrow_mut();
            pos.clear();
            pos.insert(
                (user, series_id.clone(), None),
                Position {
                    user,
                    series_id: series_id.clone(),
                    outcome_id: None,
                    net_qty: 1,
                    reserved_margin_usd: 10,
                },
            );
        });

        // System equity = 99.9 USD = 99_900_000 (less than 100 gross, more than 98 net)
        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            acc.clear();
            let mut a = AccountState::new(user);
            a.cash_balance_usd = 99_900_000;
            acc.insert(user, a);
        });

        COLLATERAL_ASSETS.with(|c| c.borrow_mut().clear());

        // Fee ratios: insurance 100 bps (1%) + protocol 100 bps (1%) = 2% total
        // Gross payoff = 100_000_000. Fees = 2_000_000. Net = 98_000_000.
        // 98_000_000 < 99_900_000 → solvency check passes.
        let result = prepare_settlement_impl(
            &series,
            &series_id,
            &SettlementInput::Price(settlement_price),
            100,
            100,
        );

        assert!(
            result.is_ok(),
            "Expected settlement to succeed (net payoff < equity), got: {:?}",
            result
        );

        let plan = result.unwrap();
        // Verify fees and net cashflow
        assert_eq!(plan.insurance_fee_usd, 1_000_000); // 1% of 100_000_000
        assert_eq!(plan.fee_usd, 1_000_000); // 1% of 100_000_000
        assert_eq!(
            plan.positions[0].cashflow_usd,
            100_000_000 - 1_000_000 - 1_000_000 // 98_000_000
        );

        // Cleanup
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().remove(&series_id));
        ACCOUNT_STATES.with(|acc| acc.borrow_mut().clear());
    }

    /// Validates that the solvency check uses post-settlement equity (with clamping).
    ///
    /// Scenario with two accounts:
    /// - Winner: equity = 10 USD, will receive +95 USD cashflow
    /// - Loser:  equity = 100 USD, will pay -200 USD cashflow (goes to -100 → clamped to 0)
    ///
    /// Pre-settlement total equity = 110 USD → a net payoff of 95 would pass naively.
    /// Post-settlement total equity = max(0, 105) + max(0, -100) = 105 + 0 = 105 USD.
    ///
    /// We set net payoff to 106 to be above post-settlement equity (105) but below
    /// pre-settlement equity (110), proving the check uses the correct baseline.
    #[test]
    fn test_solvency_check_accounts_for_post_settlement_clamping() {
        let winner_p = Principal::from_slice(&[10]);
        let winner = User(winner_p);
        let loser_p = Principal::from_slice(&[11]);
        let loser = User(loser_p);

        // Set up account states: winner = 10 USD, loser = 100 USD
        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            acc.clear();
            let mut w = AccountState::new(winner);
            w.cash_balance_usd = 10_000_000; // 10 USD
            acc.insert(winner, w);
            let mut l = AccountState::new(loser);
            l.cash_balance_usd = 100_000_000; // 100 USD
            acc.insert(loser, l);
        });

        COLLATERAL_ASSETS.with(|c| c.borrow_mut().clear());

        // Pre-settlement total equity = 110 USD
        // Simulate: winner gets +95, loser gets -200
        // Post-settlement: winner = 105, loser = max(0, -100) = 0 → total = 105
        let positions = vec![
            SettlementPosition {
                user: winner,
                outcome_id: None,
                net_qty: 1,
                reserved_margin_usd: 5_000_000,
                cashflow_usd: 95_000_000, // +95 USD
            },
            SettlementPosition {
                user: loser,
                outcome_id: None,
                net_qty: -1,
                reserved_margin_usd: 50_000_000,
                cashflow_usd: -200_000_000, // -200 USD
            },
        ];

        // Net payoff = 106 USD (only positive cashflows count, but we test the
        // solvency threshold directly).
        // 106 > 105 (post-settlement) but 106 < 110 (pre-settlement).
        let result = check_settlement_solvency(106_000_000, &positions);

        assert!(
            result.is_err(),
            "Expected SolvencyViolation because post-settlement equity (105) < net payoff (106)"
        );
        if let Err(SettlementError::SolvencyViolation {
            total_net_payoff,
            total_collateral_usd,
        }) = result
        {
            assert_eq!(total_net_payoff, 106_000_000);
            assert_eq!(total_collateral_usd, 105_000_000); // post-settlement clamped equity
        } else {
            panic!("Expected SolvencyViolation");
        }

        // Also verify that a payoff within post-settlement equity passes.
        let result_ok = check_settlement_solvency(105_000_000, &positions);
        assert!(
            result_ok.is_ok(),
            "Expected success when net payoff equals post-settlement equity"
        );

        // Cleanup
        ACCOUNT_STATES.with(|acc| acc.borrow_mut().clear());
    }
}
