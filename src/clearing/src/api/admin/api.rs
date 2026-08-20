use std::collections::BTreeMap;

use candid::{Nat, Principal};
use ic_cdk::api::{is_controller, msg_caller, trap};
use ic_cdk_macros::{query, update};
use shared::types::{
    AllowedBalanceDomains, Asset, AssetId, AssetMetrics, BalanceDomain, CollateralAssetConfig,
    DomainPolicy,
};

use super::{
    errors::{
        CancelFundWithdrawalError, ReassignAccountError, RefreshIcrcAssetMetadataError,
        RegisterIcrcAssetError, UpdateAssetPriceError, UpdateCollateralAllowedDomainsError,
        WithdrawFundError,
    },
    params::{
        CancelFundWithdrawalParams, FundType, RefreshIcrcAssetMetadataParams,
        RegisterIcrcAssetParams, UpdateAssetMetricsParams, UpdateAssetPriceParams,
        UpdateCollateralAllowedDomainsParams, UpdateCollateralAssetParams,
        UpdateDomainPolicyParams, WithdrawFundParams,
    },
    results::{
        CancelFundWithdrawalResult, GetFundsResult, ReassignAccountResult,
        RefreshIcrcAssetMetadataResult, RegisterIcrcAssetResult, UpdateAssetPriceResult,
        UpdateCollateralAllowedDomainsResult, WithdrawFundResult,
    },
};
use crate::{
    assets::{
        asset::{
            handler::{get_handler, AssetHandler},
            params::AssetTransferParams,
        },
        types::AssetAmount,
    },
    guards::{caller_is_controller, caller_is_not_anonymous},
    memory::{
        ACCOUNT_STATES, ASSET_METRICS, COLLATERAL_ASSETS, CONFIG, DEPOSIT_PLANS, DOMAIN_POLICIES,
        FROZEN_TRANSFERS, FUND_WITHDRAWAL_PLANS, INSURANCE_FUND, LIMIT_ORDERS, MIGRATION_PLANS,
        POSITIONS, REGISTRY_CANISTER, SETTLEMENT_PLANS, TREASURY, WITHDRAWAL_PLANS,
    },
    types::{
        account::AssetAccount,
        errors::CommonError,
        payment::PaymentReceipt,
        plans::{FundWithdrawalPlan, FundWithdrawalPlanParams, PlanStatus},
        state::Config,
        user::User,
    },
    utils::{
        registry,
        system::now_ns,
        vusd::{is_internal_asset, is_internal_ledger},
    },
};

/// Returns the principal of the Series Registry canister.
#[query(guard = "caller_is_controller")]
#[must_use]
pub fn get_registry_canister() -> Principal {
    REGISTRY_CANISTER.with(|r| *r.borrow())
}

/// Sets the principal of the Series Registry canister.
///
/// This principal is used to discover and validate derivative series.
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn set_registry_canister(registry: Principal) {
    REGISTRY_CANISTER.with(|r| {
        *r.borrow_mut() = registry;
    });
}

/// Returns the current global configuration of the Clearing canister.
#[query(guard = "caller_is_controller")]
#[must_use]
pub fn config() -> Config {
    CONFIG.with(|c| c.borrow().clone())
}

/// Updates the global configuration for the Clearing canister.
///
/// Immutable properties like `internal_ledger_id` and `version` are preserved from the existing
/// state. This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn update_config(config: Config) {
    CONFIG.with(|c| {
        let mut new_config = config;
        let current = c.borrow();
        new_config.internal_ledger = current.internal_ledger.clone();
        new_config.version = current.version;
        drop(current);
        *c.borrow_mut() = new_config;
    });
}

/// Returns the current balances of the Insurance Fund and Treasury.
///
/// This method is gated to canister controllers.
#[query(guard = "caller_is_controller")]
#[must_use]
pub fn get_funds() -> GetFundsResult {
    let insurance_fund = INSURANCE_FUND.with(|f| f.borrow().clone());
    let treasury = TREASURY.with(|f| f.borrow().clone());

    GetFundsResult {
        insurance_fund,
        treasury,
    }
}

