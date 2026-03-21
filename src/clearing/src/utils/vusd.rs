use candid::Principal;
use shared::types::{Asset, AssetId};

use crate::memory::CONFIG;

/// Checks if the provided Principal matches the configured internal ledger ID (vUSD).
pub fn is_internal_ledger(ledger_id: &Principal) -> bool {
    CONFIG.with(|c| match &c.borrow().internal_ledger.asset {
        Asset::Icrc(p) => p == ledger_id,
        _ => false,
    })
}

/// Checks if the provided `AssetId` matches the configured internal ledger's asset ID.
pub fn is_internal_asset(asset_id: &AssetId) -> bool {
    CONFIG.with(|c| &c.borrow().internal_ledger.asset_id == asset_id)
}

/// Returns the configured internal ledger ID (vUSD).
#[expect(dead_code)]
pub fn get_internal_ledger_id() -> Principal {
    CONFIG.with(|c| match c.borrow().internal_ledger.asset {
        Asset::Icrc(p) => p,
        _ => panic!("Internal ledger must be an ICRC asset"),
    })
}

/// Returns the configured internal asset ID.
pub fn get_internal_asset_id() -> AssetId {
    CONFIG.with(|c| c.borrow().internal_ledger.asset_id.clone())
}
