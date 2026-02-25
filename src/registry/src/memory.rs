use std::{cell::RefCell, collections::BTreeMap};

use ic_cdk::storage;
use shared::types::{Series, SeriesId};

thread_local! {
    /// Global stable storage for all registered derivative series.
    pub static SERIES_STORE: RefCell<BTreeMap<SeriesId, Series>> = const { RefCell::new(BTreeMap::new()) };
}

pub fn save_state() {
    SERIES_STORE.with(|store| {
        storage::stable_save((store.borrow().clone(),)).expect("Failed to save to stable storage");
    });
}

pub fn restore_state() {
    let (state,): (BTreeMap<SeriesId, Series>,) =
        storage::stable_restore().expect("Failed to restore from stable storage");

    let series = state.into_iter().collect();

    SERIES_STORE.with(|w| *w.borrow_mut() = series);
}
