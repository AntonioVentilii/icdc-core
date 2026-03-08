use std::collections::BTreeMap;

use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::{AssetId, CollateralAssetConfig, SeriesId};

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
    pub fn calculate_equity_usd(&self, configs: &BTreeMap<AssetId, CollateralAssetConfig>) -> u128 {
        let mut total_equity_usd: i128 = self.cash_balance_usd;

        for (asset_id, balance) in &self.collateral_balances {
            if let Some(config) = configs.get(asset_id) {
                if config.is_enabled {
                    let value = (*balance as f64
                        * config.price_usd.to_f64()
                        * config.valuation_factor()) as i128;
                    total_equity_usd += value;
                }
            }
        }

        if total_equity_usd < 0 {
            0
        } else {
            total_equity_usd as u128
        }
    }

    /// Calculates the available equity (excess margin) in USD.
    pub fn get_available_equity_usd(
        &self,
        configs: &BTreeMap<AssetId, CollateralAssetConfig>,
    ) -> i128 {
        let equity = self.calculate_equity_usd(configs);
        (equity as i128) - (self.reserved_margin_usd as i128)
    }
}
