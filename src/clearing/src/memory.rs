use std::{cell::RefCell, collections::BTreeMap};

use candid::Principal;
use ic_cdk::storage;
use shared::{
    constants::{CKUSDC_LEDGER, ICP_LEDGER},
    types::{Series, SeriesId},
};

use crate::types::{
    event::Event,
    margin::{MarginAccount, Position},
    plan::{DepositPlan, WithdrawalPlan},
    state::StableState,
    user::{DepositId, User, WithdrawalId},
};

thread_local! {
    pub static POSITIONS: RefCell<BTreeMap<(User, SeriesId), Position>> = const { RefCell::new(BTreeMap::new()) };
    pub static MARGIN_ACCOUNTS: RefCell<BTreeMap<User, MarginAccount>> = const { RefCell::new(BTreeMap::new()) };
    pub static SERIES: RefCell<BTreeMap<SeriesId, Series>> = const { RefCell::new(BTreeMap::new()) };
    pub static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    pub static NEXT_EVENT_ID: RefCell<u64> = const { RefCell::new(0) };
    pub static REGISTRY_CANISTER: RefCell<Principal> = const { RefCell::new(Principal::anonymous()) };
    pub static DEPOSIT_PLANS: RefCell<BTreeMap<DepositId, DepositPlan>> = const { RefCell::new(BTreeMap::new()) };
    pub static WITHDRAWAL_PLANS: RefCell<BTreeMap<WithdrawalId, WithdrawalPlan>> = const { RefCell::new(BTreeMap::new()) };
}

pub fn save_state() {
    let positions: Vec<Position> = POSITIONS.with(|p| p.borrow().values().cloned().collect());
    let accounts: BTreeMap<User, MarginAccount> = MARGIN_ACCOUNTS.with(|a| a.borrow().clone());
    let series: BTreeMap<SeriesId, Series> = SERIES.with(|s| s.borrow().clone());
    let events: Vec<Event> = EVENTS.with(|e| e.borrow().clone());
    let next_id: u64 = NEXT_EVENT_ID.with(|id| *id.borrow());
    let registry: Principal = REGISTRY_CANISTER.with(|r| *r.borrow());
    let deposit_plans: BTreeMap<DepositId, DepositPlan> =
        DEPOSIT_PLANS.with(|d| d.borrow().clone());
    let withdrawal_plans: BTreeMap<WithdrawalId, WithdrawalPlan> =
        WITHDRAWAL_PLANS.with(|w| w.borrow().clone());

    let state = StableState {
        positions,
        accounts,
        series,
        events,
        next_id,
        registry,
        deposit_plans,
        withdrawal_plans,
    };

    storage::stable_save((state,)).expect("Save failed");
}

pub fn restore_state() {
    let (state,): (StableState,) = storage::stable_restore().expect("Restore failed");

    let StableState {
        positions,
        accounts,
        series,
        events,
        next_id,
        registry,
        deposit_plans,
        withdrawal_plans,
    } = state;

    POSITIONS.with(|p| {
        let mut p = p.borrow_mut();
        for pos in positions {
            p.insert((pos.user, pos.series_id.clone()), pos);
        }
    });

    MARGIN_ACCOUNTS.with(|w| *w.borrow_mut() = accounts);
    SERIES.with(|s| *s.borrow_mut() = series);
    EVENTS.with(|e| *e.borrow_mut() = events);
    NEXT_EVENT_ID.with(|id| *id.borrow_mut() = next_id);
    REGISTRY_CANISTER.with(|r| *r.borrow_mut() = registry);
    DEPOSIT_PLANS.with(|d| *d.borrow_mut() = deposit_plans);
    WITHDRAWAL_PLANS.with(|w| *w.borrow_mut() = withdrawal_plans);
}

pub fn icp_ledger() -> Principal {
    Principal::from_text(ICP_LEDGER).expect("invalid ICP_LEDGER")
}

pub fn ckusdc_ledger() -> Principal {
    Principal::from_text(CKUSDC_LEDGER).expect("invalid CKUSDC_LEDGER")
}
