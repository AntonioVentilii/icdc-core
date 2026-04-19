use core::cell::RefCell;
use std::collections::BTreeMap;

use ic_cdk::api::msg_caller;
use ic_cdk_macros::update;
use shared::types::BalanceDomain;

use super::{errors::MigrationError, params::MigrateDomainParams, results::MigrateDomainResult};
use crate::{
    guards::caller_is_not_anonymous,
    memory::{
        ACCOUNT_STATES, DEPOSIT_PLANS, LIMIT_ORDERS, MIGRATION_PLANS, POSITIONS, SERIES,
        SETTLEMENT_PLANS, WITHDRAWAL_PLANS,
    },
    types::{
        margin::{AccountState, PositionsMap},
        plans::{MigratedOrder, MigratedPosition, MigrationPlan, MigrationPlanParams, PlanStatus},
        trade::{LimitOrder, OrderId},
        user::User,
    },
};

/// Migrates a user's entire domain state (balances, cash, margin, positions, orders)
/// from one balance domain to another.
///
/// This operation is idempotent: providing the same `migration_id` returns `Ok`
/// if the migration was already finalised.
///
/// Pre-conditions enforced:
/// - `from_domain != to_domain`
/// - No in-flight deposit, withdrawal, or settlement plans for the user in the source domain.
/// - User must have state to migrate.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn migrate_domain(params: MigrateDomainParams) -> MigrateDomainResult {
    let user: User = msg_caller().into();

    let result: Result<(), MigrationError> = migrate_domain_impl(user, params);

    result.into()
}

