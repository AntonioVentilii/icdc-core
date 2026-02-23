use std::collections::BTreeMap;

use candid::Principal;
use ic_cdk_macros::{query, update};
use shared::types::{Asset, SeriesId};

use crate::{
    guards::caller_is_not_anonymous,
    memory::{MARGIN_ACCOUNTS, POSITIONS},
    traits::ClearingAccountExt,
    types::{
        errors::ClearingError,
        margin::{MarginAccount, Position},
        params::GetPositionParams,
        results::GetMarginAccountResult,
        user::User,
    },
};

#[update(guard = "caller_is_not_anonymous")]
pub async fn get_margin_account(user: Principal) -> GetMarginAccountResult {
    let result: Result<MarginAccount, ClearingError> = (async {
        let user: User = user.into();
        let from_account = user.clearing_account();

        let assets_to_refresh = MARGIN_ACCOUNTS.with(|accounts| {
            accounts
                .borrow()
                .get(&user)
                .map(|m| m.tracked_assets())
                .unwrap_or_default()
        });

        let mut balances: BTreeMap<Asset, u128> = BTreeMap::new();

        for asset in assets_to_refresh.iter().cloned() {
            let Asset::Icrc(ledger_id) = asset.clone();

            let (ledger_balance,): (candid::Nat,) =
                ic_cdk::call(ledger_id, "icrc1_balance_of", (from_account,))
                    .await
                    .map_err(|(code, msg)| {
                        ClearingError::FetchingBalanceFailed(format!(
                            "icrc1_balance_of {:?}: {}",
                            code, msg
                        ))
                    })?;

            let bal_u128: u128 = ledger_balance
                .0
                .try_into()
                .map_err(|_| ClearingError::BalanceMathOverflow)?;

            balances.insert(asset, bal_u128);
        }

        MARGIN_ACCOUNTS.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            let acct = accounts.entry(user).or_insert(MarginAccount {
                user,
                balances: BTreeMap::new(),
                required_margin: 0,
            });

            acct.balances = balances.clone();
        });

        MARGIN_ACCOUNTS.with(|accounts| {
            accounts
                .borrow()
                .get(&user)
                .cloned()
                .ok_or(ClearingError::NoMarginAccountFound)
        })
    })
    .await;

    result.into()
}

#[update(guard = "caller_is_not_anonymous")]
pub async fn get_margin_account_fresh(user: Principal) -> GetMarginAccountResult {
    let result: Result<MarginAccount, ClearingError> = (async {
        let user: User = user.into();
        let from_account = user.clearing_account();

        let required_margin_u128 = MARGIN_ACCOUNTS.with(|accounts| {
            accounts
                .borrow()
                .get(&user)
                .map(|m| m.required_margin)
                .unwrap_or(0)
        });

        let assets_to_refresh = MARGIN_ACCOUNTS.with(|accounts| {
            accounts
                .borrow()
                .get(&user)
                .map(|m| m.tracked_assets())
                .unwrap_or_default()
        });

        let mut balances: BTreeMap<Asset, u128> = BTreeMap::new();

        for asset in assets_to_refresh.iter().cloned() {
            let Asset::Icrc(ledger_id) = asset.clone();

            let (ledger_balance,): (candid::Nat,) =
                ic_cdk::call(ledger_id, "icrc1_balance_of", (from_account,))
                    .await
                    .map_err(|(code, msg)| {
                        ClearingError::FetchingBalanceFailed(format!(
                            "icrc1_balance_of {:?}: {}",
                            code, msg
                        ))
                    })?;

            let bal_u128: u128 = ledger_balance
                .0
                .try_into()
                .map_err(|_| ClearingError::BalanceMathOverflow)?;

            balances.insert(asset, bal_u128);
        }

        MARGIN_ACCOUNTS.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            let acct = accounts.entry(user).or_insert(MarginAccount {
                user,
                balances: BTreeMap::new(),
                required_margin: 0,
            });

            acct.balances = balances.clone();
        });

        Ok(MarginAccount {
            user,
            balances,
            required_margin: required_margin_u128,
        })
    })
    .await;

    result.into()
}

#[query]
pub fn get_position(params: GetPositionParams) -> Option<Position> {
    POSITIONS.with(|positions| {
        positions
            .borrow()
            .get(&(params.user, params.series_id))
            .cloned()
    })
}

#[query]
pub fn get_positions(user: Principal) -> Vec<(SeriesId, Position)> {
    let user: User = user.into();

    POSITIONS.with(|positions| {
        positions
            .borrow()
            .iter()
            .filter(|((u, _), _)| *u == user)
            .map(|((_, series_id), position)| (series_id.clone(), position.clone()))
            .collect()
    })
}
