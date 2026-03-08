use candid::CandidType;
use serde::{Deserialize, Serialize};

use crate::types::{asset::Asset, decimal::DecimalValue};

/// Unique identifier for a collateral asset.
pub type AssetId = String;

/// Configuration for a supported collateral asset.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CollateralAssetConfig {
    pub asset_id: AssetId,
    pub asset: Asset,
    pub symbol: String,
    pub decimals: u8,
    /// Static price in USD (canonical accounting unit).
    pub price_usd: DecimalValue,
    /// Static haircut in basis points (e.g., 1000 = 10% haircut, 90% value).
    pub haircut_bps: u16,
    pub is_enabled: bool,
}

impl CollateralAssetConfig {
    /// Returns the effective value multiplier (1.0 - haircut).
    pub fn valuation_factor(&self) -> f64 {
        if self.haircut_bps >= 10000 {
            0.0
        } else {
            (10000 - self.haircut_bps) as f64 / 10000.0
        }
    }
}
