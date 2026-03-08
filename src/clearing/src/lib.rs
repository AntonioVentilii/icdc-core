use candid::{Nat, Principal};
use ic_cdk_macros::{post_upgrade, pre_upgrade};
use shared::types::{Series, SeriesId};

use crate::{
    api::{
        account::{
            params::{GetMarginAccountParams, GetPositionParams},
            results::GetMarginAccountResult,
        },
        admin::{
            params::WithdrawFundParams,
            results::{AdminResult, GetFundsResult},
        },
        collateral::{
            params::{
                BlockCollateralParams, DepositCollateralParams, UnblockCollateralParams,
                WithdrawCollateralParams,
            },
            results::{
                BlockCollateralResult, DepositCollateralResult, UnblockCollateralResult,
                WithdrawCollateralResult,
            },
        },
        settlement::{params::SettleSeriesParams, results::SettleSeriesResult},
        trade::{
            params::{
                CancelLimitOrderParams, FreezePositionForTransferParams, ListOrdersParams,
                SubmitLimitOrderParams, SubmitMarketOrderParams, SubmitMatchedTradeParams,
            },
            results::{AcceptPositionTransferResult, SubmitMatchedTradeResult},
        },
    },
    types::{
        event::Event,
        http::{HttpRequest, HttpResponse},
        margin::Position,
        state::{Config, PositionProof},
        stats::Stats,
        trade::LimitOrder,
    },
};

pub mod api;
pub mod assets;
pub mod guards;
pub mod memory;
pub mod payoffs;
pub mod trade;
pub mod traits;
pub mod types;
pub mod utils;

#[pre_upgrade]
fn pre_upgrade() {
    memory::save_state();
}

#[post_upgrade]
fn post_upgrade() {
    memory::restore_state();
}

ic_cdk::export_candid!();
