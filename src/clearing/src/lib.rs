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
            params::{AggregateLeanParams, GetAccountStateParams, GetPositionParams},
            results::{AggregateLean, GetAccountStateResult},
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
        leaderboard::{params::ListLeaderboardParams, results::LeaderboardPage},
        migration::{params::MigrateDomainParams, results::MigrateDomainResult},
        settlement::{
            params::{BackfillSettlementEventsParams, ListSettledSeriesParams, SettleSeriesParams},
            results::{BackfillSettlementEventsResult, SettleSeriesResult, SettledSeriesPage},
        },
        trade::{
            errors::TradeError,
            params::{
                CancelLimitOrderParams, FreezePositionForTransferParams,
                GetSeriesPriceHistoryParams, ListOrdersParams, ListSeriesTradeHistoryParams,
                ListSeriesTradedVolumesParams, SubmitLimitOrderParams, SubmitMarketOrderParams,
                SubmitMatchedTradeParams,
            },
            results::{
                AcceptPositionTransferResult, SeriesPriceHistory, SeriesTradeHistoryPage,
                SeriesTradedVolume, SubmitMatchedTradeResult,
            },
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
pub mod migrations;
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
    api::settlement::api::start_settlement_heartbeat();
}

#[pre_upgrade]
fn pre_upgrade() {
    memory::save_state();
}

#[post_upgrade]
fn post_upgrade() {
    memory::restore_state();
    // IC timers do not survive an upgrade, so any settlement that was still in
    // flight would otherwise stall until a manual `resume_settlement`. Re-drive
    // them here so they catch up on their own.
    api::settlement::api::resume_pending_settlements();
    // Interval timers also do not survive an upgrade, so re-arm the heartbeat
    // that recovers any settlement whose self-resumption chain later breaks.
    api::settlement::api::start_settlement_heartbeat();
}

export_candid!();