/// Withdraws assets from the Insurance Fund or Treasury to an external wallet.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub async fn withdraw_fund(params: WithdrawFundParams) -> WithdrawFundResult {
    let res: Result<Nat, WithdrawFundError> =
        (async {
            let WithdrawFundParams {
                request_id,
                fund_type,
                asset_id,
                amount,
                to,
            } = params;

            let config =
                COLLATERAL_ASSETS.with(|assets| {
                    assets.borrow().get(&asset_id).cloned().ok_or(
                        WithdrawFundError::TransferFailed("Unsupported asset".to_owned()),
                    )
                })?;

            let asset = config.asset;

            // ---------- Phase A: Build or resume plan ----------
            let mut plan = FundWithdrawalPlan::get_or_create(FundWithdrawalPlanParams {
                request_id: request_id.clone(),
                fund_type,
                asset_id: asset_id.clone(),
                amount,
                to,
            });

            if plan.status == PlanStatus::Finalised {
                return plan.receipt.map(|r| r.block_index()).ok_or(
                    WithdrawFundError::TransferFailed("No receipt found".to_owned()),
                );
            }

            // ---------- Phase B: Deduct fund balance (internal) ----------
            if plan.status == PlanStatus::Planned {
                deduct_fund_balance_impl(&asset_id, amount, fund_type)?;

                plan.status = PlanStatus::Executing;
                FUND_WITHDRAWAL_PLANS
                    .with(|m| m.borrow_mut().insert(request_id.clone(), plan.clone()));
            }

            // ---------- Phase C: Execute ledger transfer ----------
            if plan.receipt.is_none() {
                let handler = get_handler(&asset)
                    .map_err(|e| WithdrawFundError::TransferFailed(format!("{e:?}")))?;

                let transfer_res = handler
                    .transfer(AssetTransferParams {
                        asset: &asset,
                        asset_id: &asset_id,
                        from: AssetAccount::CanisterMain,
                        to: AssetAccount::external_principal(to),
                        amount: AssetAmount::Fixed(amount),
                        created_at_time_ns: plan.idempotency_ns.to_created_at_time_ns(),
                    })
                    .await;

                match transfer_res {
                    Ok(block) => {
                        plan.receipt = Some(PaymentReceipt::IcrcBlockIndex(Nat::from(block)));
                        plan.status = PlanStatus::Finalised;
                    }
                    Err(e) => {
                        // For fund withdrawals, keep state as Executing since deducting from the
                        // internal fund (Phase B) already succeeded. This prevents a retry from
                        // duplicating Phase B. Because the exact identical `idempotency_ns` will be
                        // provided on retry, the ICRC ledger handler duplicate check makes retrying
                        // entirely safe.
                        return Err(WithdrawFundError::TransferFailed(format!("{e:?}")));
                    }
                }

                FUND_WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(request_id, plan.clone()));
            }

            Ok(plan.receipt.unwrap().block_index())
        })
        .await;

    res.into()
}

/// Cancels a stuck fund withdrawal and refunds the internal ledger balance.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
#[must_use]
pub fn cancel_fund_withdrawal(params: CancelFundWithdrawalParams) -> CancelFundWithdrawalResult {
    let old_plan = FUND_WITHDRAWAL_PLANS.with(|m| m.borrow_mut().remove(&params.request_id));
    let Some(plan) = old_plan else {
        return Err(CancelFundWithdrawalError::PlanNotFound).into();
    };

    if plan.status != PlanStatus::Executing || plan.receipt.is_some() {
        // Put it back
        FUND_WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(params.request_id, plan));
        return Err(CancelFundWithdrawalError::InvalidPlanStatus).into();
    }

    // Refund internally
    let store = match plan.fund_type {
        FundType::Insurance => &INSURANCE_FUND,
        FundType::Treasury => &TREASURY,
    };

    store.with(|f| {
        let mut f = f.borrow_mut();
        let current = f.get(&plan.asset_id).copied().unwrap_or(0);
        f.insert(plan.asset_id.clone(), current + plan.amount);
    });

    Ok(()).into()
}

/// Adds or updates a collateral asset configuration.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn update_collateral_asset(params: UpdateCollateralAssetParams) {
    let config = params.config;

    // Enforce ICRC registration policy: ICRC assets MUST be registered via register_icrc_asset
    // to ensure metadata (symbol, decimals, fee) is fetched directly from the ledger.
    if matches!(config.asset, Asset::Icrc(_)) {
        trap(
            "ICRC assets must be registered via 'register_icrc_asset' to ensure metadata integrity",
        );
    }

    insert_collateral_asset_impl(config);
}

