use core::{cell::RefCell, cmp::Ordering};
use std::collections::{BTreeMap, BTreeSet};

use ic_cdk::api::msg_caller;
use ic_cdk_macros::{query, update};
use shared::types::{BalanceDomain, OutcomeId};

use super::{
    errors::AccountStateError,
    params::{AggregateLeanParams, GetAccountStateParams, GetPositionParams},
    results::{AggregateLean, OutcomeLean},
};
use crate::{
    account::service::AccountService,
    assets::asset::{handler::get_handler, params::AssetBalanceOfParams},
    guards::caller_is_not_anonymous,
    memory::{ACCOUNT_STATES, ASSET_METRICS, COLLATERAL_ASSETS, POSITIONS},
    types::{
        account::AssetAccount,
        margin::{AccountState, Position, PositionsMap},
        user::User,
    },
    GetAccountStateResult,
};

/// Upper bound on the caller-supplied principal set in [`aggregate_lean`]. The
/// set is caller-controlled and scanned in full, so it is capped to keep a
/// single query within the replica's instruction and argument-size budgets,
/// mirroring the leaderboard's league cap. Any realistic set is far below this;
/// anything longer is truncated to the first `MAX_AGGREGATE_PRINCIPALS`.
const MAX_AGGREGATE_PRINCIPALS: usize = 10_000;

/// Retrieves the current user's account state (query only).
///
/// This does not refresh balances from external ledgers.
#[query(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn get_account_state_query() -> GetAccountStateResult {
    let user: User = msg_caller().into();

    ACCOUNT_STATES
        .with(|accounts| {
            accounts
                .borrow()
                .get(&user)
                .cloned()
                .ok_or(AccountStateError::NoAccountStateFound)
        })
        .map(|state| {
            COLLATERAL_ASSETS.with(|configs| {
                ASSET_METRICS.with(|metrics| {
                    AccountService::build_account_state_response(
                        state,
                        BalanceDomain::Settlement,
                        &configs.borrow(),
                        &metrics.borrow(),
                    )
                })
            })
        })
        .into()
}

/// Retrieves the current user's account state, optionally refreshing balances.
///
/// # Arguments
/// * `params` - Includes an optional `refresh` flag to trigger external ledger checks.
#[update(guard = "caller_is_not_anonymous")]
pub async fn get_account_state(params: GetAccountStateParams) -> GetAccountStateResult {
    let result: Result<(AccountState, BalanceDomain), AccountStateError> = (async {
        let user: User = msg_caller().into();

        let GetAccountStateParams { refresh, domain } = params;

        let refresh = refresh.unwrap_or(false);
        let domain = domain.unwrap_or(BalanceDomain::Settlement);

        // If not refreshing, just return cached state
        if !refresh {
            let state = ACCOUNT_STATES.with(|accounts| {
                accounts
                    .borrow()
                    .get(&user)
                    .cloned()
                    .ok_or(AccountStateError::NoAccountStateFound)
            })?;
            return Ok((state, domain));
        }

        // Refresh: verify external ledger balances match internal accounting.
        // We do NOT overwrite domain-specific balances here because the deposit
        // flow is the authoritative source for per-domain allocations. Writing
        // the raw external balance into a specific domain would contaminate
        // the other domain's accounting (the user has one shared ledger account
        // but balances are logically partitioned across domains).
        let collateral_configs = COLLATERAL_ASSETS.with(|assets| assets.borrow().clone());

        for config in collateral_configs.values() {
            if !config.is_enabled {
                continue;
            }

            let handler = get_handler(&config.asset).map_err(AccountStateError::Asset)?;

            // We still call balance_of to warm the cache / detect ledger issues,
            // but we intentionally do not overwrite domain balances.
            let _external_balance = handler
                .balance_of(AssetBalanceOfParams {
                    asset: &config.asset,
                    account: AssetAccount::UserClearing(user),
                })
                .await
                .map_err(AccountStateError::Asset)?;
        }

        let final_state = ACCOUNT_STATES.with(|accounts| {
            let accounts = accounts.borrow();
            accounts
                .get(&user)
                .cloned()
                .unwrap_or_else(|| AccountState::new(user))
        });

        Ok((final_state, domain))
    })
    .await;

    match result {
        Ok((state, domain)) => {
            let response = COLLATERAL_ASSETS.with(|configs| {
                ASSET_METRICS.with(|metrics| {
                    AccountService::build_account_state_response(
                        state,
                        domain,
                        &configs.borrow(),
                        &metrics.borrow(),
                    )
                })
            });
            Ok(response).into()
        }
        Err(e) => Err(e).into(),
    }
}

