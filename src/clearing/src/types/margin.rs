use std::collections::BTreeMap;

use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::{Asset, SeriesId};

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
    /// The amount of collateral locked for this position.
    pub locked_collateral: u128,
}

/// Represents a user's margin account, tracking balances across different assets.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct MarginAccount {
    /// The owner of the margin account.
    pub user: User,
    /// A map of assets to their respective balances.
    pub balances: BTreeMap<Asset, u128>,
    /// The total required margin to MAINTAIN current positions.
    pub required_margin: u128,
}
impl MarginAccount {
    /// Retrieves the balance for a specific [`Asset`].
    pub fn get_balance(&self, asset: &Asset) -> u128 {
        *self.balances.get(asset).unwrap_or(&0)
    }

    /// Updates the balance for a specific [`Asset`].
    pub fn set_balance(&mut self, asset: Asset, amount: u128) {
        self.balances.insert(asset, amount);
    }

    /// Returns a list of all assets currently tracked in the account.
    pub fn tracked_assets(&self) -> Vec<Asset> {
        self.balances.keys().cloned().collect()
    }
}
