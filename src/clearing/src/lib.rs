pub mod api;
pub mod assets;
pub mod guards;
pub mod memory;
pub mod payoffs;
pub mod trade;
pub mod traits;
pub mod types;
pub mod utils;

use candid::Principal;
use ic_cdk::export_candid;
use ic_cdk_macros::{post_upgrade, pre_upgrade};
use shared::types::{Series, SeriesId};

use crate::types::{
    http::{HttpRequest, HttpResponse},
    margin::Position,
    params::{
        BlockCollateralParams, CancelLimitOrderParams, DepositCollateralParams,
        FreezePositionForTransferParams, GetMarginAccountParams, GetPositionParams,
        SettleSeriesParams, SubmitLimitOrderParams, SubmitMarketOrderParams,
        SubmitMatchedTradeParams, UnblockCollateralParams, WithdrawCollateralParams,
    },
    results::{
        AcceptPositionTransferResult, BlockCollateralResult, DepositCollateralResult,
        GetMarginAccountResult, SettleSeriesResult, SubmitMatchedTradeResult,
        UnblockCollateralResult, WithdrawCollateralResult,
    },
    state::PositionProof,
    stats::Stats,
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