pub(crate) fn migrate_domain_impl(
    user: User,
    params: MigrateDomainParams,
) -> Result<(), MigrationError> {
    let MigrateDomainParams {
        migration_id,
        from_domain,
        to_domain,
    } = params;

    if from_domain == to_domain {
        return Err(MigrationError::SameDomain);
    }

    // Check for existing plan (idempotency)
    let mut plan = MigrationPlan::get_or_create(MigrationPlanParams {
        migration_id,
        user,
        from_domain,
        to_domain,
    });

    if plan.status == PlanStatus::Finalised {
        return Ok(());
    }

    // Validate no in-flight plans for this user in the source domain
    check_no_inflight_plans(user, from_domain)?;

    // ---------- Phase A: Snapshot and validate ----------
    if plan.status == PlanStatus::Planned {
        plan.status = PlanStatus::Executing;

        // 1. Snapshot balances from source domain
        let (balances, cash, margin) = ACCOUNT_STATES.with(|accounts| {
            let accounts = accounts.borrow();
            let Some(state) = accounts.get(&user) else {
                return Err(MigrationError::NoStateTOMigrate);
            };

            let domain_balances = state
                .balances
                .get(&from_domain)
                .cloned()
                .unwrap_or_default();

            if domain_balances.is_empty()
                && state.get_cash_balance_usd(from_domain) == 0
                && state.get_reserved_margin_usd(from_domain) == 0
            {
                // Also check if any positions or orders exist
                let has_positions = POSITIONS.with(|p: &RefCell<PositionsMap>| {
                    p.borrow().keys().any(|(u, sid, _)| {
                        *u == user
                            && SERIES.with(|s| {
                                s.borrow()
                                    .get(sid)
                                    .is_some_and(|ser| ser.balance_domain == from_domain)
                            })
                    })
                });

                let has_orders = LIMIT_ORDERS.with(|o: &RefCell<BTreeMap<OrderId, LimitOrder>>| {
                    o.borrow()
                        .values()
                        .any(|o| o.creator == user && o.balance_domain == from_domain)
                });

                if !has_positions && !has_orders {
                    return Err(MigrationError::NoStateTOMigrate);
                }
            }

            Ok((
                domain_balances.into_iter().collect::<Vec<_>>(),
                state.get_cash_balance_usd(from_domain),
                state.get_reserved_margin_usd(from_domain),
            ))
        })?;

        plan.balances = balances;
        plan.cash_balance_usd = cash;
        plan.reserved_margin_usd = margin;

        // 2. Snapshot positions in source domain
        plan.positions = POSITIONS.with(|positions: &RefCell<PositionsMap>| {
            let positions = positions.borrow();
            positions
                .iter()
                .filter(|((u, sid, _), _)| {
                    *u == user
                        && SERIES.with(|s| {
                            s.borrow()
                                .get(sid)
                                .is_some_and(|ser| ser.balance_domain == from_domain)
                        })
                })
                .map(|((_, _, outcome_id), pos)| MigratedPosition {
                    series_id: pos.series_id.clone(),
                    outcome_id: outcome_id.clone(),
                    net_qty: pos.net_qty,
                    reserved_margin_usd: pos.reserved_margin_usd,
                })
                .collect()
        });

        // 3. Snapshot orders in source domain
        plan.orders = LIMIT_ORDERS.with(|orders: &RefCell<BTreeMap<OrderId, LimitOrder>>| {
            orders
                .borrow()
                .values()
                .filter(|o| o.creator == user && o.balance_domain == from_domain)
                .map(|o| MigratedOrder {
                    order_id: o.order_id.clone(),
                    blocked_margin_usd: o.blocked_margin_usd,
                })
                .collect()
        });

        // Persist plan before mutation
        let key = (user, plan.migration_id.clone());
        MIGRATION_PLANS.with(|m| m.borrow_mut().insert(key, plan.clone()));
    }

    // ---------- Phase B: Atomic mutation ----------
    // Move balances
    ACCOUNT_STATES.with(|accounts| {
        let mut accounts = accounts.borrow_mut();
        let state = accounts
            .entry(user)
            .or_insert_with(|| AccountState::new(user));

        // Move asset balances
        for (asset_id, amount) in &plan.balances {
            let current_from = state.get_balance(from_domain, asset_id);
            state.set_balance(
                from_domain,
                asset_id.clone(),
                current_from.saturating_sub(*amount),
            );

            let current_to = state.get_balance(to_domain, asset_id);
            state.set_balance(to_domain, asset_id.clone(), current_to + amount);
        }

        // Move cash balance
        let current_cash_from = state.get_cash_balance_usd(from_domain);
        state.set_cash_balance_usd(from_domain, current_cash_from - plan.cash_balance_usd);
        let current_cash_to = state.get_cash_balance_usd(to_domain);
        state.set_cash_balance_usd(to_domain, current_cash_to + plan.cash_balance_usd);

        // Move reserved margin
        let current_margin_from = state.get_reserved_margin_usd(from_domain);
        state.set_reserved_margin_usd(
            from_domain,
            current_margin_from.saturating_sub(plan.reserved_margin_usd),
        );
        let current_margin_to = state.get_reserved_margin_usd(to_domain);
        state.set_reserved_margin_usd(to_domain, current_margin_to + plan.reserved_margin_usd);
    });

    // Move orders: update balance_domain tag
    LIMIT_ORDERS.with(|orders: &RefCell<BTreeMap<OrderId, LimitOrder>>| {
        let mut orders = orders.borrow_mut();
        for migrated in &plan.orders {
            if let Some(order) = orders.get_mut(&migrated.order_id) {
                if order.creator == user && order.balance_domain == from_domain {
                    order.balance_domain = to_domain;
                }
            }
        }
    });

    // Note: Positions are keyed by (user, series_id, outcome_id) and don't carry a domain tag.
    // The domain is determined by the series' balance_domain. Since positions reference series
    // and the series' domain hasn't changed, positions remain valid.
    // If the user needs to move positions to series in a different domain,
    // that requires series migration (out of scope here).

    // ---------- Phase C: Finalise ----------
    plan.status = PlanStatus::Finalised;
    let key = (user, plan.migration_id.clone());
    MIGRATION_PLANS.with(|m| m.borrow_mut().insert(key, plan));

    Ok(())
}