/// Adds or updates dynamic metrics for a collateral asset.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn update_asset_metrics(params: UpdateAssetMetricsParams) {
    insert_asset_metrics_impl(params.asset_id, params.metrics);
}

/// Automatically registers an ICRC asset by fetching its metadata from the ledger.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub async fn register_icrc_asset(params: RegisterIcrcAssetParams) -> RegisterIcrcAssetResult {
    let res: Result<(), RegisterIcrcAssetError> = (async {
        let RegisterIcrcAssetParams {
            asset_id,
            ledger_id,
            haircut_bps,
            oracle_id,
            is_enabled,
            allowed_balance_domains,
        } = params;

        let allowed_balance_domains = Vec::from(
            AllowedBalanceDomains::try_from(allowed_balance_domains)
                .map_err(|_| RegisterIcrcAssetError::InvalidAllowedBalanceDomains)?,
        );

        let asset = Asset::Icrc(ledger_id);

        if is_internal_ledger(&ledger_id) {
            return Err(RegisterIcrcAssetError::VusdCannotBeCollateral);
        }

        let exists = COLLATERAL_ASSETS.with(|assets| assets.borrow().contains_key(&asset_id))
            || is_internal_asset(&asset_id);

        if exists {
            return Err(RegisterIcrcAssetError::AssetAlreadyExists);
        }

        let handler = get_handler(&asset)
            .map_err(|e| RegisterIcrcAssetError::Common(CommonError::Internal(format!("{e:?}"))))?;

        let AssetHandler::Icrc(icrc_handler) = &handler else {
            return Err(RegisterIcrcAssetError::Common(CommonError::Internal(
                "Expected ICRC handler".to_owned(),
            )));
        };

        let metadata = icrc_handler
            .get_metadata(&asset)
            .await
            .map_err(|e| RegisterIcrcAssetError::Common(CommonError::Internal(format!("{e:?}"))))?;

        let config = CollateralAssetConfig {
            asset_id: asset_id.clone(),
            asset,
            symbol: metadata.symbol,
            decimals: metadata.decimals,
            is_enabled,
            oracle_id,
            allowed_balance_domains,
        };

        // If it exists, preserve the current price; otherwise start at 0
        let current_price = ASSET_METRICS.with(|m| {
            m.borrow()
                .get(&asset_id)
                .map(|metrics| metrics.price_usd.clone())
                .unwrap_or_default()
        });

        let metrics = AssetMetrics {
            price_usd: current_price,
            latest_transfer_fee: Some(metadata.fee),
            haircut_bps,
            insurance_fee_ratio: None,
            protocol_fee_ratio: None,
            last_updated_ns: Some(now_ns()),
        };

        upsert_asset_state_impl(config, metrics);

        Ok(())
    })
    .await;

    res.into()
}

fn upsert_asset_state_impl(config: CollateralAssetConfig, metrics: AssetMetrics) {
    let asset_id = config.asset_id.clone();

    insert_collateral_asset_impl(config);

    insert_asset_metrics_impl(asset_id, metrics);
}

/// Refreshes the metadata (decimals, fee, symbol) of an already registered ICRC asset.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub async fn refresh_icrc_asset_metadata(
    params: RefreshIcrcAssetMetadataParams,
) -> RefreshIcrcAssetMetadataResult {
    let res: Result<(), RefreshIcrcAssetMetadataError> = (async {
        let RefreshIcrcAssetMetadataParams { asset_id } = params;

        let mut config = COLLATERAL_ASSETS
            .with(|assets| assets.borrow().get(&asset_id).cloned())
            .ok_or(RefreshIcrcAssetMetadataError::AssetNotFound)?;

        if !matches!(config.asset, Asset::Icrc(_)) {
            return Err(RefreshIcrcAssetMetadataError::NotAnIcrcAsset);
        }

        let handler = get_handler(&config.asset).map_err(|e| {
            RefreshIcrcAssetMetadataError::Common(CommonError::Internal(format!("{e:?}")))
        })?;

        let AssetHandler::Icrc(icrc_handler) = &handler else {
            return Err(RefreshIcrcAssetMetadataError::Common(
                CommonError::Internal("Expected ICRC handler".to_owned()),
            ));
        };

        let metadata = icrc_handler
            .get_metadata(&config.asset)
            .await
            .map_err(|e| {
                RefreshIcrcAssetMetadataError::Common(CommonError::Internal(format!("{e:?}")))
            })?;

        config.symbol = metadata.symbol;
        config.decimals = metadata.decimals;

        insert_collateral_asset_impl(config);

        ASSET_METRICS.with(|m| {
            if let Some(metrics) = m.borrow_mut().get_mut(&asset_id) {
                metrics.latest_transfer_fee = Some(metadata.fee);
                metrics.last_updated_ns = Some(now_ns());
            }
        });

        Ok(())
    })
    .await;

    res.into()
}

