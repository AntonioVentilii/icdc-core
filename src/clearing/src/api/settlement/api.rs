use core::{cell::RefCell, ops::Bound, time::Duration};
use std::collections::{BTreeMap, HashMap, HashSet};

use candid::Principal;
use ic_cdk::api::{instruction_counter, is_controller, msg_caller};
use ic_cdk_macros::{query, update};
use ic_cdk_timers::set_timer;
use shared::types::{BalanceDomain, Price, Series, SeriesId, SettlementInput};

use super::{
    errors::SettlementError,
    params::{BackfillSettlementEventsParams, ListSettledSeriesParams, SettleSeriesParams},
    results::{BackfillSettlementEventsResult, SettleSeriesResult, SettledSeriesPage},
};
use crate::{
    guards::{caller_is_controller, caller_is_not_anonymous},
    memory::{
        ACCOUNT_STATES, ASSET_METRICS, COLLATERAL_ASSETS, CONFIG, EVENTS, INSURANCE_FUND,
        LIMIT_ORDERS, NEXT_EVENT_ID, POSITIONS, REGISTRY_CANISTER, SERIES, SETTLEMENT_PLANS,
        TREASURY,
    },
    payoffs::{fees::calculate_settlement_fee, get_settlement_value},
    types::{
        errors::CommonError,
        event::{Event, EventType},
        margin::PositionsMap,
        plans::{
            PlanStatus, SettlementPlan, SettlementPlanParams, SettlementPosition,
            SettlementStatusView,
        },
        user::User,
    },
    utils::{
        registry,
        system::{canister_id, now_ns},
        vusd::get_internal_asset_id,
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
    let caller = msg_caller();

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
        let oracle_source = if let Some(src) = oracle_source {
            src
        } else {
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
        };

        let registry_canister = REGISTRY_CANISTER.with(|r| *r.borrow());
        if registry_canister == Principal::anonymous() {
            return SettleSeriesResult::Err(SettlementError::Common(CommonError::RegistryNotSet));
        }

        match registry::is_oracle_authorized(registry_canister, oracle_source, caller).await {
            Ok(true) => {}
            Ok(false) => {
                return SettleSeriesResult::Err(SettlementError::Common(CommonError::Unauthorized));
            }
            Err(e) => return SettleSeriesResult::Err(SettlementError::Common(e)),
        }
    }

    settle_series_inner(params).await
}

/// Returns the full settlement plan including per-position accounting details (admin only).
#[query(guard = "caller_is_controller")]
#[must_use]
pub fn get_settlement_plan(series_id: SeriesId) -> Option<SettlementPlan> {
    SETTLEMENT_PLANS.with(move |m| m.borrow().get(&series_id).cloned())
}

/// Returns the public settlement progress for a derivative series without exposing user positions.
#[query(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn get_settlement_status(series_id: SeriesId) -> Option<SettlementStatusView> {
    SETTLEMENT_PLANS.with(move |m| m.borrow().get(&series_id).map(SettlementStatusView::from))
}