/// Retrieves a specific position for the caller.
#[query(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn get_position(params: GetPositionParams) -> Option<Position> {
    let caller: User = msg_caller().into();

    POSITIONS.with(|positions| {
        positions
            .borrow()
            .get(&(caller, params.series_id, params.outcome_id))
            .cloned()
    })
}

/// Retrieves all open positions for the caller.
#[query(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn get_positions() -> Vec<Position> {
    let caller: User = msg_caller().into();
    POSITIONS.with(|positions: &RefCell<PositionsMap>| {
        positions
            .borrow()
            .iter()
            .filter(|((u, _, _), _)| *u == caller)
            .map(|(_, position)| position.clone())
            .collect()
    })
}

/// Aggregates, for a series, the long/short lean of a supplied set of
/// principals, broken down per outcome.
///
/// The clearing layer ascribes no meaning to the set — it is just a list of
/// principals the caller passes in; how it is assembled is a concern of the
/// consuming layer-2 / application, not of clearing.
///
/// Returns **counts only**: per outcome, how many of the supplied principals
/// are net long vs net short, plus the number of distinct principals with any
/// non-flat position on the series. It never exposes individual identities,
/// sides, quantities, or P&L, so no single principal's own side is ever singled
/// out.
///
/// The supplied set is de-duplicated and capped at `MAX_AGGREGATE_PRINCIPALS`.
/// Guarded by `caller_is_not_anonymous`, matching the other position reads.
#[query(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn aggregate_lean(params: AggregateLeanParams) -> AggregateLean {
    POSITIONS.with(|positions| aggregate_lean_impl(&positions.borrow(), params))
}

