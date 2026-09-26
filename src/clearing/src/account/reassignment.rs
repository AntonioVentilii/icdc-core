//! Account reassignment: moves a whole clearing account, internal maps and
//! on-ledger custody alike, from one principal to another.
//!
//! # Ordering and recovery
//!
//! The internal re-key runs first, in the synchronous prefix of the very
//! message that validated it, so no other call can ever observe a half-moved
//! account. The custody sweep follows, asset by asset, and it runs against a
//! source nobody can feed any more: both principals are locked for the lifetime
//! of the plan, so no deposit, withdrawal, trade, or transfer can touch either
//! of them until the sweep finishes.
//!
//! Custody funds therefore sit in exactly one of two places: the old owner's
//! subaccounts (sweep outstanding, plan not finalised) or the new owner's. A
//! trap or a failed ledger call ends the call with the plan still pending;
//! replaying the same `reassignment_id` resumes it from the first unswept
//! asset. Every pass re-reads the source subaccount and moves exactly the
//! balance it finds there, net of the ledger fee, so a lost response, a
//! duplicated admin call, or a fee that drifted between attempts can never move
//! funds twice or leave the maps and the ledger disagreeing.
//!
//! # Concurrency
//!
//! Durable plans are not enough to serialise this against user calls: a call
//! that passed its guard and is suspended at an await would resume after the
//! re-key and recreate state under the old principal. [`ReassignmentGuard`]
//! closes that window: user-facing mutations snapshot the principal's
//! [`ReassignmentMark`] before their first await and revalidate it right before
//! they mutate, so a call that straddles a reassignment is rejected instead of
//! resurrecting a drained account.

use candid::{Nat, Principal};
use shared::types::{Asset, AssetId};

use crate::{
    api::admin::errors::ReassignAccountError,
    assets::{
        asset::{
            handler::get_handler,
            params::{AssetBalanceOfParams, AssetTransferParams},
        },
        types::AssetAmount,
    },
    memory::{
        cached_transfer_fee, ACCOUNT_STATES, COLLATERAL_ASSETS, DEPOSIT_PLANS, FROZEN_TRANSFERS,
        LIMIT_ORDERS, MIGRATION_PLANS, POSITIONS, REASSIGNMENT_MARKS, REASSIGNMENT_PLANS,
        SETTLEMENT_PLANS, WITHDRAWAL_PLANS,
    },
    types::{
        account::AssetAccount,
        plans::{PlanStatus, ReassignmentKey, ReassignmentPlan, ReassignmentPlanParams},
        user::User,
    },
    utils::system::now_ns,
};

/// Per-principal reassignment bookkeeping (see [`ReassignmentGuard`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReassignmentMark {
    /// Bumped every time this principal's account is re-keyed, in or out.
    pub generation: u64,
    /// Set while a reassignment touching this principal has not finalised.
    pub locked: bool,
}

/// A user mutation raced an account reassignment of the same principal.
#[derive(Clone, Copy, Debug)]
pub struct ReassignmentConflict {
    /// The principal whose account is being, or has just been, reassigned.
    pub user: User,
}

/// Serialises a user-facing mutation against account reassignment.
///
/// Acquire it before the call's first await and revalidate it immediately
/// before the state mutation: acquisition rejects a principal that is already
/// under reassignment, and revalidation rejects one whose account was re-keyed
/// while the call was suspended.
pub struct ReassignmentGuard {
    user: User,
    generation: u64,
}

impl ReassignmentGuard {
    /// Snapshots `user`'s reassignment state, failing if one is already in
    /// flight for that principal.
    pub fn acquire(user: User) -> Result<Self, ReassignmentConflict> {
        let mark = mark_of(user);

        if mark.locked {
            return Err(ReassignmentConflict { user });
        }

        Ok(Self {
            user,
            generation: mark.generation,
        })
    }

    /// Fails if the principal has been reassigned, or has come under
    /// reassignment, since the guard was acquired.
    pub fn revalidate(&self) -> Result<(), ReassignmentConflict> {
        let mark = mark_of(self.user);

        if mark.locked || mark.generation != self.generation {
            return Err(ReassignmentConflict { user: self.user });
        }

        Ok(())
    }
}