fn insert_collateral_asset_impl(config: CollateralAssetConfig) {
    COLLATERAL_ASSETS.with(|assets| {
        assets.borrow_mut().insert(config.asset_id.clone(), config);
    });
}

fn insert_asset_metrics_impl(asset_id: AssetId, metrics: AssetMetrics) {
    ASSET_METRICS.with(|assets| {
        assets.borrow_mut().insert(asset_id, metrics);
    });
}

/// Updates the price of an asset.
///
/// This method can be called by canister controllers or authorized oracles.
#[update(guard = "caller_is_not_anonymous")]
pub async fn update_asset_price(params: UpdateAssetPriceParams) -> UpdateAssetPriceResult {
    update_asset_price_impl(params).await.into()
}

async fn update_asset_price_impl(
    params: UpdateAssetPriceParams,
) -> Result<(), UpdateAssetPriceError> {
    let caller = msg_caller();

    let asset_config = COLLATERAL_ASSETS
        .with(|assets| assets.borrow().get(&params.asset_id).cloned())
        .ok_or(UpdateAssetPriceError::AssetNotFound)?;

    if !is_controller(&caller) {
        let oracle_id = asset_config
            .oracle_id
            .ok_or(UpdateAssetPriceError::OracleNotConfigured)?;
        let registry_canister = REGISTRY_CANISTER.with(|r| *r.borrow());
        if registry_canister == Principal::anonymous() {
            return Err(UpdateAssetPriceError::Common(CommonError::RegistryNotSet));
        }

        match registry::is_oracle_authorized(registry_canister, oracle_id, caller).await {
            Ok(true) => {}
            Ok(false) => {
                return Err(UpdateAssetPriceError::Common(CommonError::Unauthorized));
            }
            Err(e) => return Err(UpdateAssetPriceError::Common(e)),
        }
    }

    ASSET_METRICS.with(|m| {
        let mut m = m.borrow_mut();
        if let Some(metrics) = m.get_mut(&params.asset_id) {
            metrics.price_usd = params.price.decimal.clone();
            metrics.last_updated_ns = Some(now_ns());
            Ok(())
        } else {
            Err(UpdateAssetPriceError::AssetMetricsNotInitialized)
        }
    })
}

/// Returns the current domain policies for all configured domains.
///
/// This method is gated to canister controllers.
#[query(guard = "caller_is_controller")]
#[must_use]
pub fn get_domain_policies() -> BTreeMap<BalanceDomain, DomainPolicy> {
    DOMAIN_POLICIES.with(|p| p.borrow().clone())
}

/// Adds or updates the policy for a specific balance domain.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
pub fn update_domain_policy(params: UpdateDomainPolicyParams) {
    DOMAIN_POLICIES.with(|p| {
        p.borrow_mut().insert(params.domain, params.policy);
    });
}

/// Updates which balance domains may hold this collateral asset (deposits and withdrawals).
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
#[must_use]
pub fn update_collateral_allowed_domains(
    params: UpdateCollateralAllowedDomainsParams,
) -> UpdateCollateralAllowedDomainsResult {
    let res: Result<(), UpdateCollateralAllowedDomainsError> = (|| {
        let allowed_balance_domains = Vec::from(
            AllowedBalanceDomains::try_from(params.allowed_balance_domains)
                .map_err(|_| UpdateCollateralAllowedDomainsError::InvalidAllowedBalanceDomains)?,
        );

        COLLATERAL_ASSETS.with(|assets| {
            let mut assets = assets.borrow_mut();
            let config = assets
                .get_mut(&params.asset_id)
                .ok_or(UpdateCollateralAllowedDomainsError::AssetNotFound)?;
            config.allowed_balance_domains = allowed_balance_domains;
            Ok(())
        })
    })();

    res.into()
}