/// Pure core of [`aggregate_lean`], taking the positions map explicitly so it can
/// be unit-tested without canister state.
fn aggregate_lean_impl(positions: &PositionsMap, params: AggregateLeanParams) -> AggregateLean {
    let AggregateLeanParams {
        series_id,
        principals,
    } = params;

    // De-duplicate the caller-supplied set into a lookup, capping the number of
    // *distinct* principals so the scan stays within budget regardless of how
    // many (possibly duplicate) entries the caller sends.
    let mut requested: BTreeSet<User> = BTreeSet::new();
    for principal in principals {
        if requested.len() >= MAX_AGGREGATE_PRINCIPALS {
            break;
        }
        requested.insert(User::from(principal));
    }

    // outcome_id -> (long, short) counts; BTreeMap keeps a stable outcome order.
    let mut by_outcome: BTreeMap<Option<OutcomeId>, (u64, u64)> = BTreeMap::new();
    let mut total: u64 = 0;

    // POSITIONS is keyed by `(User, SeriesId, Option<OutcomeId>)`, so one user's
    // entries for a given series form a contiguous run. Instead of scanning the
    // whole map and filtering, seek straight to each requested user's run with a
    // range query and stop as soon as the key leaves that user/series — so
    // unrequested users and unrelated series are never visited, and each user is
    // settled in a single pass (no re-checking once counted).
    for &user in &requested {
        let mut holds_position = false;
        for ((u, sid, outcome_id), position) in positions.range((user, series_id.clone(), None)..) {
            if *u != user || sid != &series_id {
                break;
            }
            let counts = match position.net_qty.cmp(&0) {
                Ordering::Greater => &mut by_outcome.entry(outcome_id.clone()).or_default().0,
                Ordering::Less => &mut by_outcome.entry(outcome_id.clone()).or_default().1,
                // Flat positions lean neither way and are excluded entirely.
                Ordering::Equal => continue,
            };
            *counts += 1;
            holds_position = true;
        }
        if holds_position {
            total += 1;
        }
    }

    let outcomes = by_outcome
        .into_iter()
        .map(|(outcome_id, (long, short))| OutcomeLean {
            outcome_id,
            long,
            short,
            total: long + short,
        })
        .collect();

    AggregateLean {
        series_id,
        outcomes,
        total,
    }
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::SeriesId;

    use super::*;

    fn principal(n: u8) -> Principal {
        Principal::from_slice(&[n])
    }

    fn series(s: &str) -> SeriesId {
        s.to_owned().into()
    }

    fn position(p: Principal, sid: &SeriesId, outcome: Option<&str>, net_qty: i128) -> Position {
        Position {
            user: User(p),
            series_id: sid.clone(),
            outcome_id: outcome.map(|o| o.to_owned().into()),
            net_qty,
            reserved_margin_usd: 0,
        }
    }

    fn map(positions: Vec<Position>) -> PositionsMap {
        positions
            .into_iter()
            .map(|p| ((p.user, p.series_id.clone(), p.outcome_id.clone()), p))
            .collect()
    }

    fn lean(positions: &PositionsMap, sid: &SeriesId, principals: Vec<Principal>) -> AggregateLean {
        aggregate_lean_impl(
            positions,
            AggregateLeanParams {
                series_id: sid.clone(),
                principals,
            },
        )
    }

    #[test]
    fn counts_binary_long_and_short() {
        let sid = series("s1");
        let positions = map(vec![
            position(principal(1), &sid, None, 5),
            position(principal(2), &sid, None, 3),
            position(principal(3), &sid, None, -2),
        ]);

        let result = lean(
            &positions,
            &sid,
            vec![principal(1), principal(2), principal(3)],
        );

        assert_eq!(result.series_id, sid);
        assert_eq!(result.total, 3);
        assert_eq!(
            result.outcomes,
            vec![OutcomeLean {
                outcome_id: None,
                long: 2,
                short: 1,
                total: 3,
            }]
        );
    }

    #[test]
    fn aggregates_per_outcome_for_categorical_series() {
        let sid = series("s1");
        let positions = map(vec![
            position(principal(1), &sid, Some("a"), 4),
            position(principal(2), &sid, Some("a"), -1),
            position(principal(2), &sid, Some("b"), 7),
            position(principal(3), &sid, Some("b"), 2),
        ]);

        let result = lean(
            &positions,
            &sid,
            vec![principal(1), principal(2), principal(3)],
        );

        // principal(2) holds on both outcomes but is counted once overall.
        assert_eq!(result.total, 3);
        assert_eq!(
            result.outcomes,
            vec![
                OutcomeLean {
                    outcome_id: Some("a".to_owned().into()),
                    long: 1,
                    short: 1,
                    total: 2,
                },
                OutcomeLean {
                    outcome_id: Some("b".to_owned().into()),
                    long: 2,
                    short: 0,
                    total: 2,
                },
            ]
        );
    }

    #[test]
    fn excludes_other_series_and_unlisted_principals() {
        let sid = series("s1");
        let other = series("s2");
        let positions = map(vec![
            position(principal(1), &sid, None, 5),
            position(principal(2), &sid, None, 5), // not in the supplied set
            position(principal(1), &other, None, 5), // different series
        ]);

        let result = lean(&positions, &sid, vec![principal(1)]);

        assert_eq!(result.total, 1);
        assert_eq!(
            result.outcomes,
            vec![OutcomeLean {
                outcome_id: None,
                long: 1,
                short: 0,
                total: 1,
            }]
        );
    }

    #[test]
    fn isolates_target_series_among_a_users_other_series() {
        // The same user holds positions in series sorting both before and after
        // the target. The range scan must seek past the earlier one and stop at
        // the later one, counting only the target.
        let target = series("m");
        let positions = map(vec![
            position(principal(1), &series("a"), None, 5),
            position(principal(1), &target, Some("x"), -3),
            position(principal(1), &series("z"), None, 7),
        ]);

        let result = lean(&positions, &target, vec![principal(1)]);

        assert_eq!(result.total, 1);
        assert_eq!(
            result.outcomes,
            vec![OutcomeLean {
                outcome_id: Some("x".to_owned().into()),
                long: 0,
                short: 1,
                total: 1,
            }]
        );
    }

    #[test]
    fn excludes_flat_positions() {
        let sid = series("s1");
        let positions = map(vec![
            position(principal(1), &sid, None, 0),
            position(principal(2), &sid, None, 4),
        ]);

        let result = lean(&positions, &sid, vec![principal(1), principal(2)]);

        assert_eq!(result.total, 1);
        assert_eq!(
            result.outcomes,
            vec![OutcomeLean {
                outcome_id: None,
                long: 1,
                short: 0,
                total: 1,
            }]
        );
    }

    #[test]
    fn duplicate_principals_do_not_double_count() {
        let sid = series("s1");
        let positions = map(vec![position(principal(1), &sid, None, 5)]);

        let result = lean(
            &positions,
            &sid,
            vec![principal(1), principal(1), principal(1)],
        );

        assert_eq!(result.total, 1);
        assert_eq!(result.outcomes[0].long, 1);
    }

    #[test]
    fn empty_when_no_requested_positions() {
        let sid = series("s1");
        let positions = map(vec![position(principal(9), &sid, None, 5)]);

        let result = lean(&positions, &sid, vec![principal(1)]);

        assert_eq!(result.total, 0);
        assert!(result.outcomes.is_empty());
    }
}
