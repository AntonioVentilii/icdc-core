//! One-shot, quarantined stable-state migration for the clearing canister.
//!
//! Clearing persists [`StableState`] via candid and previously traps on any
//! decode failure. Adding the compulsory `Series::resolution` field makes
//! pre-resolution blobs undecodable into the current `StableState` (candid
//! rejects a missing non-optional field). [`LegacyStableState`] mirrors the
//! historical shape — differing only in the `series` map element type
//! ([`LegacySeries`]) — so old state can be decoded and converted via
//! [`into_current`], backfilling `resolution` from each series' description.
//!
//! # Retirement
//!
//! Delete this module once every deployed clearing canister has been upgraded
//! past the release that introduced `Series::resolution`. See
//! `docs/ai/migrations.md` for the convention.

use std::collections::BTreeMap;

use candid::{CandidType, Principal};
use serde::Deserialize;
use shared::{
    migrations::{upgrade_series, LegacySeries},
    types::{
        AssetId, AssetMetrics, BalanceDomain, CollateralAssetConfig, DomainPolicy, Series, SeriesId,
    },
};

use crate::types::{
    event::Event,
    margin::{AccountState, Position},
    plans::{
        DepositPlan, FundWithdrawalPlan, MigrationKey, MigrationPlan, SettlementPlan,
        WithdrawalPlan,
    },
    state::{Config, PositionProof, StableState},
    trade::{LimitOrder, OrderId, TradeId, TransferId},
    user::{DepositKey, User, WithdrawalKey},
};

/// Pre-`resolution` mirror of [`StableState`]. Identical except that `series`
/// holds [`LegacySeries`] (the pre-`resolution` series shape).
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct LegacyStableState {
    pub config: Config,
    pub positions: Vec<Position>,
    pub accounts: BTreeMap<User, AccountState>,
    pub series: BTreeMap<SeriesId, LegacySeries>,
    pub events: Vec<Event>,
    pub next_id: u64,
    pub registry: Principal,
    pub deposit_plans: BTreeMap<DepositKey, DepositPlan>,
    pub withdrawal_plans: BTreeMap<WithdrawalKey, WithdrawalPlan>,
    pub fund_withdrawal_plans: BTreeMap<String, FundWithdrawalPlan>,
    pub executed_trades: BTreeMap<TradeId, u64>,
    pub frozen_transfers: BTreeMap<TransferId, PositionProof>,
    pub accepted_transfers: BTreeMap<TransferId, bool>,
    pub settlement_plans: BTreeMap<SeriesId, SettlementPlan>,
    pub limit_orders: BTreeMap<OrderId, LimitOrder>,
    pub insurance_fund: BTreeMap<AssetId, u128>,
    pub treasury: BTreeMap<AssetId, u128>,
    pub collateral_assets: BTreeMap<AssetId, CollateralAssetConfig>,
    pub evm_addresses: BTreeMap<Principal, String>,
    pub asset_metrics: BTreeMap<AssetId, AssetMetrics>,
    pub domain_policies: BTreeMap<BalanceDomain, DomainPolicy>,
    pub migration_plans: BTreeMap<MigrationKey, MigrationPlan>,
}

/// Converts a decoded pre-resolution [`LegacyStableState`] into the current
/// [`StableState`], backfilling `resolution` on every cached series and moving
/// all other state across verbatim.
#[must_use]
pub fn into_current(legacy: LegacyStableState) -> StableState {
    let series: BTreeMap<SeriesId, Series> = legacy
        .series
        .into_iter()
        .map(|(id, s)| (id, upgrade_series(s)))
        .collect();

    StableState {
        config: legacy.config,
        positions: legacy.positions,
        accounts: legacy.accounts,
        series,
        events: legacy.events,
        next_id: legacy.next_id,
        registry: legacy.registry,
        deposit_plans: legacy.deposit_plans,
        withdrawal_plans: legacy.withdrawal_plans,
        fund_withdrawal_plans: legacy.fund_withdrawal_plans,
        executed_trades: legacy.executed_trades,
        frozen_transfers: legacy.frozen_transfers,
        accepted_transfers: legacy.accepted_transfers,
        settlement_plans: legacy.settlement_plans,
        limit_orders: legacy.limit_orders,
        insurance_fund: legacy.insurance_fund,
        treasury: legacy.treasury,
        collateral_assets: legacy.collateral_assets,
        evm_addresses: legacy.evm_addresses,
        asset_metrics: legacy.asset_metrics,
        domain_policies: legacy.domain_policies,
        migration_plans: legacy.migration_plans,
    }
}