/// Atomically reassigns the entire clearing account of `old_owner` to `new_owner`.
///
/// Moves the full [`AccountState`](crate::types::margin::AccountState) (collateral
/// balances across all assets and balance domains, internal cash balances (USD), and
/// reserved margins per domain) plus every open position keyed by the old principal.
/// The generic use case is an account-ownership handover, e.g. a custodial key
/// rotation where an operator starts signing for the same logical account with a
/// newly derived principal.
///
/// Guard rails (the call rejects, mutating nothing, when any fails):
/// - `old_owner == new_owner`
/// - `old_owner` has no account
/// - `old_owner` has resting limit orders (cancel them first; the book's ownership is never
///   rewritten behind its back)
/// - `old_owner` has positions frozen for cross-canister transfer (their signed proofs are bound to
///   the old principal)
/// - `old_owner` or `new_owner` has non-finalised deposit / withdrawal / settlement /
///   domain-migration plans
/// - `new_owner` already holds any clearing state (this reassigns, it never merges)
///
/// The historical event log (and the leaderboard / accuracy projections derived
/// from it) is left untouched: it is an audit trail of what happened under the old
/// principal. Finalised plans likewise stay under their original keys as records.
///
/// This method is gated to canister controllers.
#[update(guard = "caller_is_controller")]
#[must_use]
pub fn admin_reassign_account(old_owner: Principal, new_owner: Principal) -> ReassignAccountResult {
    reassign_account_impl(old_owner.into(), new_owner.into()).into()
}

pub(crate) fn reassign_account_impl(
    old_owner: User,
    new_owner: User,
) -> Result<(), ReassignAccountError> {
    // ---------- Validation (no mutation before every check passes) ----------
    if old_owner == new_owner {
        return Err(ReassignAccountError::SameOwner);
    }

    if !ACCOUNT_STATES.with(|a| a.borrow().contains_key(&old_owner)) {
        return Err(ReassignAccountError::AccountNotFound);
    }

    let old_has_orders =
        LIMIT_ORDERS.with(|orders| orders.borrow().values().any(|o| o.creator == old_owner));
    if old_has_orders {
        return Err(ReassignAccountError::OpenOrdersExist);
    }

    let old_has_frozen =
        FROZEN_TRANSFERS.with(|transfers| transfers.borrow().values().any(|p| p.user == old_owner));
    if old_has_frozen {
        return Err(ReassignAccountError::PendingPositionTransfersExist);
    }

    check_no_inflight_plans_for_reassignment(old_owner)?;
    check_no_inflight_plans_for_reassignment(new_owner)?;

    if target_has_clearing_state(new_owner) {
        return Err(ReassignAccountError::TargetAccountNotEmpty);
    }

    // ---------- Atomic mutation (single synchronous message, no awaits) ----------
    ACCOUNT_STATES.with(|accounts| {
        let mut accounts = accounts.borrow_mut();
        if let Some(mut state) = accounts.remove(&old_owner) {
            state.user = new_owner;
            accounts.insert(new_owner, state);
        }
    });

    POSITIONS.with(|positions| {
        let mut positions = positions.borrow_mut();
        let old_keys: Vec<_> = positions
            .keys()
            .filter(|(user, _, _)| *user == old_owner)
            .cloned()
            .collect();
        for key in old_keys {
            if let Some(mut position) = positions.remove(&key) {
                position.user = new_owner;
                positions.insert((new_owner, key.1, key.2), position);
            }
        }
    });

    Ok(())
}

/// True when `owner` holds any clearing state that a reassignment would clobber:
/// a non-empty account state, open positions, resting orders, or positions frozen
/// for transfer. An empty [`AccountState`](crate::types::margin::AccountState)
/// shell does not count: it carries no economic state and is simply replaced.
fn target_has_clearing_state(owner: User) -> bool {
    let has_account_state = ACCOUNT_STATES.with(|accounts| {
        accounts.borrow().get(&owner).is_some_and(|state| {
            !state.balances.is_empty()
                || !state.cash_balances_usd.is_empty()
                || !state.reserved_margins_usd.is_empty()
        })
    });

    let has_positions =
        POSITIONS.with(|positions| positions.borrow().keys().any(|(user, _, _)| *user == owner));

    let has_orders =
        LIMIT_ORDERS.with(|orders| orders.borrow().values().any(|o| o.creator == owner));

    let has_frozen =
        FROZEN_TRANSFERS.with(|transfers| transfers.borrow().values().any(|p| p.user == owner));

    has_account_state || has_positions || has_orders || has_frozen
}

