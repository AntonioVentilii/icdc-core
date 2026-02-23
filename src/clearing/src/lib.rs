pub mod account;
pub mod api;
pub mod error;
pub mod guards;
pub mod memory;
pub mod params;
pub mod results;
pub mod series;
mod traits;
pub mod types;

use candid::Principal;
use ic_cdk::export_candid;
use ic_cdk_macros::{post_upgrade, pre_upgrade};
use shared::types::{Series, SeriesId};
use types::PositionProof;

use crate::{
    error::ClearingError,
    params::{
        DepositCollateralParams, FreezePositionForTransferParams, GetPositionParams,
        SettleSeriesParams, SubmitMatchedTradeParams, WithdrawCollateralParams,
    },
    results::{
        AcceptPositionTransferResult, DepositCollateralResult, SettleSeriesResult,
        SubmitMatchedTradeResult, WithdrawCollateralResult,
    },
    types::{MarginAccount, Position},
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
