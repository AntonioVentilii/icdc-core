use std::{cell::RefCell, collections::HashMap};

use candid::Principal;
use ic_cdk::storage;
use shared::{Event, MarginAccount, Position};

thread_local! {
    pub static POSITIONS: RefCell<HashMap<(Principal, String), Position>> = RefCell::new(HashMap::new());
    pub static MARGIN_ACCOUNTS: RefCell<HashMap<Principal, MarginAccount>> = RefCell::new(HashMap::new());
    pub static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    pub static NEXT_EVENT_ID: RefCell<u64> = const { RefCell::new(0) };
}

pub fn save_state() {
    let positions: Vec<Position> = POSITIONS.with(|p| p.borrow().values().cloned().collect());
    let accounts: Vec<MarginAccount> =
        MARGIN_ACCOUNTS.with(|a| a.borrow().values().cloned().collect());
    let events: Vec<Event> = EVENTS.with(|e| e.borrow().clone());
    let next_id: u64 = NEXT_EVENT_ID.with(|id| *id.borrow());

    storage::stable_save((positions, accounts, events, next_id)).expect("Save failed");
}

pub fn restore_state() {
    let (positions, accounts, events, next_id): (
        Vec<Position>,
        Vec<MarginAccount>,
        Vec<Event>,
        u64,
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

    EVENTS.with(|e| *e.borrow_mut() = events);
    NEXT_EVENT_ID.with(|id| *id.borrow_mut() = next_id);
}
