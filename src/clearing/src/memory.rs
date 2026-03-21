use core::cell::RefCell;
use std::collections::BTreeMap;

use candid::Principal;
use ic_cdk::{storage, trap};
use shared::types::{
    Asset, AssetId, AssetMetrics, BalanceDomain, CollateralAssetConfig, DomainPolicy, Series,
    SeriesId,
};

use crate::{
    types::{
        event::Event,
        margin::{AccountState, Position, PositionsMap},
        plans::{
            DepositPlan, FundWithdrawalPlan, MigrationKey, MigrationPlan, SettlementPlan,
            WithdrawalPlan,
        },
        state::{Config, StableState},
        trade::{LimitOrder, OrderId, TradeId, TransferId},
        user::{DepositKey, User, WithdrawalKey},
    },
    PositionProof,
};

fn default_config() -> Config {
    Config {
        insurance_fund_fee_ratio: 10,
        protocol_fee_ratio: 5,
        evm_rpc: Principal::anonymous(),
        signer_canister: Principal::anonymous(),
        internal_ledger: CollateralAssetConfig {
            asset_id: String::new(),
            asset: Asset::Icrc(Principal::anonymous()),
            symbol: String::new(),
            decimals: 0,
            is_enabled: false,
            oracle_id: None,
            allowed_balance_domains: vec![BalanceDomain::Settlement, BalanceDomain::Playground],
        },
        version: 0,
    }
}

thread_local! {
    pub static CONFIG: RefCell<Config> = RefCell::new(default_config());
    pub static POSITIONS: RefCell<PositionsMap> = const { RefCell::new(BTreeMap::new()) };
    pub static ACCOUNT_STATES: RefCell<BTreeMap<User, AccountState>> = const { RefCell::new(BTreeMap::new()) };
    pub static SERIES: RefCell<BTreeMap<SeriesId, Series>> = const { RefCell::new(BTreeMap::new()) };
    pub static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    pub static NEXT_EVENT_ID: RefCell<u64> = const { RefCell::new(0) };
    pub static REGISTRY_CANISTER: RefCell<Principal> = const { RefCell::new(Principal::anonymous()) };
    pub static DEPOSIT_PLANS: RefCell<BTreeMap<DepositKey, DepositPlan>> = const { RefCell::new(BTreeMap::new()) };
    pub static WITHDRAWAL_PLANS: RefCell<BTreeMap<WithdrawalKey, WithdrawalPlan>> = const { RefCell::new(BTreeMap::new()) };
    pub static FUND_WITHDRAWAL_PLANS: RefCell<BTreeMap<String, FundWithdrawalPlan>> = const { RefCell::new(BTreeMap::new()) };
    pub static EXECUTED_TRADES: RefCell<BTreeMap<TradeId, u64>> = const { RefCell::new(BTreeMap::new()) };
    pub static LIMIT_ORDERS: RefCell<BTreeMap<OrderId, LimitOrder>> = const { RefCell::new(BTreeMap::new()) };
    pub static FROZEN_TRANSFERS: RefCell<BTreeMap<TransferId, PositionProof>> = const { RefCell::new(BTreeMap::new()) };
    pub static ACCEPTED_TRANSFERS: RefCell<BTreeMap<TransferId, bool>> = const { RefCell::new(BTreeMap::new()) };
    pub static SETTLEMENT_PLANS: RefCell<BTreeMap<SeriesId, SettlementPlan>> = const { RefCell::new(BTreeMap::new()) };
    pub static INSURANCE_FUND: RefCell<BTreeMap<AssetId, u128>> = const { RefCell::new(BTreeMap::new()) };
    pub static TREASURY: RefCell<BTreeMap<AssetId, u128>> = const { RefCell::new(BTreeMap::new()) };
    pub static COLLATERAL_ASSETS: RefCell<BTreeMap<AssetId, CollateralAssetConfig>> = const { RefCell::new(BTreeMap::new()) };
    pub static EVM_ADDRESSES: RefCell<BTreeMap<Principal, String>> = const { RefCell::new(BTreeMap::new()) };
    pub static ASSET_METRICS: RefCell<BTreeMap<AssetId, AssetMetrics>> = const { RefCell::new(BTreeMap::new()) };
    pub static DOMAIN_POLICIES: RefCell<BTreeMap<BalanceDomain, DomainPolicy>> = const { RefCell::new(BTreeMap::new()) };
    pub static MIGRATION_PLANS: RefCell<BTreeMap<MigrationKey, MigrationPlan>> = const { RefCell::new(BTreeMap::new()) };
}

