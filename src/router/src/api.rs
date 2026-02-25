use ic_cdk_macros::{query, update};
use candid::Principal;
use crate::memory::{ShardKey, SHARD_MAP};

/// Resolves the principal of the clearing canister responsible for a specific shard.
///
/// # Arguments
/// * `underlying_id` - The canonical ID of the underlying asset.
/// * `expiry_month` - The expiry period in YYYYMM format.
///
/// # Returns
/// * `Some(Principal)` if a shard is registered for the given key.
/// * `None` otherwise.
#[query]
pub fn resolve_clearing(underlying_id: u32, expiry_month: u32) -> Option<Principal> {
    let key = ShardKey { underlying_id, expiry_month };
    SHARD_MAP.with(|map| map.borrow().get(&key).cloned())
}

/// Registers a clearing canister to handle a specific shard.
///
/// # Arguments
/// * `underlying_id` - The canonical ID of the underlying asset.
/// * `expiry_month` - The expiry period in YYYYMM format.
/// * `clearing_id` - The principal of the target clearing canister.
#[update]
pub fn register_shard(underlying_id: u32, expiry_month: u32, clearing_id: Principal) {
    // TODO: Add access control (only controllers)
    let key = ShardKey { underlying_id, expiry_month };
    SHARD_MAP.with(|map| {
        map.borrow_mut().insert(key, clearing_id);
    });
}

/// Returns a complete list of all registered clearing shards.
#[query]
pub fn list_shards() -> Vec<(ShardKey, Principal)> {
    SHARD_MAP.with(|map| {
        map.borrow()
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    })
}
