use core::cell::RefCell;
use std::collections::BTreeMap;

use candid::Principal;
use ic_cdk::{storage, trap};
use shared::types::{Oracle, Series, SeriesId};

thread_local! {
    /// Global stable storage for all registered derivative series.
    pub static SERIES_STORE: RefCell<BTreeMap<SeriesId, Series>> = const { RefCell::new(BTreeMap::new()) };
    /// Global stable storage for authorised oracles and their metadata.
    pub static ORACLE_STORE: RefCell<BTreeMap<String, Oracle>> = const { RefCell::new(BTreeMap::new()) };
    /// Global stable storage for authorised series creators.
    pub static AUTHORIZED_CREATORS: RefCell<BTreeMap<Principal, bool>> = const { RefCell::new(BTreeMap::new()) };
}

pub fn save_state() {
    let state = (
        SERIES_STORE.with(|s| return s.borrow().clone()),
        ORACLE_STORE.with(|o| return o.borrow().clone()),
        AUTHORIZED_CREATORS.with(|a| return a.borrow().clone()),
    );

    storage::stable_save(state)
        .unwrap_or_else(|e| trap(format!("Failed to save to stable storage: {e:?}")));
}

pub fn restore_state() {
    let (series, oracles, creators): (
        BTreeMap<SeriesId, Series>,
        BTreeMap<String, Oracle>,
        BTreeMap<Principal, bool>,
    ) = storage::stable_restore()
        .unwrap_or_else(|e| trap(format!("Failed to restore from stable storage: {e:?}")));

    SERIES_STORE.with(|w| *w.borrow_mut() = series);
    ORACLE_STORE.with(|w| *w.borrow_mut() = oracles);
    AUTHORIZED_CREATORS.with(|w| *w.borrow_mut() = creators);
}
