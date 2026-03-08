use candid::{Nat, Principal};
use ic_cdk_macros::{post_upgrade, pre_upgrade};
use shared::types::{Asset, AssetId, CollateralAssetConfig, PayoutUnit, Price, Series, SeriesId};

use crate::{
    api::{
        account::{
            params::{GetAccountStateParams, GetPositionParams},
            results::GetAccountStateResult,
        },
        admin::{
            params::{FundType, UpdateCollateralAssetParams, WithdrawFundParams},
            results::{AdminError, AdminResult, GetFundsResult},
        },
        collateral::{
            params::{DepositCollateralParams, WithdrawCollateralParams},
            results::{DepositCollateralResult, WithdrawCollateralResult},
        },
        settlement::{
            errors::SettlementError, params::SettleSeriesParams, results::SettleSeriesResult,
        },
        trade::{
            errors::TradeError,
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
        state::{ClearingConfig, PositionProof},
        stats::Stats,
        trade::LimitOrder,
    },
};

pub mod api;
pub use api::{
    account::api::*, admin::api::*, collateral::api::*, general::api::*, metrics::api::*,
    settlement::api::*, trade::api::*,
};

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
