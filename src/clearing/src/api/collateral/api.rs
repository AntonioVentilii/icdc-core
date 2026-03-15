use candid::Nat;
use ic_cdk::caller;
use ic_cdk_macros::{query, update};
use shared::{
    constants::{BPS_BASE, USD_DECIMALS, VUSD_ASSET_ID},
    types::{asset::errors::AssetError, BalanceDomain, CollateralAssetInfo},
};

use super::{
    errors::{DepositCollateralError, WithdrawCollateralError},
    params::{DepositCollateralParams, WithdrawCollateralParams},
    results::{DepositCollateralResult, WithdrawCollateralResult},
};
use crate::{
    assets::{
        asset::{
            handler::get_handler,
            params::{AssetTransferFromParams, AssetTransferParams},
        },
        types::AssetAmount,
    },
    guards::caller_is_not_anonymous,
    memory::{ACCOUNT_STATES, ASSET_METRICS, COLLATERAL_ASSETS, DEPOSIT_PLANS, WITHDRAWAL_PLANS},
    types::{
        account::AssetAccount,
        margin::AccountState,
        plans::{DepositPlan, DepositPlanParams, PlanStatus, WithdrawalPlan, WithdrawalPlanParams},
        user::User,
    },
};

/// Deposits collateral into the user's account state.
///
/// This is a multi-phase operation:
/// 1. Building a [`DepositPlan`] for idempotency.
/// 2. Executing the asynchronous ledger transfer (`transfer_from`).
/// 3. Finalising the internal collateral balances.
#[update(guard = "caller_is_not_anonymous")]
pub async fn deposit_collateral(params: DepositCollateralParams) -> DepositCollateralResult {
    let result: Result<(), DepositCollateralError> = (async {
        let user: User = caller().into();

        let DepositCollateralParams {
            amount,
            asset_id,
            deposit_id,
            domain,
        } = params;

        // Verify the asset is supported and enabled
        let config = COLLATERAL_ASSETS.with(|assets| {
            assets
                .borrow()
                .get(&asset_id)
                .cloned()
                .ok_or(DepositCollateralError::Asset(AssetError::UnsupportedAsset))
        })?;

        if !config.is_enabled {
            return Err(DepositCollateralError::Asset(AssetError::UnsupportedAsset));
        }

        // ---------- Phase A: Build plan (no awaits) ----------
        let mut plan = DepositPlan::get_or_create(DepositPlanParams {
            deposit_id: deposit_id.clone(),
            user,
            asset_id: asset_id.clone(),
            amount: amount.clone(),
        });

        if plan.status == PlanStatus::Finalised {
            return Ok(());
        }

        let key = (user, deposit_id.clone());

        // ---------- Phase B: Execute transfer (async, resumable) ----------
        if plan.receipt.is_none() {
            plan.status = PlanStatus::Executing;

            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));

            let handler = get_handler(&config.asset).map_err(DepositCollateralError::Asset)?;

            let amount_u128: u128 = amount
                .0
                .clone()
                .try_into()
                .map_err(|_| DepositCollateralError::MathOverflow)?;

            let res = handler
                .transfer_from(AssetTransferFromParams {
                    asset: &config.asset,
                    spender: AssetAccount::CanisterMain,
                    from: AssetAccount::external_principal(user.principal()),
                    to: AssetAccount::UserClearing(user),
                    amount: AssetAmount::Fixed(amount_u128),
                    created_at_time_ns: plan.idempotency_ns.to_created_at_time_ns(),
                })
                .await;

            match res {
                Ok(block_index) => {
                    plan.receipt = Some(Nat::from(block_index).into());
                }
                Err(e) => {
                    DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));

                    return Err(DepositCollateralError::Asset(e));
                }
            }

            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));
        }

        // ---------- Phase C: Finalise (no awaits, idempotent) ----------
        if plan.receipt.is_some() && plan.status != PlanStatus::Finalised {
            let amount_u128: u128 = amount
                .0
                .clone()
                .try_into()
                .map_err(|_| DepositCollateralError::MathOverflow)?;

            ACCOUNT_STATES.with(|accounts| {
                let mut accounts = accounts.borrow_mut();
                let state = accounts
                    .entry(user)
                    .or_insert_with(|| AccountState::new(user));

                let domain = domain.unwrap_or(BalanceDomain::Settlement);

                if asset_id == VUSD_ASSET_ID {
                    let amount_usd = if config.decimals > USD_DECIMALS {
                        (amount_u128 / 10_u128.pow(u32::from(config.decimals - USD_DECIMALS)))
                            .cast_signed()
                    } else {
                        (amount_u128 * 10_u128.pow(u32::from(USD_DECIMALS - config.decimals)))
                            .cast_signed()
                    };
                    let current_cash = state.get_cash_balance_usd(domain);
                    state.set_cash_balance_usd(domain, current_cash + amount_usd);
                } else {
                    let current = state.get_balance(domain, &asset_id);
                    state.set_balance(domain, asset_id.clone(), current + amount_u128);
                }
            });

            plan.status = PlanStatus::Finalised;
            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan));
        }

        Ok(())
    })
    .await;

    result.into()
}

