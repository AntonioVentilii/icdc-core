use std::collections::BTreeMap;

use ic_cdk_macros::{query, update};
use shared::types::{AssetId, SeriesId};

use super::{
    errors::AccountStateError,
    params::{GetAccountStateParams, GetPositionParams},
};
use crate::{
    assets::asset::{handler::get_handler, params::AssetBalanceOfParams},
    guards::caller_is_not_anonymous,
    memory::{ACCOUNT_STATES, COLLATERAL_ASSETS, POSITIONS},
    types::{
        account::AssetAccount,
        margin::{AccountState, Position},
        user::User,
    },
    GetAccountStateResult,
};

/// Retrieves the current user's account state (query only).
///
/// This does not refresh balances from external ledgers.
#[query(guard = "caller_is_not_anonymous")]
pub fn get_account_state_query() -> GetAccountStateResult {
    let result: Result<AccountState, AccountStateError> = {
        let user: User = ic_cdk::caller().into();

        ACCOUNT_STATES.with(|accounts| {
            accounts
                .borrow()
                .get(&user)
                .cloned()
                .ok_or(AccountStateError::NoAccountStateFound)
        })
    };

    result.into()
}

/// Retrieves the current user's account state, optionally refreshing balances.
///
/// # Arguments
/// * `params` - Includes an optional `refresh` flag to trigger external ledger checks.
#[update(guard = "caller_is_not_anonymous")]
pub async fn get_account_state(params: GetAccountStateParams) -> GetAccountStateResult {
    let result: Result<AccountState, AccountStateError> = (async {
        let user: User = ic_cdk::caller().into();

        let GetAccountStateParams { refresh } = params;

        let refresh = refresh.unwrap_or(false);

        // If not refreshing, just return cached state
        if !refresh {
            return ACCOUNT_STATES.with(|accounts| {
                accounts
                    .borrow()
                    .get(&user)
                    .cloned()
                    .ok_or(AccountStateError::NoAccountStateFound)
            });
        }

        // Refresh balances from ledgers
        let collateral_configs = COLLATERAL_ASSETS.with(|assets| assets.borrow().clone());

        let mut fresh_collateral_balances: BTreeMap<AssetId, u128> = BTreeMap::new();

        for (asset_id, config) in collateral_configs {
            if !config.is_enabled {
                continue;
            }

            let handler = get_handler(&config.asset).map_err(AccountStateError::Asset)?;

            let balance = handler
                .balance_of(AssetBalanceOfParams {
                    asset: &config.asset,
                    account: AssetAccount::UserClearing(user),
                })
                .await
                .map_err(AccountStateError::Asset)?;

            fresh_collateral_balances.insert(asset_id, balance);
        }

        // Update the account state in memory
        let final_state = ACCOUNT_STATES.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            let state = accounts
                .entry(user)
                .or_insert_with(|| AccountState::new(user));

            // Merge refreshed balances for enabled assets into existing balances
            // instead of overwriting the entire map, so that balances for assets
            // that are no longer enabled are not silently discarded.
            for (asset_id, balance) in fresh_collateral_balances {
                state.collateral_balances.insert(asset_id, balance);
            }

            // Note: cash_balance_usd and reserved_margin_usd are updated by other processes
            // (trades, settlement)
            state.clone()
        });

        Ok(final_state)
    })
    .await;

    result.into()
}

/// Retrieves a specific position for the caller.
#[query(guard = "caller_is_not_anonymous")]
pub fn get_position(params: GetPositionParams) -> Option<Position> {
    let caller: User = ic_cdk::caller().into();

    POSITIONS.with(|positions| positions.borrow().get(&(caller, params.series_id)).cloned())
}

/// Retrieves all open positions for the caller.
#[query(guard = "caller_is_not_anonymous")]
pub fn get_positions() -> Vec<(SeriesId, Position)> {
    let caller: User = ic_cdk::caller().into();

    POSITIONS.with(|positions| {
        positions
            .borrow()
            .iter()
            .filter(|((u, _), _)| *u == caller)
            .map(|((_, series_id), position)| (series_id.clone(), position.clone()))
            .collect()
    })
}