/// Rejects the reassignment while `user` has non-finalised deposit, withdrawal,
/// settlement, or domain-migration plans: those plans reference the principal and
/// would credit, refund, or settle against the wrong owner once the account moved.
fn check_no_inflight_plans_for_reassignment(user: User) -> Result<(), ReassignAccountError> {
    let has_deposit = DEPOSIT_PLANS.with(|plans| {
        plans
            .borrow()
            .iter()
            .any(|((u, _), p)| *u == user && p.status != PlanStatus::Finalised)
    });
    if has_deposit {
        return Err(ReassignAccountError::InFlightPlansExist);
    }

    let has_withdrawal = WITHDRAWAL_PLANS.with(|plans| {
        plans
            .borrow()
            .iter()
            .any(|((u, _), p)| *u == user && p.status != PlanStatus::Finalised)
    });
    if has_withdrawal {
        return Err(ReassignAccountError::InFlightPlansExist);
    }

    let has_migration = MIGRATION_PLANS.with(|plans| {
        plans
            .borrow()
            .iter()
            .any(|((u, _), p)| *u == user && p.status != PlanStatus::Finalised)
    });
    if has_migration {
        return Err(ReassignAccountError::InFlightPlansExist);
    }

    let has_settlement = SETTLEMENT_PLANS.with(|plans| {
        plans.borrow().values().any(|p| {
            p.status != PlanStatus::Finalised && p.positions.iter().any(|pos| pos.user == user)
        })
    });
    if has_settlement {
        return Err(ReassignAccountError::InFlightPlansExist);
    }

    Ok(())
}

