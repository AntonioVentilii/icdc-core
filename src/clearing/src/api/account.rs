use std::collections::BTreeMap;

use ic_cdk_macros::{query, update};
use shared::types::{Asset, SeriesId};

use crate::{
    guards::caller_is_not_anonymous,
    memory::{MARGIN_ACCOUNTS, POSITIONS},
    traits::ClearingAccountExt,
    types::{
        errors::{LedgerError, MarginAccountError},
        margin::{MarginAccount, Position},
        params::{GetMarginAccountParams, GetPositionParams},
        results::GetMarginAccountResult,
        user::User,
    },
};

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

#[update(guard = "caller_is_not_anonymous")]
pub async fn get_margin_account(params: GetMarginAccountParams) -> GetMarginAccountResult {
    let result: Result<MarginAccount, MarginAccountError> = (async {
        let user: User = ic_cdk::caller().into();

        let GetMarginAccountParams { refresh } = params;

        let refresh = refresh.unwrap_or(false);

        // Always read required_margin from internal state (risk state)
        let required_margin_u128 = MARGIN_ACCOUNTS.with(|accounts| {
            accounts
                .borrow()
                .get(&user)
                .map(|m| m.required_margin)
                .unwrap_or(0)
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
                        MarginAccountError::Ledger(LedgerError::FetchingBalanceFailed(format!(
                            "icrc1_balance_of {:?}: {}",
                            code, msg
                        )))
                    })?;

            let bal_u128: u128 = ledger_balance
                .0
                .try_into()
                .map_err(|_| MarginAccountError::BalanceMathOverflow)?;

            balances.insert(asset, bal_u128);
        }

        // Persist refreshed balances, but do NOT overwrite required_margin
        MARGIN_ACCOUNTS.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            let acct = accounts.entry(user).or_insert(MarginAccount {
                user,
                balances: BTreeMap::new(),
                required_margin: 0,
            });

            acct.balances = balances.clone();
            acct.required_margin = required_margin_u128;
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

#[query(guard = "caller_is_not_anonymous")]
pub fn get_position(params: GetPositionParams) -> Option<Position> {
    let caller: User = ic_cdk::caller().into();

    POSITIONS.with(|positions| positions.borrow().get(&(caller, params.series_id)).cloned())
}

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
