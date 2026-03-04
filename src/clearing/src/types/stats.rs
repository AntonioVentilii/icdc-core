use std::collections::BTreeMap;

use candid::{CandidType, Nat};
use serde::{Deserialize, Serialize};
use shared::types::Asset;

/// Represents structured statistics for the clearing system.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Stats {
    /// Total absolute open interest across all series.
    pub open_interest: Nat,
    /// Total collateral locked across all positions.
    pub total_collateral_locked: Nat,
    /// Total number of unique margin accounts.
    pub total_users: u64,
    /// Total number of derivative series.
    pub total_series: u64,
    /// Total number of executed trades.
    pub total_trades: u64,
    /// Total collateral balance in margin accounts per asset.
    pub margin_balances: BTreeMap<Asset, Nat>,
    /// Total number of events per type.
    pub event_counts: BTreeMap<String, u64>,
}
