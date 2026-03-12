use std::collections::BTreeMap;

use candid::{CandidType, Nat};
use serde::{Deserialize, Serialize};
use shared::{
    constants::{BPS_BASE, USD_DECIMALS},
    types::{AssetId, AssetMetrics, BalanceDomain, CollateralAssetConfig, OutcomeId, SeriesId},
};

use crate::types::user::User;

/// A composite key used to look up a position in a map.
pub type PositionKey = (User, SeriesId, Option<OutcomeId>);

/// A type alias for the map containing all user positions.
pub type PositionsMap = BTreeMap<PositionKey, Position>;

/// Represents an open position in a derivative series for a specific user.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Position {
    /// The user who owns the position.
    pub user: User,
    /// The unique identifier of the derivative series.
    pub series_id: SeriesId,
    /// The specific outcome for categorical markets.
    pub outcome_id: Option<OutcomeId>,
    /// The net quantity of the position (positive for Long, negative for Short).
    pub net_qty: i128,
    /// The amount of margin reserved for this specific position in USD.
    pub reserved_margin_usd: u128,
}

/// Represents a user's account state in the clearing system.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AccountState {
    /// The owner of the account.
    pub user: User,
    /// Real deposited collateral assets and their balances, scoped by domain.
    pub balances: BTreeMap<BalanceDomain, BTreeMap<AssetId, u128>>,
    /// Internal realised PnL / debt / credits, scoped by domain.
    pub cash_balances_usd: BTreeMap<BalanceDomain, i128>,
    /// The total required margin reserved for current activity, scoped by domain.
    pub reserved_margins_usd: BTreeMap<BalanceDomain, u128>,
}

impl AccountState {
    pub fn new(user: User) -> Self {
        Self {
            user,
            balances: BTreeMap::new(),
            cash_balances_usd: BTreeMap::new(),
            reserved_margins_usd: BTreeMap::new(),
        }
    }

    /// Retrieves the balance for a specific asset in a given domain.
    pub fn get_balance(&self, domain: BalanceDomain, asset_id: &AssetId) -> u128 {
        self.balances
            .get(&domain)
            .and_then(|m| m.get(asset_id))
            .copied()
            .unwrap_or(0)
    }

    /// Updates the balance for a specific asset in a given domain.
    pub fn set_balance(&mut self, domain: BalanceDomain, asset_id: AssetId, amount: u128) {
        if amount == 0 {
            if let Some(m) = self.balances.get_mut(&domain) {
                m.remove(&asset_id);
                if m.is_empty() {
                    self.balances.remove(&domain);
                }
            }
        } else {
            self.balances
                .entry(domain)
                .or_default()
                .insert(asset_id, amount);
        }
    }

    pub fn get_cash_balance_usd(&self, domain: BalanceDomain) -> i128 {
        *self.cash_balances_usd.get(&domain).unwrap_or(&0)
    }

    pub fn set_cash_balance_usd(&mut self, domain: BalanceDomain, amount: i128) {
        if amount == 0 {
            self.cash_balances_usd.remove(&domain);
        } else {
            self.cash_balances_usd.insert(domain, amount);
        }
    }

    pub fn get_reserved_margin_usd(&self, domain: BalanceDomain) -> u128 {
        *self.reserved_margins_usd.get(&domain).unwrap_or(&0)
    }

    pub fn set_reserved_margin_usd(&mut self, domain: BalanceDomain, amount: u128) {
        if amount == 0 {
            self.reserved_margins_usd.remove(&domain);
        } else {
            self.reserved_margins_usd.insert(domain, amount);
        }
    }

    /// Calculates the total account equity in USD using provided asset valuations for a specific
    /// domain.
    ///
    /// Formula (all integer):
    /// value_usd = (balance * price_value * (10000 - haircut_bps)) / (10000 * 10^(decimals +
    /// price_decimals - 6))
    pub fn calculate_equity_usd(
        &self,
        domain: BalanceDomain,
        configs: &BTreeMap<AssetId, CollateralAssetConfig>,
        metrics: &BTreeMap<AssetId, AssetMetrics>,
    ) -> u128 {
        let raw = self.calculate_raw_equity_i128(domain, configs, metrics);
        if raw < 0 {
            0
        } else {
            raw as u128
        }
    }

    /// Returns the raw (unclamped, signed) equity for a specific domain.
    ///
    /// Use this when you need to simulate post-settlement equity by adding a
    /// cashflow before applying the `max(0, ...)` floor.
    pub fn calculate_raw_equity_i128(
        &self,
        domain: BalanceDomain,
        configs: &BTreeMap<AssetId, CollateralAssetConfig>,
        metrics: &BTreeMap<AssetId, AssetMetrics>,
    ) -> i128 {
        let mut total_equity_usd: i128 = self.get_cash_balance_usd(domain);

        let target_decimals = USD_DECIMALS as u32;

        if let Some(domain_balances) = self.balances.get(&domain) {
            for (asset_id, balance) in domain_balances {
                if let (Some(config), Some(metric)) = (configs.get(asset_id), metrics.get(asset_id))
                {
                    if config.is_enabled {
                        let price_value = metric.price_usd.value;
                        let price_decimals = metric.price_usd.decimals as u32;
                        let asset_decimals = config.decimals as u32;

                        let haircut_multiplier =
                            (BPS_BASE as u128).saturating_sub(metric.haircut_bps as u128);

                        let numerator = Nat::from(*balance)
                            * Nat::from(price_value)
                            * Nat::from(haircut_multiplier);

                        let total_source_decimals = asset_decimals + price_decimals;

                        let value_usd_nat = if total_source_decimals >= target_decimals {
                            let divisor = Nat::from(BPS_BASE)
                                * Nat::from(10u128.pow(total_source_decimals - target_decimals));
                            numerator / divisor
                        } else {
                            let multiplier =
                                Nat::from(10u128.pow(target_decimals - total_source_decimals));
                            (numerator * multiplier) / Nat::from(BPS_BASE)
                        };

                        let value_usd: u128 = value_usd_nat.0.try_into().unwrap_or(u128::MAX);
                        total_equity_usd += value_usd as i128;
                    }
                }
            }
        }

        total_equity_usd
    }

    /// Calculates the available equity (excess margin) in USD for a specific domain.
    pub fn get_available_equity_usd(
        &self,
        domain: BalanceDomain,
        configs: &BTreeMap<AssetId, CollateralAssetConfig>,
        metrics: &BTreeMap<AssetId, AssetMetrics>,
    ) -> i128 {
        let equity = self.calculate_equity_usd(domain, configs, metrics);
        (equity as i128) - (self.get_reserved_margin_usd(domain) as i128)
    }
}