/// Lists the ids of series that have been settled (resolved), with stable
/// cursor pagination.
///
/// A series appears here as soon as a [`SettlementPlan`] is opened for it — in
/// any [`PlanStatus`] — which is precisely when it stops being tradeable. This
/// is the authoritative resolution set: the clearing canister owns settlement
/// state, the registry does not. Front ends that build a "currently-tradeable"
/// candidate set call the registry's `list_series_with` with `only_unexpired`
/// for the open/unexpired catalog and subtract the ids returned here to drop the
/// resolved markets, instead of reconstructing resolution from the activity log.
///
/// Ids are returned in ascending order from `SETTLEMENT_PLANS` (a `BTreeMap`
/// keyed by `SeriesId`), so paging with the previous response's `next_cursor`
/// is stable as long as the underlying set is not mutated mid-traversal.
#[query(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn list_settled_series(params: ListSettledSeriesParams) -> SettledSeriesPage {
    let ListSettledSeriesParams {
        balance_domain,
        start_after,
        limit,
    } = params;

    // Default to "all remaining" when no limit is supplied, mirroring the
    // registry's pagination contract.
    let limit = usize::try_from(limit.unwrap_or(u64::MAX)).unwrap_or(usize::MAX);

    SETTLEMENT_PLANS.with(|plans| {
        let plans = plans.borrow();

        let mut iter = match &start_after {
            Some(after) => plans.range((Bound::Excluded(after), Bound::Unbounded)),
            None => plans.range(..),
        }
        .filter(|(_, plan)| balance_domain.is_none_or(|d| plan.balance_domain == d))
        .map(|(series_id, _)| series_id);

        let mut items = Vec::new();
        for _ in 0..limit {
            match iter.next() {
                Some(id) => items.push(id.clone()),
                None => {
                    // Fewer than `limit` ids remain: this is the last page.
                    return SettledSeriesPage {
                        items,
                        next_cursor: None,
                    };
                }
            }
        }

        // A full page was produced. Emit a cursor only if at least one more id
        // exists. `start_after` is exclusive, so the cursor is the last id
        // returned (mirroring `backfill_settlement_events`); resuming with it
        // yields the next id without dropping or repeating any.
        let next_cursor = iter
            .next()
            .is_some()
            .then(|| items.last().cloned())
            .flatten();
        SettledSeriesPage { items, next_cursor }
    })
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
            "Plan not found".to_owned(),
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
                // A plan has already been locked on this series with a
                // different settlement input. Rather than flattening this into
                // `InvalidInput`, return the structured variant so the oracle
                // tooling can diff the two.
                return Err(SettlementError::InconsistentSettlement {
                    existing: Box::new(existing.settlement.clone()),
                    requested: Box::new(settlement),
                });
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

                let internal_asset_id = get_internal_asset_id();

                TREASURY.with(|t| {
                    let mut t = t.borrow_mut();
                    let current = t.get(&internal_asset_id).copied().unwrap_or(0);
                    t.insert(internal_asset_id.clone(), current + protocol_fee_total);
                });

                INSURANCE_FUND.with(|i| {
                    let mut i = i.borrow_mut();
                    let current = i.get(&internal_asset_id).copied().unwrap_or(0);
                    i.insert(internal_asset_id.clone(), current + insurance_fee_total);
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

            set_timer(Duration::from_millis(50), async move {
                let _ = settle_series_inner(params_clone).await;
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
        |positions: &RefCell<PositionsMap>| -> Result<Vec<SettlementPosition>, SettlementError> {
            let positions = positions.borrow();

            let mut results = Vec::new();

            for ((user, sid, outcome_id), pos) in positions.iter() {
                if sid != series_id {
                    continue;
                }

                let payoff_u128 = get_settlement_value(ser, pos, settlement).map_err(|e| {
                    SettlementError::Common(CommonError::Internal(format!(
                        "Payoff calculation failed: {e:?}"
                    )))
                })?;

                let i_fee = calculate_settlement_fee(payoff_u128, insurance_fund_fee_ratio);

                let p_fee = calculate_settlement_fee(payoff_u128, protocol_fee_ratio);

                let cashflow: i128 = (payoff_u128.cast_signed())
                    - (pos.reserved_margin_usd.cast_signed())
                    - (i_fee.cast_signed())
                    - (p_fee.cast_signed());

                total_insurance_fee += i_fee;
                total_protocol_fee += p_fee;

                // Only positive cashflows represent outflows from the system equity pool.
                if cashflow > 0 {
                    total_net_payoff += cashflow.cast_unsigned();
                }

                results.push(SettlementPosition {
                    user: *user,
                    outcome_id: outcome_id.clone(),
                    net_qty: pos.net_qty,
                    reserved_margin_usd: pos.reserved_margin_usd,
                    cashflow_usd: cashflow,
                });
            }
            Ok(results)
        },
    )?;

    // Perform solvency check before any state modifications.
    // Uses the aggregate net payoff (post-fee) — fees stay in the system
    // (TREASURY + INSURANCE_FUND) so they don't reduce system equity.
    // We pass the settlement positions so that the check can compute
    // expected post-settlement equity (accounting for max(0,...) clamping).
    check_settlement_solvency(ser.balance_domain, total_net_payoff, &positions_to_settle)?;

    // Now that solvency is verified, atomically remove positions and build the plan.
    POSITIONS.with(|positions: &RefCell<BTreeMap<_, _>>| {
        let mut positions = positions.borrow_mut();
        for pos in &positions_to_settle {
            positions.remove(&(pos.user, series_id.clone(), pos.outcome_id.clone()));
        }
    });

    // Cancel every open LIMIT order on this series and release the maker's
    // blocked margin back to their account. Leaving orders in the book after
    // settlement would let takers match them against a series that no longer
    // trades, and would leave collateral reserved forever.
    cancel_open_limit_orders(series_id);

    Ok(SettlementPlan::get_or_create(SettlementPlanParams {
        series_id: series_id.clone(),
        settlement: settlement.clone(),
        oracle_source: ser.oracle_source.clone(),
        fee: total_protocol_fee,
        insurance_fee: total_insurance_fee,
        positions: positions_to_settle,
        balance_domain: ser.balance_domain,
    }))
}

/// Removes every open limit order on `series_id` and releases each order's
/// `blocked_margin_usd` from the maker's `reserved_margin_usd` (freeing that
/// collateral for other trades or withdrawals), mirroring the bookkeeping
/// done by [`cancel_limit_order`].
///
/// Called synchronously from [`prepare_settlement_impl`] right after positions
/// are removed so that the canister-level invariant "no open orders for a
/// settled series" holds before the plan is persisted. Idempotent: safe to
/// call on a series that has no open orders.
fn cancel_open_limit_orders(series_id: &SeriesId) {
    let cancelled: Vec<(User, BalanceDomain, u128)> = LIMIT_ORDERS.with(|orders| {
        let mut orders = orders.borrow_mut();
        let matching: Vec<_> = orders
            .iter()
            .filter(|(_, o)| &o.series_id == series_id)
            .map(|(id, o)| {
                (
                    id.clone(),
                    o.creator,
                    o.balance_domain,
                    o.blocked_margin_usd,
                )
            })
            .collect();

        for (id, _, _, _) in &matching {
            orders.remove(id);
        }

        matching
            .into_iter()
            .map(|(_, creator, domain, margin)| (creator, domain, margin))
            .collect()
    });

    if cancelled.is_empty() {
        return;
    }

    ACCOUNT_STATES.with(|accounts| {
        let mut accounts = accounts.borrow_mut();
        for (user, domain, blocked_margin_usd) in cancelled {
            if let Some(acc) = accounts.get_mut(&user) {
                let current = acc.get_reserved_margin_usd(domain);
                acc.set_reserved_margin_usd(domain, current.saturating_sub(blocked_margin_usd));
            }
        }
    });
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
/// Fees are credited to the TREASURY and `INSURANCE_FUND` and therefore remain
/// inside the system.  Individual user insolvency is handled by the liquidator.
fn check_settlement_solvency(
    domain: BalanceDomain,
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
                let current_equity = acc.calculate_raw_equity_i128(domain, &configs, &metrics);
                let cashflow = cashflow_by_user.get(&acc.user).copied().unwrap_or(0);
                let post_equity = current_equity + cashflow;
                if post_equity < 0 {
                    0_u128
                } else {
                    post_equity.cast_unsigned()
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
    // Settlement events emitted in this invocation; pushed once after the
    // accounting loop so we don't hold both ACCOUNT_STATES and EVENTS
    // borrows simultaneously.
    let mut emitted: Vec<Event> = Vec::new();

    // Price field on a Settled event records what the series resolved to.
    // For Price settlements we surface the actual settlement price; for
    // Outcome settlements there's no scalar — encode 1.0 as a neutral
    // placeholder so consumers can still read `decimal` without special-casing.
    let settlement_price: Price = match &plan.settlement {
        SettlementInput::Price(p) => p.clone(),
        SettlementInput::Outcome(_) => Price::new(1, 0),
    };

    ACCOUNT_STATES.with(|accounts| {
        let mut accounts = accounts.borrow_mut();

        while plan.accounting_cursor < plan.positions.len() {
            let idx = plan.accounting_cursor;
            let pos = &plan.positions[idx];

            if let Some(account) = accounts.get_mut(&pos.user) {
                let domain = plan.balance_domain;
                // 1. Update cash balance (PnL)
                let current_cash = account.get_cash_balance_usd(domain);
                account.set_cash_balance_usd(domain, current_cash + pos.cashflow_usd);

                // 2. Release margin
                let current_reserved = account.get_reserved_margin_usd(domain);
                account.set_reserved_margin_usd(
                    domain,
                    current_reserved.saturating_sub(pos.reserved_margin_usd),
                );

                // 3. Record a per-user Settled event so the trade-history query can surface the
                //    settlement. `qty` carries the signed cashflow (positive = winner, negative =
                //    loser).
                let event_id = NEXT_EVENT_ID.with(|id| {
                    let mut id = id.borrow_mut();
                    let current = *id;
                    *id += 1;
                    current
                });
                emitted.push(Event {
                    event_id,
                    clearing_id: canister_id(),
                    series_id: plan.series_id.clone(),
                    user: pos.user,
                    qty: pos.cashflow_usd,
                    price: settlement_price.clone(),
                    event_type: EventType::Settled,
                    timestamp: now_ns(),
                });
            }

            plan.accounting_cursor += 1;

            // Batched execution: yield to avoid instruction limits.
            // In synchronous logic (tests), we can ignore the limit,
            // but we'll stick to 100 to keep it consistent with the production behavior.
            if plan.accounting_cursor.is_multiple_of(100) && instruction_counter() > 0 {
                break;
            }
        }
    });

    if !emitted.is_empty() {
        EVENTS.with(|events| {
            events.borrow_mut().extend(emitted);
        });
    }

    let newly_applied = plan.accounting_cursor == plan.positions.len() && !plan.accounting_applied;
    if newly_applied {
        plan.accounting_applied = true;
    }
    newly_applied
}

/// One-shot, idempotent backfill of `Settled` events for plans finalized
/// before per-user event emission was added to `apply_settlement_accounting_logic`.
///
/// Iterates `SETTLEMENT_PLANS` in sorted order starting after
/// `params.start_after`. For each `Finalised` plan it synthesizes one
/// `EventType::Settled` event per `SettlementPosition` whose `(user,
/// series_id)` pair has no pre-existing `Settled` event in `EVENTS`. The
/// synthesized event's `timestamp` is the plan's original
/// `idempotency_ns` so backfilled rows sort alongside the live events
/// that would have been written at settlement time.
///
/// Yields cooperatively when the instruction counter is high so a single
/// call cannot exhaust the update budget. Callers loop until the response
/// returns `next_cursor: None`.
#[update(guard = "caller_is_controller")]
#[must_use]
pub fn backfill_settlement_events(
    params: BackfillSettlementEventsParams,
) -> BackfillSettlementEventsResult {
    // Threshold below the 10B-per-message instruction limit. Same shape
    // as the `apply_settlement_accounting_logic` chunking pattern but
    // using a hard ceiling rather than batch-of-100, since per-plan work
    // varies with the number of positions.
    #[cfg(not(test))]
    const INSTRUCTION_BUDGET: u64 = 1_000_000_000;

    let BackfillSettlementEventsParams { start_after } = params;

    // Per-series dedup: any plan whose series already has at least one
    // Settled event in EVENTS is treated as already backfilled. A plan
    // is backfilled atomically (all positions or none), so this single
    // marker is sufficient and matches the live-emission contract,
    // which emits one event per `SettlementPosition` — including
    // multiple positions per user on the same series (e.g. hedged
    // YES + NO).
    let mut series_already_backfilled: HashSet<SeriesId> = EVENTS.with(|events| {
        events
            .borrow()
            .iter()
            .filter(|e| matches!(e.event_type, EventType::Settled))
            .map(|e| e.series_id.clone())
            .collect()
    });

    let mut result = BackfillSettlementEventsResult::default();
    let mut emitted: Vec<Event> = Vec::new();

    SETTLEMENT_PLANS.with(|plans| {
        let plans = plans.borrow();

        // Lower bound on the iteration: exclude `start_after` itself.
        let iter: Box<dyn Iterator<Item = (&SeriesId, &SettlementPlan)>> = match &start_after {
            None => Box::new(plans.iter()),
            Some(after) => {
                Box::new(plans.range::<SeriesId, _>((Bound::Excluded(after), Bound::Unbounded)))
            }
        };

        for (series_id, plan) in iter {
            if plan.status == PlanStatus::Finalised {
                result.plans_scanned += 1;

                if series_already_backfilled.contains(series_id) {
                    // Skip the whole plan — at least one position has already
                    // produced a Settled event for this series.
                    result.events_skipped += plan.positions.len() as u64;
                } else {
                    // Use the original settlement-initiation timestamp so
                    // backfilled rows have meaningful chronology rather than the
                    // backfill execution time.
                    let timestamp = plan
                        .idempotency_ns
                        .to_created_at_time_ns()
                        .unwrap_or_else(now_ns);

                    let price: Price = match &plan.settlement {
                        SettlementInput::Price(p) => p.clone(),
                        SettlementInput::Outcome(_) => Price::new(1, 0),
                    };

                    for pos in &plan.positions {
                        let event_id = NEXT_EVENT_ID.with(|id| {
                            let mut id = id.borrow_mut();
                            let current = *id;
                            *id += 1;
                            current
                        });
                        emitted.push(Event {
                            event_id,
                            clearing_id: canister_id(),
                            series_id: series_id.clone(),
                            user: pos.user,
                            qty: pos.cashflow_usd,
                            price: price.clone(),
                            event_type: EventType::Settled,
                            timestamp,
                        });
                        result.events_emitted += 1;
                    }

                    series_already_backfilled.insert(series_id.clone());
                }
            }

            // Cooperative yield after every visited plan — including
            // non-finalized and already-backfilled ones — so the per-call
            // instruction budget remains bounded even when the cursor
            // encounters long runs of plans that produce no events.
            // `instruction_counter()` panics when called outside a canister,
            // so unit tests skip the check entirely and process every plan in
            // one pass. The cursor uses exclusive `start_after` semantics, so
            // the next call resumes after `series_id`.
            #[cfg(not(test))]
            {
                let used = instruction_counter();
                if used > INSTRUCTION_BUDGET {
                    result.next_cursor = Some(series_id.clone());
                    return;
                }
            }
        }

        result.next_cursor = None;
    });

    if !emitted.is_empty() {
        EVENTS.with(|events| {
            events.borrow_mut().extend(emitted);
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use core::cell::RefCell;
    use std::collections::BTreeMap;

    use candid::Principal;
    use shared::types::{
        BalanceDomain, Description, PayoffType, PayoutUnit, Price, Series, SeriesId,
        SettlementInput,
    };

    use crate::{
        api::settlement::{
            api::check_settlement_solvency, errors::SettlementError, get_settlement_plan,
            list_settled_series, params::ListSettledSeriesParams, prepare_settlement_impl,
        },
        memory::{ACCOUNT_STATES, COLLATERAL_ASSETS, POSITIONS, SETTLEMENT_PLANS},
        types::{
            margin::AccountState,
            plans::{PlanStatus, SettlementPlan, SettlementPlanParams, SettlementPosition},
            user::User,
        },
        Position,
    };

    #[test]
    fn settle_series_atomicity_on_solvency_failure() {
        let user_p = Principal::from_slice(&[3]);
        let user = User(user_p);
        let series_id = SeriesId::from("test_ser".to_owned());

        let series = Series {
            series_id: series_id.clone(),
            underlying: "BTC".to_owned(),
            expiry_ns: 2_000_000_000,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(100, 0)),
            price_precision: 0,
            payout_unit: PayoutUnit::usd(),
            outcomes: None,
            balance_domain: BalanceDomain::Settlement,
            oracle_source: "oracle".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 1_000_000_000,
            title: "Test".to_owned(),
            description: Description::plain("Test Description"),
            icon_url: None,
            banner_url: None,
            trading_access: vec![],
            engine_id: None,
            forked_from: None,
            locale: None,
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
                    reserved_margin_usd: 100_000,
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
            a.set_cash_balance_usd(BalanceDomain::Settlement, -100_000); // -10 USD
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
            assert_eq!(total_net_payoff, 900_000); // 100 USD payoff - 10 USD margin
            assert_eq!(total_collateral_usd, 800_000); // -10 initial + 100 payoff - 10 margin = 80
                                                       // USD
        } else {
            panic!("Expected SolvencyViolation, got {result:?}");
        }

        // Verify position STILL EXISTS (atomicity check)
        POSITIONS.with(|pos: &RefCell<BTreeMap<_, _>>| {
            assert!(pos.borrow().contains_key(&(user, series_id, None)));
        });
    }

    #[test]
    fn settle_series_normal_flow() {
        let user_p = Principal::from_slice(&[1]);
        let user = User(user_p);
        let series_id = SeriesId::from("test_ser".to_owned());

        let series = Series {
            series_id: series_id.clone(),
            underlying: "BTC".to_owned(),
            expiry_ns: 2_000_000_000,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(100, 0)),
            price_precision: 0,
            payout_unit: PayoutUnit::usd(),
            outcomes: None,
            balance_domain: BalanceDomain::Settlement,
            oracle_source: "oracle".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 1_000_000_000,
            title: "Test".to_owned(),
            description: Description::plain("Test Description"),
            icon_url: None,
            banner_url: None,
            trading_access: vec![],
            engine_id: None,
            forked_from: None,
            locale: None,
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
                    reserved_margin_usd: 100_000,
                },
            );
        });

        // Set total system equity to 1000 USD
        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            acc.clear();
            let mut a = AccountState::new(user);
            a.set_cash_balance_usd(BalanceDomain::Settlement, 10_000_000);
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
        // fees are in USD base units (`USD_DECIMALS`).
        assert_eq!(plan.insurance_fee_usd, 500);
        assert_eq!(plan.fee_usd, 250);
        assert_eq!(
            plan.positions[0].cashflow_usd,
            500_000 - 100_000 - 500 - 250
        );

        // Verify position WAS REMOVED
        POSITIONS.with(|pos: &RefCell<BTreeMap<_, _>>| {
            assert!(!pos.borrow().contains_key(&(user, series_id, None)));
        });
    }

    #[test]
    fn get_settlement_plan_returns_active_plan() {
        let series_id = SeriesId::from("plan_query_test".to_owned());
        let price = Price::new(100, 0);

        // Insert a plan directly
        let _plan = SettlementPlan::get_or_create(SettlementPlanParams {
            series_id: series_id.clone(),
            settlement: SettlementInput::Price(price.clone()),
            oracle_source: "test_oracle".to_owned(),
            fee: 1000,
            insurance_fee: 500,
            positions: vec![],
            balance_domain: BalanceDomain::Settlement,
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
        let missing = get_settlement_plan(SeriesId::from("nonexistent".to_owned()));
        assert!(missing.is_none());

        // Cleanup
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().remove(&series_id));
    }

    #[test]
    fn chunked_accounting_processes_in_batches_of_100() {
        let series_id = SeriesId::from("chunk_test".to_owned());
        let num_positions: u32 = 150;

        // Create 150 users with account states
        let users: Vec<User> = (0..num_positions)
            .map(|i| {
                let bytes = i.to_be_bytes();
                User(Principal::from_slice(&bytes))
            })
            .collect();

        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            for user in &users {
                let mut a = AccountState::new(*user);
                a.set_cash_balance_usd(BalanceDomain::Settlement, 10_000); // 1 USD
                a.set_reserved_margin_usd(BalanceDomain::Settlement, 5_000); // 0.5 USD
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
                reserved_margin_usd: 5_000,
                cashflow_usd: 1_000, // +0.1 USD each
            })
            .collect();

        // Create a plan with 150 positions
        let mut plan = SettlementPlan::get_or_create(SettlementPlanParams {
            series_id: series_id.clone(),
            settlement: SettlementInput::Price(Price::new(100, 0)),
            oracle_source: "test".to_owned(),
            fee: 0,
            insurance_fee: 0,
            positions,
            balance_domain: BalanceDomain::Settlement,
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
                    let domain = plan.balance_domain;
                    let curr_cash = account.get_cash_balance_usd(domain);
                    account.set_cash_balance_usd(domain, curr_cash + pos.cashflow_usd);

                    let curr_margin = account.get_reserved_margin_usd(domain);
                    account.set_reserved_margin_usd(
                        domain,
                        curr_margin.saturating_sub(pos.reserved_margin_usd),
                    );
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
            let domain = BalanceDomain::Settlement;
            assert_eq!(state.get_cash_balance_usd(domain), 11_000); // 1.0 + 0.1
            assert_eq!(state.get_reserved_margin_usd(domain), 0); // released

            // User 100 should NOT be updated yet
            let user_100 = &users[100];
            let state_100 = acc.get(user_100).unwrap();
            assert_eq!(state_100.get_cash_balance_usd(domain), 10_000); // unchanged
            assert_eq!(state_100.get_reserved_margin_usd(domain), 5_000); // unchanged
        });

        // --- Simulate Phase B: second chunk (should process remaining 50) ---
        ACCOUNT_STATES.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            while plan.accounting_cursor < plan.positions.len() {
                let idx = plan.accounting_cursor;
                let pos = &plan.positions[idx];
                if let Some(account) = accounts.get_mut(&pos.user) {
                    let domain = plan.balance_domain;
                    let curr_cash = account.get_cash_balance_usd(domain);
                    account.set_cash_balance_usd(domain, curr_cash + pos.cashflow_usd);

                    let curr_margin = account.get_reserved_margin_usd(domain);
                    account.set_reserved_margin_usd(
                        domain,
                        curr_margin.saturating_sub(pos.reserved_margin_usd),
                    );
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
            let domain = BalanceDomain::Settlement;
            assert_eq!(state.get_cash_balance_usd(domain), 11_000); // now updated
            assert_eq!(state.get_reserved_margin_usd(domain), 0); // released
        });

        // Cleanup
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().remove(&series_id));
        ACCOUNT_STATES.with(|acc| acc.borrow_mut().clear());
    }

    #[test]
    fn settlement_price_immutability() {
        let series_id = SeriesId::from("immutable_price_test".to_owned());

        // Create a plan with price 100
        let plan = SettlementPlan::get_or_create(SettlementPlanParams {
            series_id: series_id.clone(),
            settlement: SettlementInput::Price(Price::new(100, 0)),
            oracle_source: "oracle".to_owned(),
            fee: 0,
            insurance_fee: 0,
            positions: vec![],
            balance_domain: BalanceDomain::Settlement,
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
            oracle_source: "oracle".to_owned(),
            fee: 999,
            insurance_fee: 999,
            positions: vec![],
            balance_domain: BalanceDomain::Settlement,
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
    fn solvency_check_uses_net_payoffs() {
        let user_p = Principal::from_slice(&[42]);
        let user = User(user_p);
        let series_id = SeriesId::from("net_payoff_test".to_owned());

        let series = Series {
            series_id: series_id.clone(),
            underlying: "BTC".to_owned(),
            expiry_ns: 2_000_000_000,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(100, 0)),
            price_precision: 0,
            payout_unit: PayoutUnit::usd(),
            oracle_source: "oracle".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 1_000_000_000,
            title: "Test".to_owned(),
            description: Description::plain("Net payoff solvency test"),
            outcomes: None,
            icon_url: None,
            banner_url: None,
            balance_domain: BalanceDomain::Settlement,
            trading_access: vec![],
            engine_id: None,
            forked_from: None,
            locale: None,
        };

        // Settlement price 200 → gross payoff = 200 - 100 = 100 USD (4-decimal units)
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
                    reserved_margin_usd: 100_000,
                },
            );
        });

        // System equity = 99.9 USD (less than 100 gross, more than 98 net)
        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            acc.clear();
            let mut a = AccountState::new(user);
            a.set_cash_balance_usd(BalanceDomain::Settlement, 999_000);
            acc.insert(user, a);
        });

        COLLATERAL_ASSETS.with(|c| c.borrow_mut().clear());

        // Fee ratios: insurance 100 bps (1%) + protocol 100 bps (1%) = 2% total
        // Gross payoff = 1_000_000. Fees = 20_000. Net = 980_000.
        // 980_000 < 999_000 → solvency check passes.
        let result = prepare_settlement_impl(
            &series,
            &series_id,
            &SettlementInput::Price(settlement_price),
            100,
            100,
        );

        assert!(
            result.is_ok(),
            "Expected settlement to succeed (net payoff < equity), got: {result:?}"
        );

        let plan = result.unwrap();
        // Verify fees and net cashflow
        assert_eq!(plan.insurance_fee_usd, 10_000); // 1% of 1_000_000
        assert_eq!(plan.fee_usd, 10_000); // 1% of 1_000_000
        assert_eq!(
            plan.positions[0].cashflow_usd,
            1_000_000 - 100_000 - 10_000 - 10_000 // Payoff - Margin - Fees = 880,000
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
    fn solvency_check_accounts_for_post_settlement_clamping() {
        let winner_p = Principal::from_slice(&[10]);
        let winner = User(winner_p);
        let loser_p = Principal::from_slice(&[11]);
        let loser = User(loser_p);

        // Set up account states: winner = 10 USD, loser = 100 USD
        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            acc.clear();
            let mut w = AccountState::new(winner);
            w.set_cash_balance_usd(BalanceDomain::Settlement, 100_000); // 10 USD
            acc.insert(winner, w);
            let mut l = AccountState::new(loser);
            l.set_cash_balance_usd(BalanceDomain::Settlement, 1_000_000); // 100 USD
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
                reserved_margin_usd: 50_000,
                cashflow_usd: 950_000, // +95 USD
            },
            SettlementPosition {
                user: loser,
                outcome_id: None,
                net_qty: -1,
                reserved_margin_usd: 500_000,
                cashflow_usd: -2_000_000, // -200 USD
            },
        ];

        // Net payoff = 106 USD (only positive cashflows count, but we test the
        // solvency threshold directly).
        // 106 > 105 (post-settlement) but 106 < 110 (pre-settlement).
        let result = check_settlement_solvency(BalanceDomain::Settlement, 1_060_000, &positions);

        assert!(
            result.is_err(),
            "Expected SolvencyViolation because post-settlement equity (105) < net payoff (106)"
        );
        if let Err(SettlementError::SolvencyViolation {
            total_net_payoff,
            total_collateral_usd,
        }) = result
        {
            assert_eq!(total_net_payoff, 1_060_000);
            assert_eq!(total_collateral_usd, 1_050_000); // post-settlement clamped equity
        } else {
            panic!("Expected SolvencyViolation");
        }

        // Also verify that a payoff within post-settlement equity passes.
        let result_ok = check_settlement_solvency(BalanceDomain::Settlement, 1_050_000, &positions);
        assert!(
            result_ok.is_ok(),
            "Expected success when net payoff equals post-settlement equity"
        );

        // Cleanup
        ACCOUNT_STATES.with(|acc| acc.borrow_mut().clear());
    }

    /// The structured `InconsistentSettlement` variant preserves both the
    /// originally-locked settlement input and the conflicting attempt, so a
    /// second `settle_series` call with a different price or outcome does not
    /// lose information the way the previous generic `InvalidInput` did.
    #[test]
    fn inconsistent_settlement_variant_preserves_both_inputs() {
        use crate::api::settlement::errors::SettlementError;

        let original = SettlementInput::Price(Price::new(100, 0));
        let requested = SettlementInput::Outcome("YES".to_owned().into());

        let err = SettlementError::InconsistentSettlement {
            existing: Box::new(original.clone()),
            requested: Box::new(requested.clone()),
        };

        match err {
            SettlementError::InconsistentSettlement {
                existing,
                requested: req,
            } => {
                assert_eq!(*existing, original);
                assert_eq!(*req, requested);
            }
            other => panic!("expected InconsistentSettlement, got {other:?}"),
        }
    }

    /// `prepare_settlement_impl` must cancel every open limit order on the
    /// series being settled and refund the blocked margin to each maker, so
    /// that a settled series leaves no live book and no stuck collateral.
    #[test]
    fn prepare_settlement_cancels_open_limit_orders_and_refunds_margin() {
        use shared::types::OutcomeId;

        use crate::{
            memory::LIMIT_ORDERS,
            types::trade::{LimitOrder, OrderId, Side},
        };

        let maker_p = Principal::from_slice(&[7]);
        let maker = User(maker_p);
        let other_maker_p = Principal::from_slice(&[8]);
        let other_maker = User(other_maker_p);
        let series_id = SeriesId::from("cleanup_test".to_owned());
        let other_series_id = SeriesId::from("untouched_series".to_owned());

        let series = Series {
            series_id: series_id.clone(),
            underlying: "BTC".to_owned(),
            expiry_ns: 2_000_000_000,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(100, 0)),
            price_precision: 0,
            payout_unit: PayoutUnit::usd(),
            outcomes: None,
            balance_domain: BalanceDomain::Settlement,
            oracle_source: "oracle".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 1_000_000_000,
            title: "Test".to_owned(),
            description: Description::plain("Cleanup test"),
            icon_url: None,
            banner_url: None,
            trading_access: vec![],
            engine_id: None,
            forked_from: None,
            locale: None,
        };

        POSITIONS.with(|pos| pos.borrow_mut().clear());

        // Seed two orders on the series under settlement and one order on an
        // unrelated series that MUST be left alone.
        let order_ids = [
            (
                OrderId::from("o1".to_owned()),
                maker,
                series_id.clone(),
                40_000_u128,
            ),
            (
                OrderId::from("o2".to_owned()),
                maker,
                series_id.clone(),
                60_000_u128,
            ),
            (
                OrderId::from("o3".to_owned()),
                other_maker,
                other_series_id.clone(),
                25_000_u128,
            ),
        ];

        LIMIT_ORDERS.with(|orders| {
            let mut orders = orders.borrow_mut();
            orders.clear();
            for (id, creator, sid, blocked) in &order_ids {
                orders.insert(
                    id.clone(),
                    LimitOrder {
                        order_id: id.clone(),
                        creator: *creator,
                        series_id: sid.clone(),
                        outcome_id: None::<OutcomeId>,
                        side: Side::Buy,
                        qty: 1,
                        price: Price::new(50, 0),
                        blocked_margin_usd: *blocked,
                        balance_domain: BalanceDomain::Settlement,
                    },
                );
            }
        });

        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            acc.clear();
            let mut a = AccountState::new(maker);
            a.set_cash_balance_usd(BalanceDomain::Settlement, 10_000_000);
            // Maker has 100_000 reserved for both orders (40_000 + 60_000).
            a.set_reserved_margin_usd(BalanceDomain::Settlement, 100_000);
            acc.insert(maker, a);

            let mut b = AccountState::new(other_maker);
            b.set_cash_balance_usd(BalanceDomain::Settlement, 10_000_000);
            b.set_reserved_margin_usd(BalanceDomain::Settlement, 25_000);
            acc.insert(other_maker, b);
        });

        let result = prepare_settlement_impl(
            &series,
            &series_id,
            &SettlementInput::Price(Price::new(150, 0)),
            0,
            0,
        );
        assert!(result.is_ok(), "settlement prep failed: {result:?}");

        // Both orders on the settled series are gone; the unrelated order stays.
        LIMIT_ORDERS.with(|orders| {
            let orders = orders.borrow();
            assert!(orders.get(&OrderId::from("o1".to_owned())).is_none());
            assert!(orders.get(&OrderId::from("o2".to_owned())).is_none());
            assert!(orders.get(&OrderId::from("o3".to_owned())).is_some());
        });

        // Maker's reserved margin is refunded (100_000 → 0). The other maker
        // on the untouched series keeps their reservation.
        ACCOUNT_STATES.with(|acc| {
            let acc = acc.borrow();
            assert_eq!(
                acc.get(&maker)
                    .unwrap()
                    .get_reserved_margin_usd(BalanceDomain::Settlement),
                0,
                "maker's margin should be fully released after cancellation"
            );
            assert_eq!(
                acc.get(&other_maker)
                    .unwrap()
                    .get_reserved_margin_usd(BalanceDomain::Settlement),
                25_000,
                "unrelated series' maker should not be touched"
            );
        });

        // Cleanup
        LIMIT_ORDERS.with(|orders| orders.borrow_mut().clear());
        ACCOUNT_STATES.with(|acc| acc.borrow_mut().clear());
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().remove(&series_id));
    }

    /// Backfill emits one Settled event per finalized-plan position and is
    /// idempotent on re-run. Non-finalized plans are skipped. Synthesized
    /// timestamps reflect the plan's original `idempotency_ns`.
    #[test]
    fn backfill_settlement_events_emits_and_is_idempotent() {
        use crate::{
            api::settlement::{backfill_settlement_events, params::BackfillSettlementEventsParams},
            memory::EVENTS,
            types::event::EventType,
        };

        // Reset shared state to a known baseline so the test is hermetic.
        EVENTS.with(|e| e.borrow_mut().clear());
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().clear());

        let series_finalised = SeriesId::from("backfill_finalised".to_owned());
        let series_executing = SeriesId::from("backfill_executing".to_owned());

        let users: Vec<User> = (0..3_u32)
            .map(|i| User(Principal::from_slice(&i.to_be_bytes())))
            .collect();

        let positions: Vec<SettlementPosition> = users
            .iter()
            .enumerate()
            .map(|(i, u)| SettlementPosition {
                user: *u,
                outcome_id: None,
                net_qty: 1,
                reserved_margin_usd: 1_000,
                // Mix of winners and losers so the test exercises the signed
                // qty semantic — first two positive, last one negative.
                cashflow_usd: if i < 2 { 5_000 } else { -3_000 },
            })
            .collect();

        // Two plans: one Finalised (should be backfilled), one Executing
        // (should be skipped — its events will be emitted naturally when
        // accounting completes).
        let finalised_plan = SettlementPlan {
            series_id: series_finalised.clone(),
            settlement: SettlementInput::Outcome("YES".to_owned().into()),
            oracle_source: "test".to_owned(),
            fee_usd: 0,
            insurance_fee_usd: 0,
            positions: positions.clone(),
            accounting_cursor: positions.len(),
            accounting_applied: true,
            status: PlanStatus::Finalised,
            idempotency_ns: 1_700_000_000_000_000_000_u64.into(),
            balance_domain: BalanceDomain::Settlement,
        };

        let executing_plan = SettlementPlan {
            series_id: series_executing.clone(),
            settlement: SettlementInput::Outcome("NO".to_owned().into()),
            oracle_source: "test".to_owned(),
            fee_usd: 0,
            insurance_fee_usd: 0,
            positions: positions.clone(),
            accounting_cursor: 0,
            accounting_applied: false,
            status: PlanStatus::Executing,
            idempotency_ns: 1_700_000_000_000_000_000_u64.into(),
            balance_domain: BalanceDomain::Settlement,
        };

        SETTLEMENT_PLANS.with(|m| {
            let mut m = m.borrow_mut();
            m.insert(series_finalised.clone(), finalised_plan);
            m.insert(series_executing.clone(), executing_plan);
        });

        let first = backfill_settlement_events(BackfillSettlementEventsParams::default());
        assert_eq!(first.plans_scanned, 1, "only the Finalised plan is visited");
        assert_eq!(
            first.events_emitted, 3,
            "one Settled event per SettlementPosition"
        );
        assert_eq!(first.events_skipped, 0);
        assert!(first.next_cursor.is_none(), "single chunk completes");

        // Verify the events actually landed in EVENTS with the right shape.
        let emitted: Vec<_> = EVENTS.with(|e| {
            e.borrow()
                .iter()
                .filter(|ev| {
                    matches!(ev.event_type, EventType::Settled) && ev.series_id == series_finalised
                })
                .cloned()
                .collect()
        });
        assert_eq!(emitted.len(), 3);
        assert!(emitted
            .iter()
            .all(|e| e.timestamp == 1_700_000_000_000_000_000));
        // Two winners, one loser — qty carries signed cashflow.
        let winners = emitted.iter().filter(|e| e.qty > 0).count();
        let losers = emitted.iter().filter(|e| e.qty < 0).count();
        assert_eq!(winners, 2);
        assert_eq!(losers, 1);

        // Idempotency: second invocation emits nothing and skips all 3.
        let second = backfill_settlement_events(BackfillSettlementEventsParams::default());
        assert_eq!(second.plans_scanned, 1);
        assert_eq!(second.events_emitted, 0);
        assert_eq!(second.events_skipped, 3);

        // Cleanup
        EVENTS.with(|e| e.borrow_mut().clear());
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().clear());
    }

    /// Seeds an empty-positions settlement plan for `series_id` in `domain`, so
    /// the series is registered as settled/resolved.
    fn seed_settled(series_id: &str, domain: BalanceDomain) {
        let _ = SettlementPlan::get_or_create(SettlementPlanParams {
            series_id: SeriesId::from(series_id.to_owned()),
            settlement: SettlementInput::Price(Price::new(1, 0)),
            oracle_source: "oracle".to_owned(),
            fee: 0,
            insurance_fee: 0,
            positions: vec![],
            balance_domain: domain,
        });
    }

    #[test]
    fn list_settled_series_returns_ids_ascending_and_filters_by_domain() {
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().clear());

        // Insert out of order to prove the BTreeMap yields ascending ids.
        seed_settled("s_c", BalanceDomain::Settlement);
        seed_settled("s_a", BalanceDomain::ViciXp);
        seed_settled("s_b", BalanceDomain::Settlement);

        // No filter → every settled id, ascending.
        let all = list_settled_series(ListSettledSeriesParams::default());
        assert_eq!(
            all.items,
            vec![
                SeriesId::from("s_a".to_owned()),
                SeriesId::from("s_b".to_owned()),
                SeriesId::from("s_c".to_owned()),
            ]
        );
        assert!(all.next_cursor.is_none());

        // Domain filter → only matching plans.
        let settlement_only = list_settled_series(ListSettledSeriesParams {
            balance_domain: Some(BalanceDomain::Settlement),
            ..Default::default()
        });
        assert_eq!(
            settlement_only.items,
            vec![
                SeriesId::from("s_b".to_owned()),
                SeriesId::from("s_c".to_owned()),
            ]
        );

        SETTLEMENT_PLANS.with(|m| m.borrow_mut().clear());
    }

    #[test]
    fn list_settled_series_paginates_stably() {
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().clear());

        for id in ["s1", "s2", "s3", "s4", "s5"] {
            seed_settled(id, BalanceDomain::Settlement);
        }

        // First page of 2. `next_cursor` is the last id returned (exclusive
        // resume point), set because more ids remain.
        let page1 = list_settled_series(ListSettledSeriesParams {
            limit: Some(2),
            ..Default::default()
        });
        assert_eq!(
            page1.items,
            vec![
                SeriesId::from("s1".to_owned()),
                SeriesId::from("s2".to_owned()),
            ]
        );
        assert_eq!(page1.next_cursor, Some(SeriesId::from("s2".to_owned())));

        // Resume after the cursor → next page, with no id dropped or repeated.
        let page2 = list_settled_series(ListSettledSeriesParams {
            start_after: page1.next_cursor,
            limit: Some(2),
            ..Default::default()
        });
        assert_eq!(
            page2.items,
            vec![
                SeriesId::from("s3".to_owned()),
                SeriesId::from("s4".to_owned()),
            ]
        );
        assert_eq!(page2.next_cursor, Some(SeriesId::from("s4".to_owned())));

        // Final page → last id, no further cursor.
        let page3 = list_settled_series(ListSettledSeriesParams {
            start_after: page2.next_cursor,
            limit: Some(2),
            ..Default::default()
        });
        assert_eq!(page3.items, vec![SeriesId::from("s5".to_owned())]);
        assert!(page3.next_cursor.is_none());

        SETTLEMENT_PLANS.with(|m| m.borrow_mut().clear());
    }

    #[test]
    fn list_settled_series_empty_when_nothing_settled() {
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().clear());

        let page = list_settled_series(ListSettledSeriesParams::default());
        assert!(page.items.is_empty());
        assert!(page.next_cursor.is_none());
    }
}
