pub mod api;
pub mod error;
pub mod memory;
pub mod params;
pub mod utils;

use ic_cdk::export_candid;
use ic_cdk_macros::{post_upgrade, pre_upgrade};
use shared::Series;

use crate::{error::RegistryError, params::AddSeriesParams};

#[pre_upgrade]
fn pre_upgrade() {
    memory::save_state();
}

#[post_upgrade]
fn post_upgrade() {
    memory::restore_state();
}

export_candid!();
