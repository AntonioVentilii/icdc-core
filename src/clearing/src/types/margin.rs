use std::collections::BTreeMap;

use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::{Asset, SeriesId};

use crate::types::user::User;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Position {
    pub user: User,
    pub series_id: SeriesId,
    pub net_qty: i128,
    pub locked_collateral: u128,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct MarginAccount {
    pub user: User,
    pub balances: BTreeMap<Asset, u128>, // (Asset, Balance)
    pub required_margin: u128,
}
impl MarginAccount {
    pub fn get_balance(&self, asset: &Asset) -> u128 {
        *self.balances.get(asset).unwrap_or(&0)
    }

    pub fn set_balance(&mut self, asset: Asset, amount: u128) {
        self.balances.insert(asset, amount);
    }

    pub fn tracked_assets(&self) -> Vec<Asset> {
        self.balances.keys().cloned().collect()
    }
}
