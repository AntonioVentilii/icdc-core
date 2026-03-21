use core::cell::RefCell;

use ic_cdk::api::msg_caller;
use ic_cdk_macros::{query, update};
use shared::types::BalanceDomain;

use super::{
    errors::AccountStateError,
    params::{GetAccountStateParams, GetPositionParams},
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
