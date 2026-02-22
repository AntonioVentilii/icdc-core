use std::{cell::RefCell, collections::HashMap};

use candid::Principal;
use ic_cdk::storage;
use shared::{
    constants::{CKUSDC_LEDGER, ICP_LEDGER},
    types::{Event, MarginAccount, Position, Series},
};

thread_local! {
    pub static POSITIONS: RefCell<HashMap<(Principal, String), Position>> = RefCell::new(HashMap::new());
    pub static MARGIN_ACCOUNTS: RefCell<HashMap<Principal, MarginAccount>> = RefCell::new(HashMap::new());
    pub static SERIES: RefCell<HashMap<String, Series>> = RefCell::new(HashMap::new());
    pub static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    pub static NEXT_EVENT_ID: RefCell<u64> = const { RefCell::new(0) };
    pub static REGISTRY_CANISTER: RefCell<Principal> = const { RefCell::new(Principal::anonymous()) };
}

pub fn save_state() {
    let positions: Vec<Position> = POSITIONS.with(|p| p.borrow().values().cloned().collect());
    let accounts: Vec<MarginAccount> =
        MARGIN_ACCOUNTS.with(|a| a.borrow().values().cloned().collect());
    let series: Vec<Series> = SERIES.with(|s| s.borrow().values().cloned().collect());
    let events: Vec<Event> = EVENTS.with(|e| e.borrow().clone());
    let next_id: u64 = NEXT_EVENT_ID.with(|id| *id.borrow());
    let registry: Principal = REGISTRY_CANISTER.with(|r| *r.borrow());

    storage::stable_save((positions, accounts, series, events, next_id, registry))
        .expect("Save failed");
}

pub fn restore_state() {
    let (positions, accounts, series, events, next_id, registry): (
        Vec<Position>,
        Vec<MarginAccount>,
        Vec<Series>,
        Vec<Event>,
        u64,
        Principal,
    ) = storage::stable_restore().expect("Restore failed");

    POSITIONS.with(|p| {
        let mut p = p.borrow_mut();
        for pos in positions {
            p.insert((pos.user, pos.series_id.clone()), pos);
        }
    });

    MARGIN_ACCOUNTS.with(|a| {
        let mut a = a.borrow_mut();
        for acc in accounts {
            a.insert(acc.user, acc);
        }
    });

    SERIES.with(|s| {
        let mut s = s.borrow_mut();
        for ser in series {
            s.insert(ser.series_id.clone(), ser);
        }
    });

    EVENTS.with(|e| *e.borrow_mut() = events);
    NEXT_EVENT_ID.with(|id| *id.borrow_mut() = next_id);
    REGISTRY_CANISTER.with(|r| *r.borrow_mut() = registry);
}

pub fn icp_ledger() -> Principal {
    Principal::from_text(ICP_LEDGER).expect("invalid ICP_LEDGER")
}

pub fn ckusdc_ledger() -> Principal {
    Principal::from_text(CKUSDC_LEDGER).expect("invalid CKUSDC_LEDGER")
}