fn mark_of(user: User) -> ReassignmentMark {
    REASSIGNMENT_MARKS.with(|marks| marks.borrow().get(&user).copied().unwrap_or_default())
}

fn is_locked(user: User) -> bool {
    mark_of(user).locked
}

/// Marks both principals as under reassignment and bumps their generation, so
/// calls suspended across the re-key are rejected when they resume.
fn lock_for_reassignment(old_owner: User, new_owner: User) {
    REASSIGNMENT_MARKS.with(|marks| {
        let mut marks = marks.borrow_mut();
        for user in [old_owner, new_owner] {
            let mark = marks.entry(user).or_default();
            mark.generation += 1;
            mark.locked = true;
        }
    });
}

fn unlock_after_reassignment(old_owner: User, new_owner: User) {
    REASSIGNMENT_MARKS.with(|marks| {
        let mut marks = marks.borrow_mut();
        for user in [old_owner, new_owner] {
            marks.entry(user).or_default().locked = false;
        }
    });
}

/// Moves the entire clearing account of `old_owner` to `new_owner`: the
/// internal maps first, then the on-ledger custody funds.
///
/// See the [module documentation](self) for the ordering guarantee and for how
/// an interrupted reassignment is resumed.
pub(crate) async fn reassign_account(
    reassignment_id: String,
    old_owner: User,
    new_owner: User,
) -> Result<(), ReassignAccountError> {
    validate_principals(old_owner, new_owner)?;

    let key: ReassignmentKey = (old_owner, reassignment_id.clone());
    let existing = REASSIGNMENT_PLANS.with(|plans| plans.borrow().get(&key).cloned());

    let mut plan = match existing {
        Some(plan) if plan.new_owner != new_owner => {
            return Err(ReassignAccountError::ReassignmentIdReused)
        }
        Some(plan) if plan.status == PlanStatus::Finalised => return Ok(()),
        // A pending plan means the re-key already happened and the locks are
        // still held: resume its custody sweep rather than revalidating an
        // account that has, by design, already moved.
        Some(plan) => plan,
        None => validate_and_rekey(reassignment_id, old_owner, new_owner)?,
    };

    sweep_custody(&mut plan).await?;

    plan.status = PlanStatus::Finalised;
    persist(&plan);
    unlock_after_reassignment(old_owner, new_owner);

    Ok(())
}

/// Rejects principal pairs that no reassignment can make sense of, before the
/// plan is even looked up.
///
/// The anonymous principal is rejected on both sides: every account, position,
/// deposit, and withdrawal API is gated on a non-anonymous caller, so an account
/// assigned to it could never be reached again.
fn validate_principals(old_owner: User, new_owner: User) -> Result<(), ReassignAccountError> {
    if old_owner == new_owner {
        return Err(ReassignAccountError::SameOwner);
    }

    if old_owner.principal() == Principal::anonymous()
        || new_owner.principal() == Principal::anonymous()
    {
        return Err(ReassignAccountError::AnonymousOwner);
    }

    Ok(())
}

