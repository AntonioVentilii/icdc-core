use core::cell::RefCell;
use std::collections::BTreeMap;

use candid::Principal;
use ic_cdk::{storage, trap};
use shared::types::{
    Asset, AssetId, AssetMetrics, BalanceDomain, CollateralAssetConfig, DomainPolicy, Series,
    SeriesId,
};

use crate::{
    migrations::{into_current, LegacyStableState},
    types::{
        event::{Event, EventType, SeriesTradePoint, SeriesVolumeAggregate},
        leaderboard::{LeaderboardWindow, PnlAggregate},
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

pub(crate) fn default_config() -> Config {
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
    /// Per-series price-history index: one [`SeriesTradePoint`] per executed
    /// trade, in execution order (ascending `event_id`). Derived entirely from
    /// the executed rows in `EVENTS`, so it is **not** persisted — it is rebuilt
    /// from `EVENTS` in `post_upgrade` via [`rebuild_series_trade_history`] and
    /// maintained incrementally on each trade. Lets `list_series_trade_history`
    /// serve a market's price history in O(log series + page) instead of
    /// scanning the whole event log per call.
    pub static SERIES_TRADE_HISTORY: RefCell<BTreeMap<SeriesId, Vec<SeriesTradePoint>>> = const { RefCell::new(BTreeMap::new()) };
    /// Per-series cumulative traded-volume totals (notional + trade count),
    /// maintained alongside [`SERIES_TRADE_HISTORY`] so `list_series_traded_volumes`
    /// reads `O(#series_ids)` instead of summing each series' tape per call. Same
    /// lifecycle as the trade index: a non-persisted projection of the executed
    /// rows in `EVENTS`, rebuilt in `post_upgrade` via
    /// [`rebuild_series_trade_history`] and folded incrementally on each trade.
    pub static SERIES_TRADED_VOLUME: RefCell<BTreeMap<SeriesId, SeriesVolumeAggregate>> = const { RefCell::new(BTreeMap::new()) };
    /// Per-window leaderboard index: realized settlement `PnL` aggregated per
    /// principal, keyed by `(window, period_id)` (see [`LeaderboardWindow`]).
    /// Each settled position folds its signed `cashflow_usd` into the current
    /// week, month, and all-time buckets. Like [`SERIES_TRADE_HISTORY`] this is
    /// a pure projection of the `Settled` rows in `EVENTS`, so it is **not**
    /// persisted — it is rebuilt from `EVENTS` in `post_upgrade` via
    /// [`rebuild_leaderboard`] and maintained incrementally as settlements emit
    /// events. Lets `list_leaderboard` rank a window without scanning the whole
    /// event log per call.
    pub static SETTLEMENT_LEADERBOARD: RefCell<BTreeMap<(LeaderboardWindow, u64), BTreeMap<User, PnlAggregate>>> = const { RefCell::new(BTreeMap::new()) };
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
    // Try the current schema first. On failure, fall back to the pre-resolution
    // schema and backfill `Series::resolution` (see `crate::migrations`).
    let state = match storage::stable_restore::<(StableState,)>() {
        Ok((s,)) => s,
        Err(current_err) => match storage::stable_restore::<(LegacyStableState,)>() {
            Ok((legacy,)) => into_current(legacy),
            // Surface BOTH decode errors: which schema was expected to win
            // depends on the deployed version, so keeping both makes a failed
            // post_upgrade decode debuggable.
            Err(legacy_err) => trap(format!(
                "Failed to restore stable state: current-schema decode failed \
                 ({current_err:?}); legacy-schema decode failed ({legacy_err:?})"
            )),
        },
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
    rebuild_series_trade_history();
    rebuild_leaderboard();
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

/// Records an executed trade in the per-series price-history index
/// ([`SERIES_TRADE_HISTORY`]). Called once per trade from the execution path.
///
/// Trades execute in monotonically increasing `event_id` order, so appending
/// keeps each series' vector sorted without a re-sort.
pub(crate) fn index_executed_trade(series_id: &SeriesId, point: SeriesTradePoint) {
    // Fold this trade's notional into the per-series aggregate before the point
    // is moved into the history vector, so the volume totals stay in lockstep
    // with the index they accompany.
    let notional = point.notional_usd_base();

    SERIES_TRADED_VOLUME.with(|agg| {
        agg.borrow_mut()
            .entry(series_id.clone())
            .or_default()
            .record(notional);
    });

    SERIES_TRADE_HISTORY.with(|idx| {
        idx.borrow_mut()
            .entry(series_id.clone())
            .or_default()
            .push(point);
    });
}

/// Rebuilds [`SERIES_TRADE_HISTORY`] from the executed rows in `EVENTS`.
///
/// The index is a pure projection of `EVENTS`, so it is not part of the
/// persisted [`StableState`]; `restore_state` reconstructs it after `EVENTS` is
/// loaded. Each executed trade has two rows sharing one `event_id` (buyer with
/// `qty > 0`, seller with `qty < 0`); selecting the `qty > 0` row collapses the
/// pair to a single point per trade. Vectors are sorted by `event_id` to stay
/// robust to any future change in `EVENTS` ordering.
pub(crate) fn rebuild_series_trade_history() {
    SERIES_TRADE_HISTORY.with(|idx| {
        SERIES_TRADED_VOLUME.with(|agg| {
            let mut idx = idx.borrow_mut();
            let mut agg = agg.borrow_mut();
            idx.clear();
            agg.clear();

            EVENTS.with(|events| {
                for e in events.borrow().iter() {
                    if matches!(e.event_type, EventType::Executed) && e.qty > 0 {
                        let point = SeriesTradePoint {
                            event_id: e.event_id,
                            price: e.price.clone(),
                            qty: e.qty,
                            timestamp: e.timestamp,
                        };

                        // The notional sum is order-independent, so folding here
                        // (before the per-series sort below) is fine.
                        agg.entry(e.series_id.clone())
                            .or_default()
                            .record(point.notional_usd_base());

                        idx.entry(e.series_id.clone()).or_default().push(point);
                    }
                }
            });

            for points in idx.values_mut() {
                points.sort_by_key(|p| p.event_id);
            }
        });
    });
}

/// Folds a single settled position into the leaderboard index, adding its
/// signed `cashflow_usd` to the user's current week, month, and all-time
/// aggregates. The period is derived from `timestamp`, so the buckets a
/// settlement lands in are fixed at settlement time and independent of when the
/// leaderboard is later queried.
fn record_settled_pnl(
    idx: &mut BTreeMap<(LeaderboardWindow, u64), BTreeMap<User, PnlAggregate>>,
    user: User,
    cashflow_usd: i128,
    timestamp: u64,
) {
    for window in LeaderboardWindow::ALL {
        let period = window.period_id(timestamp);
        idx.entry((window, period))
            .or_default()
            .entry(user)
            .or_default()
            .record(cashflow_usd);
    }
}

/// Indexes the `Settled` rows in a freshly emitted batch of events into
/// [`SETTLEMENT_LEADERBOARD`]. Called at each site that appends settlement
/// events to `EVENTS` (live settlement and the one-shot backfill), so the
/// leaderboard stays current between upgrades. Non-`Settled` events are
/// ignored. A `Settled` event's `qty` is the position's signed `cashflow_usd`.
pub(crate) fn index_settled_events(events: &[Event]) {
    SETTLEMENT_LEADERBOARD.with(|idx| {
        let mut idx = idx.borrow_mut();
        for e in events {
            if matches!(e.event_type, EventType::Settled) {
                record_settled_pnl(&mut idx, e.user, e.qty, e.timestamp);
            }
        }
    });
}

/// Rebuilds [`SETTLEMENT_LEADERBOARD`] from the `Settled` rows in `EVENTS`.
///
/// The index is a pure projection of `EVENTS`, so it is not part of the
/// persisted [`StableState`]; `restore_state` reconstructs it after `EVENTS` is
/// loaded. Period bucketing is a deterministic function of each event's
/// `timestamp`, so the rebuild reproduces exactly the buckets the incremental
/// maintenance produced.
pub(crate) fn rebuild_leaderboard() {
    SETTLEMENT_LEADERBOARD.with(|idx| {
        let mut idx = idx.borrow_mut();
        idx.clear();

        EVENTS.with(|events| {
            for e in events.borrow().iter() {
                if matches!(e.event_type, EventType::Settled) {
                    record_settled_pnl(&mut idx, e.user, e.qty, e.timestamp);
                }
            }
        });
    });
}
