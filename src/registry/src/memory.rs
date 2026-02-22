use std::{cell::RefCell, collections::HashMap};

use ic_cdk::storage;
use shared::Series;

thread_local! {
    pub static SERIES_STORE: RefCell<HashMap<String, Series>> = RefCell::new(HashMap::new());
}

pub fn save_state() {
    SERIES_STORE.with(|store| {
        storage::stable_save((store.borrow().clone(),)).expect("Failed to save to stable storage");
    });
}

pub fn restore_state() {
    let (old_store,): (HashMap<String, Series>,) =
        storage::stable_restore().expect("Failed to restore from stable storage");
    SERIES_STORE.with(|store| {
        *store.borrow_mut() = old_store;
    });
}