pub fn save_state() {
    let config: Config = CONFIG.with(|c| c.borrow().clone());
    let positions: Vec<Position> = POSITIONS.with(|p| p.borrow().values().cloned().collect());
    let accounts: BTreeMap<User, AccountState> = ACCOUNT_STATES.with(|a| a.borrow().clone());
    let series: BTreeMap<SeriesId, Series> = SERIES.with(|s| s.borrow().clone());
    let events: Vec<Event> = EVENTS.with(|e| e.borrow().clone());
    let next_id: u64 = NEXT_EVENT_ID.with(|id| *id.borrow());
    let registry: Principal = REGISTRY_CANISTER.with(|r| *r.borrow());
    let deposit_plans: BTreeMap<DepositKey, DepositPlan> =
        DEPOSIT_PLANS.with(|d| d.borrow().clone());
    let withdrawal_plans: BTreeMap<WithdrawalKey, WithdrawalPlan> =
        WITHDRAWAL_PLANS.with(|w| w.borrow().clone());
    let fund_withdrawal_plans: BTreeMap<String, FundWithdrawalPlan> =
        FUND_WITHDRAWAL_PLANS.with(|f| f.borrow().clone());
    let executed_trades: BTreeMap<TradeId, u64> = EXECUTED_TRADES.with(|t| t.borrow().clone());
    let frozen_transfers: BTreeMap<TransferId, PositionProof> =
        FROZEN_TRANSFERS.with(|t| t.borrow().clone());
    let accepted_transfers: BTreeMap<TransferId, bool> =
        ACCEPTED_TRANSFERS.with(|t| t.borrow().clone());
    let settlement_plans: BTreeMap<SeriesId, SettlementPlan> =
        SETTLEMENT_PLANS.with(|s| s.borrow().clone());
    let limit_orders: BTreeMap<OrderId, LimitOrder> = LIMIT_ORDERS.with(|l| l.borrow().clone());
    let insurance_fund: BTreeMap<AssetId, u128> = INSURANCE_FUND.with(|f| f.borrow().clone());
    let treasury: BTreeMap<AssetId, u128> = TREASURY.with(|f| f.borrow().clone());
    let collateral_assets: BTreeMap<AssetId, CollateralAssetConfig> =
        COLLATERAL_ASSETS.with(|f| f.borrow().clone());
    let evm_addresses: BTreeMap<Principal, String> = EVM_ADDRESSES.with(|f| f.borrow().clone());
    let asset_metrics: BTreeMap<AssetId, AssetMetrics> = ASSET_METRICS.with(|f| f.borrow().clone());
    let domain_policies: BTreeMap<BalanceDomain, DomainPolicy> =
        DOMAIN_POLICIES.with(|f| f.borrow().clone());
    let migration_plans: BTreeMap<MigrationKey, MigrationPlan> =
        MIGRATION_PLANS.with(|f| f.borrow().clone());
    let state = StableState {
        config,
        positions,
        accounts,
        series,
        events,
        next_id,
        registry,
        deposit_plans,
        withdrawal_plans,
        fund_withdrawal_plans,
        executed_trades,
        frozen_transfers,
        accepted_transfers,
        settlement_plans,
        limit_orders,
        insurance_fund,
        treasury,
        collateral_assets,
        evm_addresses,
        asset_metrics,
        domain_policies,
        migration_plans,
    };

    storage::stable_save((state,)).expect("Save failed");
}

pub fn restore_state() {
    let result: Result<(StableState,), String> = storage::stable_restore();

    let state = match result {
        Ok((s,)) => s,
        Err(e) => {
            trap(format!("Failed to restore stable state: {e:?}"));
        }
    };

    let StableState {
        config,
        positions,
        accounts,
        series,
        events,
        next_id,
        registry,
        deposit_plans,
        withdrawal_plans,
        fund_withdrawal_plans,
        executed_trades,
        frozen_transfers,
        accepted_transfers,
        settlement_plans,
        limit_orders,
        insurance_fund,
        treasury,
        collateral_assets,
        evm_addresses,
        asset_metrics,
        domain_policies,
        migration_plans,
    } = state;

    POSITIONS.with(|p: &RefCell<PositionsMap>| {
        let mut p = p.borrow_mut();
        for pos in positions {
            p.insert(
                (pos.user, pos.series_id.clone(), pos.outcome_id.clone()),
                pos,
            );
        }
    });

    CONFIG.with(|c| *c.borrow_mut() = config);
    ACCOUNT_STATES.with(|a| *a.borrow_mut() = accounts);
    SERIES.with(|s| *s.borrow_mut() = series);
    EVENTS.with(|e| *e.borrow_mut() = events);
    NEXT_EVENT_ID.with(|id| *id.borrow_mut() = next_id);
    REGISTRY_CANISTER.with(|r| *r.borrow_mut() = registry);
    DEPOSIT_PLANS.with(|d| *d.borrow_mut() = deposit_plans);
    WITHDRAWAL_PLANS.with(|w| *w.borrow_mut() = withdrawal_plans);
    FUND_WITHDRAWAL_PLANS.with(|f| *f.borrow_mut() = fund_withdrawal_plans);
    EXECUTED_TRADES.with(|t| *t.borrow_mut() = executed_trades);
    FROZEN_TRANSFERS.with(|t| *t.borrow_mut() = frozen_transfers);
    ACCEPTED_TRANSFERS.with(|t| *t.borrow_mut() = accepted_transfers);
    SETTLEMENT_PLANS.with(|s| *s.borrow_mut() = settlement_plans);
    LIMIT_ORDERS.with(|l| *l.borrow_mut() = limit_orders);
    INSURANCE_FUND.with(|f| *f.borrow_mut() = insurance_fund);
    TREASURY.with(|f| *f.borrow_mut() = treasury);
    COLLATERAL_ASSETS.with(|f| *f.borrow_mut() = collateral_assets);
    EVM_ADDRESSES.with(|f| *f.borrow_mut() = evm_addresses);
    ASSET_METRICS.with(|f| *f.borrow_mut() = asset_metrics);
    DOMAIN_POLICIES.with(|f| *f.borrow_mut() = domain_policies);
    MIGRATION_PLANS.with(|f| *f.borrow_mut() = migration_plans);
}
