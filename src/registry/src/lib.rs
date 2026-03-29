//! The Series Registry canister provides a centralised directory for derivative contract series.
//! It allows for registration and discovery of [`Series`] by their canonical identifiers.

pub mod api;
pub mod guards;
pub mod memory;
pub mod utils;

use candid::Principal;
use ic_cdk::export_candid;
use ic_cdk_macros::{post_upgrade, pre_upgrade};
pub use shared::types::{
    groups::{
        CreateGroupParams, CreateGroupResult, Group, GroupError, GroupId, GroupResult,
        UpdateGroupMembersParams, UpdateTradingAccessParams,
    },
    oracle::{
        AddOracleParams, ManageOraclePrincipalsParams, Oracle, OracleError, OracleResult,
        UpdateOracleMetadataParams,
    },
    series::{
        AddSeriesParams, AddSeriesResult, ListSeriesParams, PaginationParams, Series, SeriesError,
        SeriesId, SeriesPage,
    },
    TradingAccess,
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