/// Withdraws collateral from the user's account state to an external address.
///
/// This implements the "Deterministic Withdrawal Policy":
/// 1. Calculate current account equity in USD.
/// 2. Verify equity >= `reserved_margin_usd` (risk check).
/// 3. If ok, proceed with asynchronous ledger transfer.
#[update(guard = "caller_is_not_anonymous")]
pub async fn withdraw_collateral(params: WithdrawCollateralParams) -> WithdrawCollateralResult {
    let result: Result<(), WithdrawCollateralError> = (async {
        let user: User = caller().into();

        let WithdrawCollateralParams {
            amount,
            asset_id,
            withdrawal_id,
            domain,
        } = params;

        let config = COLLATERAL_ASSETS.with(|assets| {
            assets
                .borrow()
                .get(&asset_id)
                .cloned()
                .ok_or(WithdrawCollateralError::Asset(AssetError::UnsupportedAsset))
        })?;

        let metric = ASSET_METRICS.with(|metrics| {
            metrics
                .borrow()
                .get(&asset_id)
                .cloned()
                .ok_or(WithdrawCollateralError::Asset(AssetError::UnsupportedAsset))
        })?;

        // ---------- Phase A: Build plan (durable, no awaits) ----------
        let mut plan = WithdrawalPlan::get_or_create(WithdrawalPlanParams {
            withdrawal_id: withdrawal_id.clone(),
            user,
            asset_id: asset_id.clone(),
            amount: amount.clone(),
            to_account: (user.principal(), None),
        });

        if plan.status == PlanStatus::Finalised {
            return Ok(());
        }

        let key = (user, withdrawal_id.clone());

        if plan.reserved_amount.is_none() {
            let amount_u128: u128 = amount
                .0
                .clone()
                .try_into()
                .map_err(|_| WithdrawCollateralError::MathOverflow)?;

            let price_value = metric.price_usd.value;
            let price_decimals = u32::from(metric.price_usd.decimals);
            let asset_decimals = u32::from(config.decimals);
            let target_decimals = u32::from(USD_DECIMALS);

            let withdrawal_value_usd_nat = {
                let haircut_multiplier =
                    u128::from(BPS_BASE).saturating_sub(u128::from(metric.haircut_bps));

                let numerator =
                    Nat::from(amount_u128) * Nat::from(price_value) * Nat::from(haircut_multiplier);

                let total_source_decimals = asset_decimals + price_decimals;

                if total_source_decimals >= target_decimals {
                    let divisor = Nat::from(BPS_BASE)
                        * Nat::from(10_u128.pow(total_source_decimals - target_decimals));
                    numerator / divisor
                } else {
                    let multiplier =
                        Nat::from(10_u128.pow(target_decimals - total_source_decimals));
                    (numerator * multiplier) / Nat::from(BPS_BASE)
                }
            };

            let withdrawal_value_usd: u128 =
                withdrawal_value_usd_nat.0.try_into().unwrap_or(u128::MAX);

            let (post_equity_usd, reserved_margin_usd, pre_equity_usd) =
                ACCOUNT_STATES.with(|accounts| {
                    let accounts = accounts.borrow();
                    let state = accounts.get(&user).ok_or(
                        WithdrawCollateralError::InsufficientExcessMargin {
                            available: 0,
                            requested: withdrawal_value_usd,
                        },
                    )?;

                    let configs = COLLATERAL_ASSETS.with(|c| c.borrow().clone());
                    let metrics = ASSET_METRICS.with(|m| m.borrow().clone());

                    let domain = domain.unwrap_or(BalanceDomain::Settlement);

                    let pre_equity = state.calculate_equity_usd(domain, &configs, &metrics);

                    let mut temp_state = state.clone();
                    if asset_id == VUSD_ASSET_ID {
                        let amount_usd = if config.decimals > USD_DECIMALS {
                            (amount_u128 / 10_u128.pow(u32::from(config.decimals - USD_DECIMALS)))
                                .cast_signed()
                        } else {
                            (amount_u128 * 10_u128.pow(u32::from(USD_DECIMALS - config.decimals)))
                                .cast_signed()
                        };
                        let current_cash = temp_state.get_cash_balance_usd(domain);
                        temp_state.set_cash_balance_usd(domain, current_cash - amount_usd);
                    } else {
                        let current = temp_state.get_balance(domain, &asset_id);
                        temp_state.set_balance(
                            domain,
                            asset_id.clone(),
                            current.saturating_sub(amount_u128),
                        );
                    }

                    let post_equity = temp_state.calculate_equity_usd(domain, &configs, &metrics);
                    Ok::<(u128, u128, u128), WithdrawCollateralError>((
                        post_equity,
                        temp_state.get_reserved_margin_usd(domain),
                        pre_equity,
                    ))
                })?;

            if post_equity_usd < reserved_margin_usd {
                return Err(WithdrawCollateralError::InsufficientExcessMargin {
                    available: pre_equity_usd.saturating_sub(reserved_margin_usd),
                    requested: withdrawal_value_usd,
                });
            }

            let mut reserved_cash_usd: Option<i128> = None;

            // Debit internal balance
            ACCOUNT_STATES.with(|accounts| {
                let mut accounts = accounts.borrow_mut();
                if let Some(state) = accounts.get_mut(&user) {
                    let domain = domain.unwrap_or(BalanceDomain::Settlement);
                    if asset_id == VUSD_ASSET_ID {
                        let amount_usd = if config.decimals > USD_DECIMALS {
                            (amount_u128 / 10_u128.pow(u32::from(config.decimals - USD_DECIMALS)))
                                .cast_signed()
                        } else {
                            (amount_u128 * 10_u128.pow(u32::from(USD_DECIMALS - config.decimals)))
                                .cast_signed()
                        };
                        let current_cash = state.get_cash_balance_usd(domain);
                        state.set_cash_balance_usd(domain, current_cash - amount_usd);
                        reserved_cash_usd = Some(amount_usd);
                    } else {
                        let current = state.get_balance(domain, &asset_id);
                        state.set_balance(
                            domain,
                            asset_id.clone(),
                            current.saturating_sub(amount_u128),
                        );
                    }
                }
            });

            plan.reserved_amount = Some(amount_u128);
            plan.reserved_cash_usd = reserved_cash_usd;
            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));
        }

        // ---------- Phase C: Execute ledger transfer ----------
        if plan.receipt.is_none() {
            plan.status = PlanStatus::Executing;
            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));

            let handler = get_handler(&config.asset).map_err(WithdrawCollateralError::Asset)?;

            let amount_u128: u128 = plan.reserved_amount.unwrap();

            let res = handler
                .transfer(AssetTransferParams {
                    asset: &config.asset,
                    from: AssetAccount::UserClearing(user),
                    to: AssetAccount::external_icrc(plan.to_account.0, plan.to_account.1),
                    amount: AssetAmount::Fixed(amount_u128),
                    created_at_time_ns: plan.idempotency_ns.to_created_at_time_ns(),
                })
                .await;

            match res {
                Ok(block_index) => {
                    plan.receipt = Some(Nat::from(block_index).into());
                }
                Err(e) => {
                    // Compensation: refund internal balance on failure
                    let reserved_tokens = plan.reserved_amount.take();
                    let reserved_cash = plan.reserved_cash_usd.take();

                    if reserved_tokens.is_some() || reserved_cash.is_some() {
                        ACCOUNT_STATES.with(|accounts| {
                            let mut accounts = accounts.borrow_mut();
                            if let Some(state) = accounts.get_mut(&user) {
                                let domain = domain.unwrap_or(BalanceDomain::Settlement);
                                match (reserved_cash, reserved_tokens) {
                                    (Some(usd), None) => {
                                        let current = state.get_cash_balance_usd(domain);
                                        state.set_cash_balance_usd(domain, current + usd);
                                    }

                                    (None, Some(tokens)) => {
                                        let current = state.get_balance(domain, &asset_id);
                                        state.set_balance(
                                            domain,
                                            asset_id.clone(),
                                            current + tokens,
                                        );
                                    }

                                    (Some(usd), Some(tokens)) => {
                                        let current_cash = state.get_cash_balance_usd(domain);
                                        state.set_cash_balance_usd(domain, current_cash + usd);
                                        let current_tokens = state.get_balance(domain, &asset_id);
                                        state.set_balance(
                                            domain,
                                            asset_id.clone(),
                                            current_tokens + tokens,
                                        );
                                    }

                                    (None, None) => {}
                                }
                            }
                        });
                    }
                    WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));
                    return Err(WithdrawCollateralError::Asset(e));
                }
            }
            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan.clone()));
        }

        // ---------- Phase D: Finalise ----------
        if plan.receipt.is_some() && plan.status != PlanStatus::Finalised {
            plan.status = PlanStatus::Finalised;
            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(key.clone(), plan));
        }

        Ok(())
    })
    .await;

    result.into()
}

