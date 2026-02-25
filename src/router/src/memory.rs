use std::{cell::RefCell, collections::BTreeMap};
use candid::{CandidType, Deserialize};
use ic_cdk::storage;

/// A unique key for a clearing shard, combining the underlying asset and expiry period.
#[derive(CandidType, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ShardKey {
    /// The canonical ID of the underlying asset.
    pub underlying_id: u32,
    /// The expiry month in YYYYMM format.
    pub expiry_month: u32,
}

thread_local! {
    /// Global map of shard keys to their responsible clearing canister principals.
    pub static SHARD_MAP: RefCell<BTreeMap<ShardKey, candid::Principal>> = const { RefCell::new(BTreeMap::new()) };
}

/// Saves the router's internal state to stable storage.
pub fn save_state() {
    SHARD_MAP.with(|map| {
        storage::stable_save((map.borrow().clone(),)).expect("Failed to save router state");
    });
}

/// Restores the router's internal state from stable storage.
pub fn restore_state() {
    let (map,): (BTreeMap<ShardKey, candid::Principal>,) = 
        storage::stable_restore().expect("Failed to restore router state");
    SHARD_MAP.with(|m| *m.borrow_mut() = map);
}