pub(crate) fn deduct_fund_balance_impl(
    asset_id: &AssetId,
    amount: u128,
    fund_type: FundType,
) -> Result<(), WithdrawFundError> {
    let store = match fund_type {
        FundType::Insurance => &INSURANCE_FUND,
        FundType::Treasury => &TREASURY,
    };
    store.with(|f| {
        let mut f = f.borrow_mut();
        let current = f.get(asset_id).copied().unwrap_or(0);
        if current < amount {
            return Err(WithdrawFundError::InsufficientFunds);
        }
        f.insert(asset_id.clone(), current - amount);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{AssetId, BalanceDomain, Price, SeriesId};

    use crate::{
        api::admin::{
            api::{deduct_fund_balance_impl, reassign_account_impl},
            errors::{ReassignAccountError, WithdrawFundError},
            params::FundType,
        },
        memory::{ACCOUNT_STATES, INSURANCE_FUND, LIMIT_ORDERS, POSITIONS, TREASURY},
        types::{
            margin::{AccountState, Position},
            trade::{LimitOrder, OrderId, Side},
            user::User,
        },
    };

    #[test]
    fn withdraw_fund_resilience_on_transfer_failure() {
        let asset_id = AssetId::from("vUSD".to_owned());
        let amount = 10_000; // $1

        // Initialize insurance fund with $10
        INSURANCE_FUND.with(|f| {
            let mut f = f.borrow_mut();
            f.clear();
            f.insert(asset_id.clone(), 100_000);
        });

        // Step 1: Deduct
        let deduct_res = deduct_fund_balance_impl(&asset_id, amount, FundType::Insurance);
        assert!(deduct_res.is_ok());

        INSURANCE_FUND.with(|f| {
            assert_eq!(f.borrow().get(&asset_id).copied().unwrap(), 90_000);
        });

        // Step 2: In the new implementation we deliberately DO NOT rollback
        // upon transfer error. The internal fund deduction stands and Phase B
        // is not retried.
        INSURANCE_FUND.with(|f| {
            assert_eq!(f.borrow().get(&asset_id).copied().unwrap(), 90_000);
        });
    }

    #[test]
    fn withdraw_fund_insufficient_funds() {
        let asset_id = AssetId::from("vUSD".to_owned());
        let amount = 1_000_000; // $100

        // Initialize treasury with $10
        TREASURY.with(|f| {
            let mut f = f.borrow_mut();
            f.clear();
            f.insert(asset_id.clone(), 100_000);
        });

        // Try to deduct $100
        let deduct_res = deduct_fund_balance_impl(&asset_id, amount, FundType::Treasury);
        assert!(matches!(
            deduct_res,
            Err(WithdrawFundError::InsufficientFunds)
        ));

        // Internal balance should remain untouched
        TREASURY.with(|f| {
            assert_eq!(f.borrow().get(&asset_id).copied().unwrap(), 100_000);
        });
    }

    fn seed_account(user: User) {
        ACCOUNT_STATES.with(|accounts| {
            let mut state = AccountState::new(user);
            state.set_balance(BalanceDomain::Settlement, "ICP".to_owned(), 1_000_000);
            state.set_balance(BalanceDomain::Playground, "ckUSDC".to_owned(), 42);
            state.set_cash_balance_usd(BalanceDomain::Settlement, -250_000);
            state.set_reserved_margin_usd(BalanceDomain::Settlement, 100_000);
            accounts.borrow_mut().insert(user, state);
        });
    }

    fn seed_position(user: User, series: &str) {
        let series_id = SeriesId::from(series.to_owned());
        POSITIONS.with(|positions| {
            positions.borrow_mut().insert(
                (user, series_id.clone(), None),
                Position {
                    user,
                    series_id,
                    outcome_id: None,
                    net_qty: 7,
                    reserved_margin_usd: 100_000,
                },
            );
        });
    }

    fn clear_reassign_state() {
        ACCOUNT_STATES.with(|a| a.borrow_mut().clear());
        POSITIONS.with(|p| p.borrow_mut().clear());
        LIMIT_ORDERS.with(|o| o.borrow_mut().clear());
    }

    #[test]
    fn reassign_account_moves_balances_and_positions() {
        clear_reassign_state();
        let old_owner = User(Principal::from_slice(&[201]));
        let new_owner = User(Principal::from_slice(&[202]));

        seed_account(old_owner);
        seed_position(old_owner, "REASSIGN-A");
        seed_position(old_owner, "REASSIGN-B");

        let res = reassign_account_impl(old_owner, new_owner);
        assert!(res.is_ok(), "expected Ok, got {res:?}");

        ACCOUNT_STATES.with(|accounts| {
            let accounts = accounts.borrow();
            assert!(
                accounts.get(&old_owner).is_none(),
                "old account must be gone"
            );

            let state = accounts.get(&new_owner).expect("new account must exist");
            assert_eq!(state.user, new_owner);
            assert_eq!(
                state.get_balance(BalanceDomain::Settlement, &"ICP".to_owned()),
                1_000_000
            );
            assert_eq!(
                state.get_balance(BalanceDomain::Playground, &"ckUSDC".to_owned()),
                42
            );
            assert_eq!(
                state.get_cash_balance_usd(BalanceDomain::Settlement),
                -250_000
            );
            assert_eq!(
                state.get_reserved_margin_usd(BalanceDomain::Settlement),
                100_000
            );
        });

        POSITIONS.with(|positions| {
            let positions = positions.borrow();
            assert!(
                !positions.keys().any(|(u, _, _)| *u == old_owner),
                "old owner must hold no positions"
            );
            let moved: Vec<_> = positions
                .iter()
                .filter(|((u, _, _), _)| *u == new_owner)
                .collect();
            assert_eq!(moved.len(), 2);
            for (_, position) in moved {
                assert_eq!(position.user, new_owner);
                assert_eq!(position.net_qty, 7);
                assert_eq!(position.reserved_margin_usd, 100_000);
            }
        });

        clear_reassign_state();
    }

    #[test]
    fn reassign_account_rejects_same_owner() {
        let owner = User(Principal::from_slice(&[203]));
        assert!(matches!(
            reassign_account_impl(owner, owner),
            Err(ReassignAccountError::SameOwner)
        ));
    }

    #[test]
    fn reassign_account_rejects_missing_account() {
        clear_reassign_state();
        let old_owner = User(Principal::from_slice(&[204]));
        let new_owner = User(Principal::from_slice(&[205]));
        assert!(matches!(
            reassign_account_impl(old_owner, new_owner),
            Err(ReassignAccountError::AccountNotFound)
        ));
    }

    #[test]
    fn reassign_account_rejects_open_orders() {
        clear_reassign_state();
        let old_owner = User(Principal::from_slice(&[206]));
        let new_owner = User(Principal::from_slice(&[207]));

        seed_account(old_owner);
        LIMIT_ORDERS.with(|orders| {
            orders.borrow_mut().insert(
                OrderId::from("reassign_open_order".to_owned()),
                LimitOrder {
                    order_id: OrderId::from("reassign_open_order".to_owned()),
                    creator: old_owner,
                    series_id: SeriesId::from("REASSIGN-ORD".to_owned()),
                    outcome_id: None,
                    side: Side::Buy,
                    qty: 1,
                    price: Price::new(500_000, 6),
                    blocked_margin_usd: 500_000,
                    balance_domain: BalanceDomain::Settlement,
                },
            );
        });

        assert!(matches!(
            reassign_account_impl(old_owner, new_owner),
            Err(ReassignAccountError::OpenOrdersExist)
        ));

        // Nothing moved.
        ACCOUNT_STATES.with(|accounts| {
            assert!(accounts.borrow().contains_key(&old_owner));
            assert!(!accounts.borrow().contains_key(&new_owner));
        });

        clear_reassign_state();
    }

    #[test]
    fn reassign_account_rejects_non_empty_target() {
        clear_reassign_state();
        let old_owner = User(Principal::from_slice(&[208]));
        let new_owner = User(Principal::from_slice(&[209]));

        seed_account(old_owner);
        seed_account(new_owner);

        assert!(matches!(
            reassign_account_impl(old_owner, new_owner),
            Err(ReassignAccountError::TargetAccountNotEmpty)
        ));

        // Both accounts untouched.
        ACCOUNT_STATES.with(|accounts| {
            let accounts = accounts.borrow();
            assert_eq!(accounts.get(&old_owner).unwrap().user, old_owner);
            assert_eq!(accounts.get(&new_owner).unwrap().user, new_owner);
        });

        clear_reassign_state();
    }

    #[test]
    fn reassign_account_allows_empty_target_shell() {
        clear_reassign_state();
        let old_owner = User(Principal::from_slice(&[210]));
        let new_owner = User(Principal::from_slice(&[211]));

        seed_account(old_owner);
        // A drained account shell (no balances, cash, or margins) is not economic
        // state; reassignment replaces it.
        ACCOUNT_STATES.with(|accounts| {
            accounts
                .borrow_mut()
                .insert(new_owner, AccountState::new(new_owner));
        });

        assert!(reassign_account_impl(old_owner, new_owner).is_ok());

        ACCOUNT_STATES.with(|accounts| {
            let accounts = accounts.borrow();
            assert!(accounts.get(&old_owner).is_none());
            assert_eq!(
                accounts
                    .get(&new_owner)
                    .unwrap()
                    .get_balance(BalanceDomain::Settlement, &"ICP".to_owned()),
                1_000_000
            );
        });

        clear_reassign_state();
    }

    #[test]
    fn reassign_account_second_call_fails_cleanly() {
        clear_reassign_state();
        let old_owner = User(Principal::from_slice(&[212]));
        let new_owner = User(Principal::from_slice(&[213]));

        seed_account(old_owner);
        seed_position(old_owner, "REASSIGN-TWICE");

        assert!(reassign_account_impl(old_owner, new_owner).is_ok());

        // The account moved exactly once; replaying the call finds no source
        // account and mutates nothing.
        assert!(matches!(
            reassign_account_impl(old_owner, new_owner),
            Err(ReassignAccountError::AccountNotFound)
        ));

        ACCOUNT_STATES.with(|accounts| {
            let accounts = accounts.borrow();
            assert!(accounts.get(&old_owner).is_none());
            assert_eq!(
                accounts
                    .get(&new_owner)
                    .unwrap()
                    .get_balance(BalanceDomain::Settlement, &"ICP".to_owned()),
                1_000_000
            );
        });
        POSITIONS.with(|positions| {
            assert_eq!(
                positions
                    .borrow()
                    .keys()
                    .filter(|(u, _, _)| *u == new_owner)
                    .count(),
                1
            );
        });

        clear_reassign_state();
    }
}