/// Returns a list of all supported collateral assets with their metrics.
#[query(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn get_collateral_assets() -> Vec<CollateralAssetInfo> {
    let configs = COLLATERAL_ASSETS.with(|c| c.borrow().clone());
    let metrics = ASSET_METRICS.with(|m| m.borrow().clone());

    configs
        .into_iter()
        .map(|(id, config)| CollateralAssetInfo {
            config,
            metrics: metrics.get(&id).cloned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use candid::{Nat, Principal};
    use shared::{constants::BPS_BASE, types::BalanceDomain};

    use crate::types::{margin::AccountState, user::User};

    #[test]
    fn withdrawal_value_calculation_with_haircut() {
        let amount_u128 = 100_000_000_u128; // 1 ICP (8 decimals)
        let price_value = 10_000_000_u128; // $10 (6 decimals)
        let price_decimals = 6_u32;
        let asset_decimals = 8_u32;
        let target_decimals = 6_u32;
        let haircut_bps = 1000_u16; // 10% haircut

        // Manual calculation:
        // Market value = (1e8 * 10e6) / 10^(8+6-6) = 1e14 / 1e8 = 1e6 ($1 USD, wait)
        // No, 1 ICP * $10 = $10 USD.
        // numerator = 1e8 * 10e6 = 1e15.
        // divisor = 10^(8+6-6) = 10^8.
        // 1e15 / 1e8 = 1e7 = 10 USD (with 6 decimals). Correct.

        // With 10% haircut:
        // multiplier = 10000 - 1000 = 9000
        // numerator = 1e8 * 10e6 * 9000 = 9e18
        // divisor = 10000 * 10^(8+6-6) = 1e4 * 1e8 = 1e12
        // value = 9e18 / 1e12 = 9e6 = 9 USD. Correct.

        let haircut_multiplier = u128::from(BPS_BASE).saturating_sub(u128::from(haircut_bps));

        let numerator =
            Nat::from(amount_u128) * Nat::from(price_value) * Nat::from(haircut_multiplier);

        let total_source_decimals = asset_decimals + price_decimals;

        let value_usd_nat = if total_source_decimals >= target_decimals {
            let divisor = Nat::from(BPS_BASE)
                * Nat::from(10_u128.pow(total_source_decimals - target_decimals));
            numerator / divisor
        } else {
            let multiplier = Nat::from(10_u128.pow(target_decimals - total_source_decimals));
            (numerator * multiplier) / Nat::from(BPS_BASE)
        };

        let value_usd: u128 = value_usd_nat.0.try_into().unwrap();
        assert_eq!(value_usd, 9_000_000); // $9 USD
    }

    #[test]
    fn domain_isolation() {
        let user_p = Principal::from_slice(&[42]);
        let user = User(user_p);
        let asset_id = "ICP".to_owned();

        let mut state = AccountState::new(user);

        // Deposit to Settlement
        state.set_balance(BalanceDomain::Settlement, asset_id.clone(), 1000);
        assert_eq!(
            state.get_balance(BalanceDomain::Settlement, &asset_id),
            1000
        );
        assert_eq!(state.get_balance(BalanceDomain::Playground, &asset_id), 0);

        // Deposit to Playground
        state.set_balance(BalanceDomain::Playground, asset_id.clone(), 500);
        assert_eq!(
            state.get_balance(BalanceDomain::Settlement, &asset_id),
            1000
        );
        assert_eq!(state.get_balance(BalanceDomain::Playground, &asset_id), 500);

        // Withdraw from Settlement
        let current = state.get_balance(BalanceDomain::Settlement, &asset_id);
        state.set_balance(BalanceDomain::Settlement, asset_id.clone(), current - 200);
        assert_eq!(state.get_balance(BalanceDomain::Settlement, &asset_id), 800);
        assert_eq!(state.get_balance(BalanceDomain::Playground, &asset_id), 500);
    }
}
