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
    pub is_enabled: bool,
    /// Identifier of the oracle responsible for updating this asset's metrics.
    pub oracle_id: Option<String>,
}

/// Dynamic metrics and risk parameters for a collateral asset.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AssetMetrics {
    /// Oracle-updated price in USD (canonical accounting unit).
    pub price_usd: DecimalValue,
    /// Haircut in basis points (e.g., 1000 = 10% haircut, 90% value).
    pub haircut_bps: u16,
    /// Latest known transfer fee for the asset.
    pub latest_transfer_fee: Option<u128>,
    /// Optional asset-specific insurance fund fee ratio in basis points.
    pub insurance_fee_ratio: Option<u16>,
    /// Optional asset-specific protocol fee ratio in basis points.
    pub protocol_fee_ratio: Option<u16>,
    /// Last time the asset metrics were updated (in nanoseconds).
    pub last_updated_ns: Option<u64>,
}