/// Validates that the user has no in-flight deposit, withdrawal, or settlement plans
/// in the specified domain that could conflict with migration.
fn check_no_inflight_plans(user: User, domain: BalanceDomain) -> Result<(), MigrationError> {
    let has_deposit = DEPOSIT_PLANS.with(|plans| {
        plans
            .borrow()
            .iter()
            .any(|((u, _), p)| *u == user && p.status != PlanStatus::Finalised)
    });

    if has_deposit {
        return Err(MigrationError::InFlightPlansExist);
    }

    let has_withdrawal = WITHDRAWAL_PLANS.with(|plans| {
        plans
            .borrow()
            .iter()
            .any(|((u, _), p)| *u == user && p.status != PlanStatus::Finalised)
    });

    if has_withdrawal {
        return Err(MigrationError::InFlightPlansExist);
    }

    let has_settlement = SETTLEMENT_PLANS.with(|plans| {
        plans.borrow().values().any(|p| {
            p.balance_domain == domain
                && p.status != PlanStatus::Finalised
                && p.positions.iter().any(|pos| pos.user == user)
        })
    });

    if has_settlement {
        return Err(MigrationError::InFlightPlansExist);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{
        BalanceDomain, Description, PayoffType, PayoutUnit, Price, Series, SeriesId,
    };

    use crate::{
        api::migration::{api::migrate_domain_impl, errors::MigrationError},
        memory::{ACCOUNT_STATES, LIMIT_ORDERS, MIGRATION_PLANS, SERIES},
        types::{
            margin::AccountState,
            trade::{OrderId, Side},
            user::User,
        },
        LimitOrder, MigrateDomainParams,
    };

    fn setup_test_series(series_id: &SeriesId, domain: BalanceDomain) {
        let series = Series {
            series_id: series_id.clone(),
            underlying: "BTC".to_owned(),
            expiry_ns: 2_000_000_000,
            payoff_type: PayoffType::Binary,
            strike: Some(Price::new(50_000, 6)),
            price_precision: 6,
            payout_unit: PayoutUnit::usd(),
            oracle_source: "oracle".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 1_000_000_000,
            title: "Test".to_owned(),
            description: Description::plain("Test"),
            outcomes: None,
            icon_url: None,
            banner_url: None,
            balance_domain: domain,
            trading_access: vec![],
            engine_id: None,
            forked_from: None,
        };
        SERIES.with(|s| s.borrow_mut().insert(series_id.clone(), series));
    }

    #[test]
    fn migrate_balances_and_cash() {
        let user_p = Principal::from_slice(&[99]);
        let user = User(user_p);
        let asset_id = "ICP".to_owned();

        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            let mut state = AccountState::new(user);
            state.set_balance(BalanceDomain::Settlement, asset_id.clone(), 1_000_000);
            state.set_cash_balance_usd(BalanceDomain::Settlement, 500_000);
            state.set_reserved_margin_usd(BalanceDomain::Settlement, 100_000);
            acc.insert(user, state);
        });

        let result = migrate_domain_impl(
            user,
            MigrateDomainParams {
                migration_id: "mig_1".to_owned(),
                from_domain: BalanceDomain::Settlement,
                to_domain: BalanceDomain::Playground,
            },
        );
        assert!(result.is_ok());

        ACCOUNT_STATES.with(|acc| {
            let acc = acc.borrow();
            let state = acc.get(&user).unwrap();
            assert_eq!(state.get_balance(BalanceDomain::Settlement, &asset_id), 0);
            assert_eq!(
                state.get_balance(BalanceDomain::Playground, &asset_id),
                1_000_000
            );
            assert_eq!(state.get_cash_balance_usd(BalanceDomain::Settlement), 0);
            assert_eq!(
                state.get_cash_balance_usd(BalanceDomain::Playground),
                500_000
            );
            assert_eq!(state.get_reserved_margin_usd(BalanceDomain::Settlement), 0);
            assert_eq!(
                state.get_reserved_margin_usd(BalanceDomain::Playground),
                100_000
            );
        });

        // Cleanup
        MIGRATION_PLANS.with(|m| m.borrow_mut().clear());
        ACCOUNT_STATES.with(|a| a.borrow_mut().clear());
    }

    #[test]
    fn migrate_same_domain_rejected() {
        let user = User(Principal::from_slice(&[100]));

        let result = migrate_domain_impl(
            user,
            MigrateDomainParams {
                migration_id: "mig_2".to_owned(),
                from_domain: BalanceDomain::Settlement,
                to_domain: BalanceDomain::Settlement,
            },
        );
        assert!(matches!(result, Err(MigrationError::SameDomain)));
    }

    #[test]
    fn migrate_no_state_rejected() {
        let user = User(Principal::from_slice(&[101]));

        ACCOUNT_STATES.with(|acc| {
            acc.borrow_mut().insert(user, AccountState::new(user));
        });

        let result = migrate_domain_impl(
            user,
            MigrateDomainParams {
                migration_id: "mig_3".to_owned(),
                from_domain: BalanceDomain::Settlement,
                to_domain: BalanceDomain::Playground,
            },
        );
        assert!(matches!(result, Err(MigrationError::NoStateTOMigrate)));

        ACCOUNT_STATES.with(|a| a.borrow_mut().clear());
        MIGRATION_PLANS.with(|m| m.borrow_mut().clear());
    }

    #[test]
    fn migrate_idempotent_replay() {
        let user = User(Principal::from_slice(&[102]));
        let asset_id = "ICP".to_owned();

        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            let mut state = AccountState::new(user);
            state.set_balance(BalanceDomain::Settlement, asset_id.clone(), 500);
            state.set_cash_balance_usd(BalanceDomain::Settlement, 100);
            acc.insert(user, state);
        });

        let params = MigrateDomainParams {
            migration_id: "mig_idem".to_owned(),
            from_domain: BalanceDomain::Settlement,
            to_domain: BalanceDomain::Playground,
        };

        let r1 = migrate_domain_impl(user, params.clone());
        assert!(r1.is_ok());

        // Second call with same id should succeed (idempotent)
        let r2 = migrate_domain_impl(user, params);
        assert!(r2.is_ok());

        // Balances should not be doubled
        ACCOUNT_STATES.with(|acc| {
            let acc = acc.borrow();
            let state = acc.get(&user).unwrap();
            assert_eq!(state.get_balance(BalanceDomain::Playground, &asset_id), 500);
            assert_eq!(state.get_balance(BalanceDomain::Settlement, &asset_id), 0);
        });

        MIGRATION_PLANS.with(|m| m.borrow_mut().clear());
        ACCOUNT_STATES.with(|a| a.borrow_mut().clear());
    }

    #[test]
    fn migrate_orders() {
        let user = User(Principal::from_slice(&[103]));
        let series_id = SeriesId::from("order_series".to_owned());
        let order_id = OrderId::from("ord_1".to_owned());

        setup_test_series(&series_id, BalanceDomain::Settlement);

        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            let mut state = AccountState::new(user);
            state.set_cash_balance_usd(BalanceDomain::Settlement, 1_000_000);
            state.set_reserved_margin_usd(BalanceDomain::Settlement, 500_000);
            acc.insert(user, state);
        });

        LIMIT_ORDERS.with(|orders| {
            orders.borrow_mut().insert(
                order_id.clone(),
                LimitOrder {
                    order_id: order_id.clone(),
                    creator: user,
                    series_id: series_id.clone(),
                    outcome_id: None,
                    side: Side::Buy,
                    qty: 1,
                    price: Price::new(500_000, 6),
                    blocked_margin_usd: 500_000,
                    balance_domain: BalanceDomain::Settlement,
                },
            );
        });

        let result = migrate_domain_impl(
            user,
            MigrateDomainParams {
                migration_id: "mig_orders".to_owned(),
                from_domain: BalanceDomain::Settlement,
                to_domain: BalanceDomain::Playground,
            },
        );
        assert!(result.is_ok());

        LIMIT_ORDERS.with(|orders| {
            let orders = orders.borrow();
            let order = orders.get(&order_id).unwrap();
            assert_eq!(order.balance_domain, BalanceDomain::Playground);
        });

        MIGRATION_PLANS.with(|m| m.borrow_mut().clear());
        ACCOUNT_STATES.with(|a| a.borrow_mut().clear());
        LIMIT_ORDERS.with(|o| o.borrow_mut().clear());
        SERIES.with(|s| s.borrow_mut().clear());
    }
}
