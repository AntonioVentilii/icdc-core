//! The Series Registry canister provides a centralised directory for derivative contract series.
//! It allows for registration and discovery of [`Series`] by their canonical identifiers.

pub mod api;
pub mod errors;
pub mod guards;
pub mod memory;
pub mod params;
pub mod results;
pub mod utils;

use ic_cdk::export_candid;
use ic_cdk_macros::{post_upgrade, pre_upgrade};
use shared::types::{Oracle, Series, SeriesId};

pub use crate::{
    errors::{OracleError, SeriesError},
    params::{
        AddOracleParams, AddSeriesParams, ListSeriesParams, ManageOraclePrincipalsParams,
        PaginationParams, UpdateOracleMetadataParams,
    },
    results::{AddSeriesResult, OracleResult, SeriesPage},
};

#[pre_upgrade]
fn pre_upgrade() {
    memory::save_state();
}

#[post_upgrade]
fn post_upgrade() {
    memory::restore_state();
}

export_candid!();
