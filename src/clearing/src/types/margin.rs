use std::collections::BTreeMap;

use candid::{CandidType, Nat};
use serde::{Deserialize, Serialize};
use shared::{
    constants::{BPS_BASE, USD_DECIMALS},
    types::{AssetId, AssetMetrics, CollateralAssetConfig, SeriesId},
};

use crate::types::user::User;

/// Represents an open position in a derivative series for a specific user.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Position {
    /// The user who owns the position.
    pub user: User,
    /// The unique identifier of the derivative series.
    pub series_id: SeriesId,
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
    /// Real deposited collateral assets and their balances.
    pub collateral_balances: BTreeMap<AssetId, u128>,
    /// Internal realised PnL / debt / credits in the clearing accounting unit (USD).
    pub cash_balance_usd: i128,
    /// The total required margin reserved for current activity (orders + positions).
    pub reserved_margin_usd: u128,
}

impl AccountState {
    pub fn new(user: User) -> Self {
        Self {
            user,
            collateral_balances: BTreeMap::new(),
            cash_balance_usd: 0,
            reserved_margin_usd: 0,
        }
    }

    /// Retrieves the balance for a specific collateral asset.
    pub fn get_collateral_balance(&self, asset_id: &AssetId) -> u128 {
        *self.collateral_balances.get(asset_id).unwrap_or(&0)
    }

    /// Updates the balance for a specific collateral asset.
    pub fn set_collateral_balance(&mut self, asset_id: AssetId, amount: u128) {
        if amount == 0 {
            self.collateral_balances.remove(&asset_id);
        } else {
            self.collateral_balances.insert(asset_id, amount);
        }
    }

    /// Calculates the total account equity in USD using provided asset valuations.
    ///
    /// Formula (all integer):
    /// value_usd = (balance * price_value * (10000 - haircut_bps)) / (10000 * 10^(decimals +
    /// price_decimals - 6))
    pub fn calculate_equity_usd(
        &self,
        configs: &BTreeMap<AssetId, CollateralAssetConfig>,
        metrics: &BTreeMap<AssetId, AssetMetrics>,
    ) -> u128 {
        let raw = self.calculate_raw_equity_i128(configs, metrics);
        if raw < 0 {
            0
        } else {
            raw as u128
        }
    }

    /// Returns the raw (unclamped, signed) equity.
    ///
    /// Use this when you need to simulate post-settlement equity by adding a
    /// cashflow before applying the `max(0, ...)` floor.
    pub fn calculate_raw_equity_i128(
        &self,
        configs: &BTreeMap<AssetId, CollateralAssetConfig>,
        metrics: &BTreeMap<AssetId, AssetMetrics>,
    ) -> i128 {
        let mut total_equity_usd: i128 = self.cash_balance_usd;

        let target_decimals = USD_DECIMALS as u32;

        for (asset_id, balance) in &self.collateral_balances {
            if let (Some(config), Some(metric)) = (configs.get(asset_id), metrics.get(asset_id)) {
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

        total_equity_usd
    }

    /// Calculates the available equity (excess margin) in USD.
    pub fn get_available_equity_usd(
        &self,
        configs: &BTreeMap<AssetId, CollateralAssetConfig>,
        metrics: &BTreeMap<AssetId, AssetMetrics>,
    ) -> i128 {
        let equity = self.calculate_equity_usd(configs, metrics);
        (equity as i128) - (self.reserved_margin_usd as i128)
    }
}
