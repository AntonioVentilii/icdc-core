use std::collections::BTreeMap;

use candid::Nat;
use ic_cdk_macros::{query, update};
use shared::{
    constants::{BPS_BASE, USD_DECIMALS},
    types::{AssetId, SeriesId},
};

use super::{
    errors::AccountStateError,
    params::{GetAccountStateParams, GetPositionParams},
};
use crate::{
    api::account::results::{AccountStateResponse, AssetWorth},
    assets::asset::{handler::get_handler, params::AssetBalanceOfParams},
    guards::caller_is_not_anonymous,
    memory::{ACCOUNT_STATES, ASSET_METRICS, COLLATERAL_ASSETS, POSITIONS},
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
    let user: User = ic_cdk::caller().into();

    let state_res = ACCOUNT_STATES.with(|accounts| {
        accounts
            .borrow()
            .get(&user)
            .cloned()
            .ok_or(AccountStateError::NoAccountStateFound)
    });

    match state_res {
        Ok(state) => Ok(build_account_state_response(state)).into(),
        Err(e) => Err(e).into(),
    }
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

    match result {
        Ok(state) => Ok(build_account_state_response(state)).into(),
        Err(e) => Err(e).into(),
    }
}

fn build_account_state_response(state: AccountState) -> AccountStateResponse {
    let configs = COLLATERAL_ASSETS.with(|c| c.borrow().clone());
    let metrics = ASSET_METRICS.with(|m| m.borrow().clone());

    let mut asset_worths = Vec::new();
    let target_decimals = USD_DECIMALS as u32;

    for (asset_id, balance) in &state.collateral_balances {
        let mut value_usd = 0u128;
        let mut pre_haircut_value_usd = 0u128;
        let mut haircut_bps = 0u16;

        if let (Some(config), Some(metric)) = (configs.get(asset_id), metrics.get(asset_id)) {
            if config.is_enabled {
                let price_value = metric.price_usd.value;
                let price_decimals = metric.price_usd.decimals as u32;
                let asset_decimals = config.decimals as u32;
                haircut_bps = metric.haircut_bps;

                let haircut_multiplier =
                    (BPS_BASE as u128).saturating_sub(metric.haircut_bps as u128);

                let numerator_pre = Nat::from(*balance) * Nat::from(price_value);
                let numerator_post = numerator_pre.clone() * Nat::from(haircut_multiplier);

                let total_source_decimals = asset_decimals + price_decimals;

                let (v_post_nat, v_pre_nat) = if total_source_decimals >= target_decimals {
                    let diff = total_source_decimals - target_decimals;
                    let divisor_raw = Nat::from(10u128.pow(diff));
                    let divisor_post = Nat::from(BPS_BASE) * divisor_raw.clone();

                    (numerator_post / divisor_post, numerator_pre / divisor_raw)
                } else {
                    let diff = target_decimals - total_source_decimals;
                    let multiplier_raw = Nat::from(10u128.pow(diff));
                    (
                        (numerator_post * multiplier_raw.clone()) / Nat::from(BPS_BASE),
                        numerator_pre * multiplier_raw,
                    )
                };

                value_usd = v_post_nat.0.try_into().unwrap_or(u128::MAX);
                pre_haircut_value_usd = v_pre_nat.0.try_into().unwrap_or(u128::MAX);
            }
        }

        asset_worths.push(AssetWorth {
            asset_id: asset_id.clone(),
            balance: *balance,
            value_usd,
            pre_haircut_value_usd,
            haircut_bps,
        });
    }

    let total_equity_usd = state.calculate_equity_usd(&configs, &metrics);
    let available_equity_usd = state.get_available_equity_usd(&configs, &metrics);

    AccountStateResponse {
        state,
        assets: asset_worths,
        total_equity_usd,
        available_equity_usd,
    }
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
