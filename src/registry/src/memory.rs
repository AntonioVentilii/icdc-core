use std::{cell::RefCell, collections::BTreeMap};

use ic_cdk::storage;
use shared::types::{Oracle, Series, SeriesId};

thread_local! {
    /// Global stable storage for all registered derivative series.
    pub static SERIES_STORE: RefCell<BTreeMap<SeriesId, Series>> = const { RefCell::new(BTreeMap::new()) };
    /// Global stable storage for authorised oracles and their metadata.
    pub static ORACLE_STORE: RefCell<BTreeMap<String, Oracle>> = const { RefCell::new(BTreeMap::new()) };
}

pub fn save_state() {
    let state = (
        SERIES_STORE.with(|s| s.borrow().clone()),
        ORACLE_STORE.with(|o| o.borrow().clone()),
    );
    storage::stable_save(state).expect("Failed to save to stable storage");
}

pub fn restore_state() {
    let (series, oracles): (BTreeMap<SeriesId, Series>, BTreeMap<String, Oracle>) =
        storage::stable_restore().expect("Failed to restore from stable storage");

    SERIES_STORE.with(|w| *w.borrow_mut() = series);
    ORACLE_STORE.with(|w| *w.borrow_mut() = oracles);
}