/// Runs every guard rail and, if they all pass, re-keys the account and opens
/// the plan for the custody sweep.
///
/// Everything here is synchronous: validation and mutation share one message
/// prefix, so the account is never seen half-moved and no call can slip in
/// between the last check and the re-key.
fn validate_and_rekey(
    reassignment_id: String,
    old_owner: User,
    new_owner: User,
) -> Result<ReassignmentPlan, ReassignAccountError> {
    if is_locked(old_owner) || is_locked(new_owner) {
        return Err(ReassignAccountError::ReassignmentInProgress);
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

    let asset_ids = custody_assets(old_owner)?;

    let mut plan = ReassignmentPlan::get_or_create(ReassignmentPlanParams {
        reassignment_id,
        old_owner,
        new_owner,
        asset_ids,
    });
    plan.status = PlanStatus::Executing;
    persist(&plan);

    lock_for_reassignment(old_owner, new_owner);

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

    Ok(plan)
}

/// Lists the assets whose custody subaccount has to follow the account.
///
/// Every asset the account has ever held is swept, zero internal balance
/// included, because the ledger is the authority on what is actually in the
/// subaccount. Custody that this canister cannot move on-chain (any non-ICRC
/// asset, whose funds live at a per-principal EVM address) rejects the whole
/// reassignment rather than stranding a balance.
fn custody_assets(old_owner: User) -> Result<Vec<AssetId>, ReassignAccountError> {
    let balances = ACCOUNT_STATES.with(|accounts| {
        accounts
            .borrow()
            .get(&old_owner)
            .map(|state| state.balances.clone())
            .unwrap_or_default()
    });

    let mut asset_ids: Vec<AssetId> = Vec::new();

    for domain_balances in balances.values() {
        for (asset_id, balance) in domain_balances {
            let is_icrc = COLLATERAL_ASSETS.with(|assets| {
                assets
                    .borrow()
                    .get(asset_id)
                    .is_some_and(|config| matches!(config.asset, Asset::Icrc(_)))
            });

            if !is_icrc {
                if *balance > 0 {
                    return Err(ReassignAccountError::UnsupportedCustodyAsset {
                        asset_id: asset_id.clone(),
                    });
                }
                continue;
            }

            if !asset_ids.contains(asset_id) {
                asset_ids.push(asset_id.clone());
            }
        }
    }

    Ok(asset_ids)
}

/// Drains each custody subaccount of the old owner into the matching subaccount
/// of the new owner, recording every settled transfer on the plan before moving
/// to the next asset.
///
/// The amount is derived from the ledger, never from internal accounting: the
/// subaccount holds whatever deposits left behind minus the fees past
/// withdrawals burned, and only the ledger knows that figure. Moving
/// `balance - fee` empties the source in one transfer; when the balance does
/// not even cover the fee there is nothing worth moving and the asset is marked
/// done.
async fn sweep_custody(plan: &mut ReassignmentPlan) -> Result<(), ReassignAccountError> {
    for index in 0..plan.sweeps.len() {
        if plan.sweeps[index].moved_amount.is_some() {
            continue;
        }

        let asset_id = plan.sweeps[index].asset_id.clone();

        let config = COLLATERAL_ASSETS
            .with(|assets| assets.borrow().get(&asset_id).cloned())
            .ok_or_else(|| ReassignAccountError::UnsupportedCustodyAsset {
                asset_id: asset_id.clone(),
            })?;

        let handler = get_handler(&config.asset).map_err(|error| {
            ReassignAccountError::CustodyTransferFailed {
                asset_id: asset_id.clone(),
                error,
            }
        })?;

        let balance = handler
            .balance_of(AssetBalanceOfParams {
                asset: &config.asset,
                account: AssetAccount::UserClearing(plan.old_owner),
            })
            .await
            .map_err(|error| ReassignAccountError::CustodyTransferFailed {
                asset_id: asset_id.clone(),
                error,
            })?;

        let fee = match cached_transfer_fee(&asset_id) {
            Some(fee) => fee,
            None => handler.get_fee(&config.asset).await.map_err(|error| {
                ReassignAccountError::CustodyTransferFailed {
                    asset_id: asset_id.clone(),
                    error,
                }
            })?,
        };

        let amount = balance.saturating_sub(fee);

        if amount == 0 {
            plan.sweeps[index].moved_amount = Some(0);
            persist(plan);
            continue;
        }

        let block = handler
            .transfer(AssetTransferParams {
                asset: &config.asset,
                asset_id: &asset_id,
                from: AssetAccount::UserClearing(plan.old_owner),
                to: AssetAccount::UserClearing(plan.new_owner),
                amount: AssetAmount::Fixed(amount),
                // Stamped per attempt on purpose: see `ReassignmentPlan::created_ns`.
                created_at_time_ns: Some(now_ns()),
            })
            .await
            .map_err(|error| ReassignAccountError::CustodyTransferFailed {
                asset_id: asset_id.clone(),
                error,
            })?;

        plan.sweeps[index].moved_amount = Some(amount);
        plan.sweeps[index].receipt = Some(Nat::from(block).into());
        persist(plan);
    }

    Ok(())
}

fn persist(plan: &ReassignmentPlan) {
    REASSIGNMENT_PLANS.with(|plans| {
        plans
            .borrow_mut()
            .insert((plan.old_owner, plan.reassignment_id.clone()), plan.clone());
    });
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

#[cfg(test)]
mod tests {
    use shared::types::{
        evm::NativeEvmAsset, BalanceDomain, CollateralAssetConfig, Price, SeriesId, SettlementInput,
    };

    use super::*;
    use crate::types::{
        margin::{AccountState, Position},
        payment::PaymentIdempotency,
        plans::{DepositPlan, MigrationPlan, SettlementPlan, SettlementPosition, WithdrawalPlan},
        state::PositionProof,
        trade::{LimitOrder, OrderId, Side, TransferId},
        user::{DepositId, WithdrawalId},
    };

    const REASSIGNMENT_ID: &str = "reassign-1";

    fn user(id: u8) -> User {
        User(Principal::from_slice(&[id]))
    }

    fn clear_state() {
        ACCOUNT_STATES.with(|a| a.borrow_mut().clear());
        POSITIONS.with(|p| p.borrow_mut().clear());
        LIMIT_ORDERS.with(|o| o.borrow_mut().clear());
        FROZEN_TRANSFERS.with(|t| t.borrow_mut().clear());
        DEPOSIT_PLANS.with(|p| p.borrow_mut().clear());
        WITHDRAWAL_PLANS.with(|p| p.borrow_mut().clear());
        MIGRATION_PLANS.with(|p| p.borrow_mut().clear());
        SETTLEMENT_PLANS.with(|p| p.borrow_mut().clear());
        REASSIGNMENT_PLANS.with(|p| p.borrow_mut().clear());
        REASSIGNMENT_MARKS.with(|m| m.borrow_mut().clear());
        COLLATERAL_ASSETS.with(|c| c.borrow_mut().clear());
    }

    fn seed_account(owner: User) {
        ACCOUNT_STATES.with(|accounts| {
            let mut state = AccountState::new(owner);
            state.set_balance(BalanceDomain::Settlement, "ICP".to_owned(), 1_000_000);
            state.set_balance(BalanceDomain::Playground, "ckUSDC".to_owned(), 42);
            state.set_cash_balance_usd(BalanceDomain::Settlement, -250_000);
            state.set_reserved_margin_usd(BalanceDomain::Settlement, 100_000);
            accounts.borrow_mut().insert(owner, state);
        });
    }

    fn seed_position(owner: User, series: &str) {
        let series_id = SeriesId::from(series.to_owned());
        POSITIONS.with(|positions| {
            positions.borrow_mut().insert(
                (owner, series_id.clone(), None),
                Position {
                    user: owner,
                    series_id,
                    outcome_id: None,
                    net_qty: 7,
                    reserved_margin_usd: 100_000,
                },
            );
        });
    }

    fn seed_icrc_asset(asset_id: &str) {
        COLLATERAL_ASSETS.with(|assets| {
            assets.borrow_mut().insert(
                asset_id.to_owned(),
                CollateralAssetConfig {
                    asset_id: asset_id.to_owned(),
                    asset: Asset::Icrc(Principal::from_slice(&[9])),
                    symbol: asset_id.to_owned(),
                    decimals: 8,
                    is_enabled: true,
                    oracle_id: None,
                    allowed_balance_domains: vec![
                        BalanceDomain::Settlement,
                        BalanceDomain::Playground,
                    ],
                },
            );
        });
    }

    fn seed_frozen_transfer(owner: User) {
        FROZEN_TRANSFERS.with(|transfers| {
            transfers.borrow_mut().insert(
                TransferId::from("frozen-1".to_owned()),
                PositionProof {
                    transfer_id: TransferId::from("frozen-1".to_owned()),
                    user: owner,
                    series_id: SeriesId::from("FROZEN".to_owned()),
                    outcome_id: None,
                    qty: 1,
                    clearing_id: Principal::from_slice(&[9]),
                    signature: vec![],
                    valuation_price: None,
                },
            );
        });
    }

    fn seed_deposit_plan(owner: User, status: PlanStatus) {
        DEPOSIT_PLANS.with(|plans| {
            plans.borrow_mut().insert(
                (owner, DepositId("dep-1".to_owned())),
                DepositPlan {
                    deposit_id: DepositId("dep-1".to_owned()),
                    user: owner,
                    asset_id: "ICP".to_owned(),
                    amount: Nat::from(1_u64),
                    status,
                    idempotency_ns: PaymentIdempotency::IcrcCreatedAtTimeNs(0),
                    receipt: None,
                },
            );
        });
    }

    fn seed_withdrawal_plan(owner: User, status: PlanStatus) {
        WITHDRAWAL_PLANS.with(|plans| {
            plans.borrow_mut().insert(
                (owner, WithdrawalId("wit-1".to_owned())),
                WithdrawalPlan {
                    withdrawal_id: WithdrawalId("wit-1".to_owned()),
                    user: owner,
                    asset_id: "ICP".to_owned(),
                    amount: Nat::from(1_u64),
                    to_account: (owner.principal(), None),
                    status,
                    idempotency_ns: PaymentIdempotency::IcrcCreatedAtTimeNs(0),
                    receipt: None,
                    reserved_amount: None,
                    reserved_cash_usd: None,
                },
            );
        });
    }

    fn seed_migration_plan(owner: User, status: PlanStatus) {
        MIGRATION_PLANS.with(|plans| {
            plans.borrow_mut().insert(
                (owner, "mig-1".to_owned()),
                MigrationPlan {
                    migration_id: "mig-1".to_owned(),
                    user: owner,
                    from_domain: BalanceDomain::Playground,
                    to_domain: BalanceDomain::Settlement,
                    status,
                    idempotency_ns: PaymentIdempotency::IcrcCreatedAtTimeNs(0),
                    positions: vec![],
                    orders: vec![],
                    balances: vec![],
                    cash_balance_usd: 0,
                    reserved_margin_usd: 0,
                },
            );
        });
    }

    fn seed_settlement_plan(owner: User, status: PlanStatus) {
        let series_id = SeriesId::from("SETTLING".to_owned());
        SETTLEMENT_PLANS.with(|plans| {
            plans.borrow_mut().insert(
                series_id.clone(),
                SettlementPlan {
                    series_id,
                    settlement: SettlementInput::Price(Price::new(1_000_000, 6)),
                    oracle_source: "oracle".to_owned(),
                    fee_usd: 0,
                    insurance_fee_usd: 0,
                    positions: vec![SettlementPosition {
                        user: owner,
                        outcome_id: None,
                        net_qty: 1,
                        reserved_margin_usd: 0,
                        cashflow_usd: 0,
                    }],
                    accounting_cursor: 0,
                    accounting_applied: false,
                    status,
                    idempotency_ns: PaymentIdempotency::IcrcCreatedAtTimeNs(0),
                    balance_domain: BalanceDomain::Settlement,
                },
            );
        });
    }

    fn rekey(old_owner: User, new_owner: User) -> Result<ReassignmentPlan, ReassignAccountError> {
        validate_and_rekey(REASSIGNMENT_ID.to_owned(), old_owner, new_owner)
    }

    /// Asserts that a rejected reassignment left both sides exactly as they were:
    /// the old owner still owns the account and its positions, the new owner owns
    /// nothing, and no plan or lock was opened.
    fn assert_untouched(old_owner: User, new_owner: User, positions: usize) {
        ACCOUNT_STATES.with(|accounts| {
            let accounts = accounts.borrow();
            let state = accounts
                .get(&old_owner)
                .expect("old owner must keep their account");
            assert_eq!(state.user, old_owner);
            assert_eq!(
                state.get_balance(BalanceDomain::Settlement, &"ICP".to_owned()),
                1_000_000
            );
            assert_eq!(
                state.get_cash_balance_usd(BalanceDomain::Settlement),
                -250_000
            );
            assert_eq!(
                state.get_reserved_margin_usd(BalanceDomain::Settlement),
                100_000
            );
            assert!(
                !accounts.contains_key(&new_owner),
                "new owner must stay empty"
            );
        });

        POSITIONS.with(|p| {
            let p = p.borrow();
            assert_eq!(
                p.keys().filter(|(u, _, _)| *u == old_owner).count(),
                positions
            );
            assert_eq!(p.keys().filter(|(u, _, _)| *u == new_owner).count(), 0);
        });

        REASSIGNMENT_PLANS.with(|plans| assert!(plans.borrow().is_empty(), "no plan was opened"));
        assert!(!is_locked(old_owner), "old owner must not be locked");
        assert!(!is_locked(new_owner), "new owner must not be locked");
    }

    #[test]
    fn rejects_same_owner() {
        let owner = user(201);
        assert!(matches!(
            validate_principals(owner, owner),
            Err(ReassignAccountError::SameOwner)
        ));
    }

    #[test]
    fn rejects_anonymous_owner_on_either_side() {
        let owner = user(202);
        let anonymous = User(Principal::anonymous());

        assert!(matches!(
            validate_principals(owner, anonymous),
            Err(ReassignAccountError::AnonymousOwner)
        ));
        assert!(matches!(
            validate_principals(anonymous, owner),
            Err(ReassignAccountError::AnonymousOwner)
        ));
    }

    #[test]
    fn rekey_moves_balances_and_positions() {
        clear_state();
        let (old_owner, new_owner) = (user(203), user(204));
        seed_icrc_asset("ICP");
        seed_icrc_asset("ckUSDC");
        seed_account(old_owner);
        seed_position(old_owner, "REASSIGN-A");
        seed_position(old_owner, "REASSIGN-B");

        let plan = rekey(old_owner, new_owner).expect("re-key must succeed");

        assert_eq!(plan.status, PlanStatus::Executing);
        let mut swept: Vec<_> = plan.sweeps.iter().map(|s| s.asset_id.clone()).collect();
        swept.sort();
        assert_eq!(
            swept,
            vec!["ICP".to_owned(), "ckUSDC".to_owned()],
            "every custody asset the account ever held is queued for the sweep"
        );

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

        // Both principals stay locked until the custody sweep finalises the plan.
        assert!(is_locked(old_owner));
        assert!(is_locked(new_owner));
    }

    #[test]
    fn rejects_missing_account() {
        clear_state();
        assert!(matches!(
            rekey(user(205), user(206)),
            Err(ReassignAccountError::AccountNotFound)
        ));
    }

    #[test]
    fn a_fresh_reassignment_of_a_moved_account_finds_no_source() {
        clear_state();
        let (old_owner, new_owner) = (user(207), user(208));
        seed_icrc_asset("ICP");
        seed_icrc_asset("ckUSDC");
        seed_account(old_owner);

        assert!(rekey(old_owner, new_owner).is_ok());
        unlock_after_reassignment(old_owner, new_owner);

        assert!(matches!(
            validate_and_rekey("reassign-2".to_owned(), old_owner, user(240)),
            Err(ReassignAccountError::AccountNotFound)
        ));
    }

    #[test]
    fn rejects_open_orders() {
        clear_state();
        let (old_owner, new_owner) = (user(209), user(210));
        seed_icrc_asset("ICP");
        seed_icrc_asset("ckUSDC");
        seed_account(old_owner);
        seed_position(old_owner, "REASSIGN-ORD");
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
            rekey(old_owner, new_owner),
            Err(ReassignAccountError::OpenOrdersExist)
        ));
        assert_untouched(old_owner, new_owner, 1);
    }

    #[test]
    fn rejects_frozen_position_transfers() {
        clear_state();
        let (old_owner, new_owner) = (user(211), user(212));
        seed_icrc_asset("ICP");
        seed_icrc_asset("ckUSDC");
        seed_account(old_owner);
        seed_position(old_owner, "REASSIGN-FROZEN");
        seed_frozen_transfer(old_owner);

        assert!(matches!(
            rekey(old_owner, new_owner),
            Err(ReassignAccountError::PendingPositionTransfersExist)
        ));
        assert_untouched(old_owner, new_owner, 1);
    }

    #[test]
    fn rejects_in_flight_deposit_plan_on_either_side() {
        for (offset, on_new_owner) in [(0_u8, false), (1_u8, true)] {
            clear_state();
            let (old_owner, new_owner) = (user(213 + offset), user(215 + offset));
            seed_icrc_asset("ICP");
            seed_icrc_asset("ckUSDC");
            seed_account(old_owner);
            seed_position(old_owner, "REASSIGN-DEP");
            seed_deposit_plan(
                if on_new_owner { new_owner } else { old_owner },
                PlanStatus::Executing,
            );

            assert!(matches!(
                rekey(old_owner, new_owner),
                Err(ReassignAccountError::InFlightPlansExist)
            ));
            assert_untouched(old_owner, new_owner, 1);
        }
    }

    #[test]
    fn rejects_in_flight_withdrawal_plan() {
        clear_state();
        let (old_owner, new_owner) = (user(217), user(218));
        seed_icrc_asset("ICP");
        seed_icrc_asset("ckUSDC");
        seed_account(old_owner);
        seed_position(old_owner, "REASSIGN-WIT");
        seed_withdrawal_plan(new_owner, PlanStatus::Planned);

        assert!(matches!(
            rekey(old_owner, new_owner),
            Err(ReassignAccountError::InFlightPlansExist)
        ));
        assert_untouched(old_owner, new_owner, 1);
    }

    #[test]
    fn rejects_in_flight_migration_plan() {
        clear_state();
        let (old_owner, new_owner) = (user(219), user(220));
        seed_icrc_asset("ICP");
        seed_icrc_asset("ckUSDC");
        seed_account(old_owner);
        seed_position(old_owner, "REASSIGN-MIG");
        seed_migration_plan(old_owner, PlanStatus::Executing);

        assert!(matches!(
            rekey(old_owner, new_owner),
            Err(ReassignAccountError::InFlightPlansExist)
        ));
        assert_untouched(old_owner, new_owner, 1);
    }

    #[test]
    fn rejects_in_flight_settlement_plan() {
        clear_state();
        let (old_owner, new_owner) = (user(221), user(222));
        seed_icrc_asset("ICP");
        seed_icrc_asset("ckUSDC");
        seed_account(old_owner);
        seed_position(old_owner, "REASSIGN-SET");
        seed_settlement_plan(old_owner, PlanStatus::Executing);

        assert!(matches!(
            rekey(old_owner, new_owner),
            Err(ReassignAccountError::InFlightPlansExist)
        ));
        assert_untouched(old_owner, new_owner, 1);
    }

    #[test]
    fn finalised_plans_do_not_block() {
        clear_state();
        let (old_owner, new_owner) = (user(223), user(224));
        seed_icrc_asset("ICP");
        seed_icrc_asset("ckUSDC");
        seed_account(old_owner);
        seed_deposit_plan(old_owner, PlanStatus::Finalised);
        seed_withdrawal_plan(new_owner, PlanStatus::Finalised);
        seed_migration_plan(old_owner, PlanStatus::Finalised);
        seed_settlement_plan(old_owner, PlanStatus::Finalised);

        assert!(rekey(old_owner, new_owner).is_ok());
    }

    #[test]
    fn rejects_non_empty_target() {
        clear_state();
        let (old_owner, new_owner) = (user(225), user(226));
        seed_icrc_asset("ICP");
        seed_icrc_asset("ckUSDC");
        seed_account(old_owner);
        seed_account(new_owner);

        assert!(matches!(
            rekey(old_owner, new_owner),
            Err(ReassignAccountError::TargetAccountNotEmpty)
        ));

        ACCOUNT_STATES.with(|accounts| {
            let accounts = accounts.borrow();
            assert_eq!(accounts.get(&old_owner).unwrap().user, old_owner);
            assert_eq!(accounts.get(&new_owner).unwrap().user, new_owner);
        });
        REASSIGNMENT_PLANS.with(|plans| assert!(plans.borrow().is_empty()));
    }

    #[test]
    fn allows_empty_target_shell() {
        clear_state();
        let (old_owner, new_owner) = (user(227), user(228));
        seed_icrc_asset("ICP");
        seed_icrc_asset("ckUSDC");
        seed_account(old_owner);
        // A drained account shell (no balances, cash, or margins) is not economic
        // state; reassignment replaces it.
        ACCOUNT_STATES.with(|accounts| {
            accounts
                .borrow_mut()
                .insert(new_owner, AccountState::new(new_owner));
        });

        assert!(rekey(old_owner, new_owner).is_ok());

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
    }

    #[test]
    fn rejects_custody_this_canister_cannot_move() {
        clear_state();
        let (old_owner, new_owner) = (user(229), user(230));
        seed_icrc_asset("ICP");
        COLLATERAL_ASSETS.with(|assets| {
            assets.borrow_mut().insert(
                "ckUSDC".to_owned(),
                CollateralAssetConfig {
                    asset_id: "ckUSDC".to_owned(),
                    asset: Asset::NativeEvm(NativeEvmAsset {
                        chain_id: 1,
                        decimals: 18,
                    }),
                    symbol: "ckUSDC".to_owned(),
                    decimals: 18,
                    is_enabled: true,
                    oracle_id: None,
                    allowed_balance_domains: vec![BalanceDomain::Playground],
                },
            );
        });
        seed_account(old_owner);
        seed_position(old_owner, "REASSIGN-EVM");

        assert!(matches!(
            rekey(old_owner, new_owner),
            Err(ReassignAccountError::UnsupportedCustodyAsset { asset_id }) if asset_id == "ckUSDC"
        ));
        assert_untouched(old_owner, new_owner, 1);
    }

    #[test]
    fn rejects_a_second_reassignment_while_one_is_pending() {
        clear_state();
        let (old_owner, new_owner, other) = (user(231), user(232), user(233));
        seed_icrc_asset("ICP");
        seed_icrc_asset("ckUSDC");
        seed_account(old_owner);
        seed_account(other);

        assert!(rekey(old_owner, new_owner).is_ok());

        // The pending plan holds both principals; the source of a second
        // reassignment into the same target is turned away.
        assert!(matches!(
            validate_and_rekey("reassign-2".to_owned(), other, new_owner),
            Err(ReassignAccountError::ReassignmentInProgress)
        ));

        unlock_after_reassignment(old_owner, new_owner);
        assert!(validate_and_rekey("reassign-2".to_owned(), other, user(234)).is_ok());
    }

    #[test]
    fn guard_rejects_a_principal_under_reassignment() {
        clear_state();
        let (old_owner, new_owner) = (user(235), user(236));
        seed_icrc_asset("ICP");
        seed_icrc_asset("ckUSDC");
        seed_account(old_owner);

        let guard = ReassignmentGuard::acquire(old_owner).expect("no reassignment yet");

        assert!(rekey(old_owner, new_owner).is_ok());

        // A call suspended across the re-key is rejected when it resumes, both
        // while the sweep is still running ...
        assert!(guard.revalidate().is_err());
        assert!(ReassignmentGuard::acquire(old_owner).is_err());
        assert!(ReassignmentGuard::acquire(new_owner).is_err());

        // ... and once the reassignment has finalised, because the principal's
        // generation moved on.
        unlock_after_reassignment(old_owner, new_owner);
        assert!(guard.revalidate().is_err());
        assert!(ReassignmentGuard::acquire(old_owner).is_ok());
    }

    #[test]
    fn guard_lets_untouched_principals_through() {
        clear_state();
        let (old_owner, new_owner, bystander) = (user(237), user(238), user(239));
        seed_icrc_asset("ICP");
        seed_icrc_asset("ckUSDC");
        seed_account(old_owner);

        let guard = ReassignmentGuard::acquire(bystander).expect("no reassignment yet");

        assert!(rekey(old_owner, new_owner).is_ok());

        assert!(
            guard.revalidate().is_ok(),
            "an unrelated principal is never affected"
        );
    }
}
