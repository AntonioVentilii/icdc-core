extern crate candid;

use core::cell::RefCell;
use std::collections::BTreeMap;

use candid::Principal;
use ic_cdk::export_candid;
use ic_cdk_macros::{init, post_upgrade, pre_upgrade};
use shared::types::{BalanceDomain, CollateralAssetInfo, DomainPolicy, Series, SeriesId};

use crate::{
    api::{
        account::{
            params::{GetAccountStateParams, GetPositionParams},
            results::GetAccountStateResult,
        },
        admin::{
            params::{
                CancelFundWithdrawalParams, RefreshIcrcAssetMetadataParams,
                RegisterIcrcAssetParams, UpdateAssetMetricsParams, UpdateAssetPriceParams,
                UpdateCollateralAllowedDomainsParams, UpdateCollateralAssetParams,
                UpdateDomainPolicyParams, WithdrawFundParams,
            },
            results::{
                CancelFundWithdrawalResult, GetFundsResult, RefreshIcrcAssetMetadataResult,
                RegisterIcrcAssetResult, UpdateAssetPriceResult,
                UpdateCollateralAllowedDomainsResult, WithdrawFundResult,
            },
        },
        collateral::{
            params::{DepositCollateralParams, WithdrawCollateralParams},
            results::{DepositCollateralResult, WithdrawCollateralResult},
        },
        migration::{params::MigrateDomainParams, results::MigrateDomainResult},
        settlement::{
            params::{BackfillSettlementEventsParams, ListSettledSeriesParams, SettleSeriesParams},
            results::{BackfillSettlementEventsResult, SettleSeriesResult, SettledSeriesPage},
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
    memory::CONFIG,
    types::{
        event::Event,
        http::{HttpRequest, HttpResponse},
        margin::Position,
        plans::{SettlementPlan, SettlementStatusView},
        state::{Config, PositionProof},
        stats::Stats,
        trade::LimitOrder,
    },
};

pub mod account;
pub mod api;
pub mod assets;
pub mod guards;
pub mod memory;
pub mod payoffs;
pub mod trade;
pub mod traits;
pub mod types;
pub mod utils;

#[init]
fn init(config: Config) {
    CONFIG.with(|c: &RefCell<Config>| {
        *c.borrow_mut() = config;
    });
}

#[pre_upgrade]
fn pre_upgrade() {
    memory::save_state();
}

#[post_upgrade]
fn post_upgrade() {
    memory::restore_state();
}

export_candid!();
