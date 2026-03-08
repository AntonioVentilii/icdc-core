use std::collections::BTreeMap;

use ic_cdk_macros::{query, update};
use shared::types::{Asset, SeriesId};

use super::{
    errors::MarginAccountError,
    params::{GetMarginAccountParams, GetPositionParams},
};
use crate::{
    assets::asset::{handler::get_handler, params::AssetBalanceOfParams},
    guards::caller_is_not_anonymous,
    memory::{MARGIN_ACCOUNTS, POSITIONS},
    types::{
        account::AssetAccount,
        margin::{MarginAccount, Position},
        user::User,
    },
    GetMarginAccountResult,
};

/// Retrieves the current user's margin account details (query only).
///
/// This does not refresh balances from external ledgers.
#[query(guard = "caller_is_not_anonymous")]
pub fn get_margin_account_query() -> GetMarginAccountResult {
    let result: Result<MarginAccount, MarginAccountError> = {
        let user: User = ic_cdk::caller().into();

        MARGIN_ACCOUNTS.with(|accounts| {
            accounts
                .borrow()
                .get(&user)
                .cloned()
                .ok_or(MarginAccountError::NoMarginAccountFound)
        })
    };

    result.into()
}

/// Retrieves the current user's margin account details, optionally refreshing balances.
///
/// # Arguments
/// * `params` - Includes an optional `refresh` flag to trigger external ledger checks.
#[update(guard = "caller_is_not_anonymous")]
pub async fn get_margin_account(params: GetMarginAccountParams) -> GetMarginAccountResult {
    let result: Result<MarginAccount, MarginAccountError> = (async {
        let user: User = ic_cdk::caller().into();

        let GetMarginAccountParams { refresh } = params;

        let refresh = refresh.unwrap_or(false);

        // Always read required_margin from internal state (risk state)
        let (required_margin_u128, reserved_balances) = MARGIN_ACCOUNTS.with(|accounts| {
            accounts
                .borrow()
                .get(&user)
                .map(|m| (m.required_margin, m.reserved_balances.clone()))
                .unwrap_or((0, BTreeMap::new()))
        });

        // If not refreshing, just return cached state (no await)
        if !refresh {
            return MARGIN_ACCOUNTS.with(|accounts| {
                accounts
                    .borrow()
                    .get(&user)
                    .cloned()
                    .ok_or(MarginAccountError::NoMarginAccountFound)
            });
        }

        // Refresh balances from ledgers (await)
        let assets_to_refresh = MARGIN_ACCOUNTS.with(|accounts| {
            accounts
                .borrow()
                .get(&user)
                .map(|m| m.tracked_assets())
                .unwrap_or_default()
        });

        let mut balances: BTreeMap<Asset, u128> = BTreeMap::new();

        for asset in assets_to_refresh.iter().cloned() {
            let handler = get_handler(&asset).map_err(MarginAccountError::Asset)?;

            let bal_u128 = handler
                .balance_of(AssetBalanceOfParams {
                    asset: &asset,
                    account: AssetAccount::UserClearing(user),
                })
                .await
                .map_err(MarginAccountError::Asset)?;

            balances.insert(asset, bal_u128);
        }

        // Persist refreshed balances, but do NOT overwrite required_margin
        MARGIN_ACCOUNTS.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            let acct = accounts.entry(user).or_insert(MarginAccount {
                user,
                balances: BTreeMap::new(),
                reserved_balances: BTreeMap::new(),
                required_margin: 0,
            });

            acct.balances = balances.clone();
            acct.required_margin = required_margin_u128;
        });

        Ok(MarginAccount {
            user,
            balances,
            reserved_balances,
            required_margin: required_margin_u128,
        })
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