#[cfg(test)]
mod tests {
    use candid::{decode_one, encode_one};
    use shared::{
        migrations::NO_RESOLUTION_PLACEHOLDER,
        types::{Description, PayoffType, PayoutUnit, TradingAccess},
    };

    use super::*;
    use crate::memory::default_config;

    fn legacy_series(id: &str, description: Description) -> LegacySeries {
        LegacySeries {
            series_id: SeriesId::from(id.to_owned()),
            underlying: "ICP".to_owned(),
            expiry_ns: 1_000,
            payoff_type: PayoffType::Binary,
            strike: None,
            price_precision: 8,
            payout_unit: PayoutUnit::usd(),
            outcomes: None,
            oracle_source: "oracle".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 0,
            title: "t".to_owned(),
            description,
            icon_url: None,
            banner_url: None,
            balance_domain: BalanceDomain::Settlement,
            trading_access: vec![TradingAccess::Open],
            engine_id: None,
            forked_from: None,
            locale: None,
        }
    }

    fn legacy_state(series: BTreeMap<SeriesId, LegacySeries>) -> LegacyStableState {
        LegacyStableState {
            config: default_config(),
            positions: Vec::new(),
            accounts: BTreeMap::new(),
            series,
            events: Vec::new(),
            next_id: 42,
            registry: Principal::anonymous(),
            deposit_plans: BTreeMap::new(),
            withdrawal_plans: BTreeMap::new(),
            fund_withdrawal_plans: BTreeMap::new(),
            executed_trades: BTreeMap::new(),
            frozen_transfers: BTreeMap::new(),
            accepted_transfers: BTreeMap::new(),
            settlement_plans: BTreeMap::new(),
            limit_orders: BTreeMap::new(),
            insurance_fund: BTreeMap::new(),
            treasury: BTreeMap::new(),
            collateral_assets: BTreeMap::new(),
            evm_addresses: BTreeMap::new(),
            asset_metrics: BTreeMap::new(),
            domain_policies: BTreeMap::new(),
            migration_plans: BTreeMap::new(),
        }
    }

    /// A pre-resolution clearing blob must decode through [`LegacyStableState`]
    /// and convert into the current state with `resolution` backfilled and all
    /// other fields preserved.
    #[test]
    fn legacy_state_round_trips_and_backfills_resolution() {
        let mut series = BTreeMap::new();
        series.insert(
            SeriesId::from("with_desc".to_owned()),
            legacy_series("with_desc", Description::plain("Settles at expiry")),
        );
        series.insert(
            SeriesId::from("empty_desc".to_owned()),
            legacy_series("empty_desc", Description::plain("")),
        );

        let bytes = encode_one(legacy_state(series)).unwrap();

        // The new schema cannot decode a pre-resolution blob (compulsory field
        // missing), which is exactly why the legacy fallback exists.
        assert!(decode_one::<StableState>(&bytes).is_err());

        let decoded: LegacyStableState = decode_one(&bytes).unwrap();
        let current = into_current(decoded);

        assert_eq!(current.next_id, 42);
        assert_eq!(
            current.series[&SeriesId::from("with_desc".to_owned())]
                .resolution
                .clause,
            "Settles at expiry"
        );
        assert_eq!(
            current.series[&SeriesId::from("empty_desc".to_owned())]
                .resolution
                .clause,
            NO_RESOLUTION_PLACEHOLDER
        );
    }
}
