use std::{cell::RefCell, collections::BTreeMap};

use ic_cdk::storage;
use shared::types::{Series, SeriesId};

thread_local! {
    /// Global stable storage for all registered derivative series.
    pub static SERIES_STORE: RefCell<BTreeMap<SeriesId, Series>> = const { RefCell::new(BTreeMap::new()) };

    /// Mapping from underlying ticker (uppercase) to assigned canonical ID.
    pub static UNDERLYING_IDS: RefCell<BTreeMap<String, u32>> = const { RefCell::new(BTreeMap::new()) };

    /// Counter for generating unique underlying IDs.
    pub static UNDERLYING_ID_COUNTER: RefCell<u32> = const { RefCell::new(0) };
}

/// Retrieves the existing canonical ID for an underlying ticker or creates a new one.
///
/// Tickers are converted to uppercase before lookup.
pub fn get_or_create_underlying_id(ticker: &str) -> u32 {
    let ticker = ticker.to_uppercase();
    UNDERLYING_IDS.with(|ids| {
        let mut ids = ids.borrow_mut();
        if let Some(&id) = ids.get(&ticker) {
            id
        } else {
            let next_id = UNDERLYING_ID_COUNTER.with(|counter| {
                let mut counter = counter.borrow_mut();
                *counter += 1;
                *counter
            });
            ids.insert(ticker, next_id);
            next_id
        }
    })
}

pub fn save_state() {
    SERIES_STORE.with(|store| {
        UNDERLYING_IDS.with(|ids| {
            UNDERLYING_ID_COUNTER.with(|counter| {
                storage::stable_save((
                    store.borrow().clone(),
                    ids.borrow().clone(),
                    *counter.borrow(),
                ))
                .expect("Failed to save to stable storage");
            })
        })
    });
}

pub fn restore_state() {
    let (series_data, ids_data, counter): (BTreeMap<SeriesId, Series>, BTreeMap<String, u32>, u32) =
        storage::stable_restore().expect("Failed to restore from stable storage");

    SERIES_STORE.with(|w| *w.borrow_mut() = series_data);
    UNDERLYING_IDS.with(|w| *w.borrow_mut() = ids_data);
    UNDERLYING_ID_COUNTER.with(|w| *w.borrow_mut() = counter);
}
