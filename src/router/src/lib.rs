//! Router Canister for ICDC Clearing.
//!
//! This canister provides deterministic routing to the correct clearing shards
//! based on the underlying asset and expiry period.

pub mod api;
pub mod memory;

use ic_cdk_macros::{post_upgrade, pre_save};

#[pre_save]
fn pre_save() {
    memory::save_state();
}

#[post_upgrade]
fn post_upgrade() {
    memory::restore_state();
}
