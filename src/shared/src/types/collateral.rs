use candid::CandidType;
use serde::{Deserialize, Serialize};

use crate::types::{
    asset::Asset, decimal::DecimalValue, domain::AllowedBalanceDomains, BalanceDomain,
};

fn default_allowed_balance_domains() -> Vec<BalanceDomain> {
    AllowedBalanceDomains::default().into()
}

/// Unique identifier for a collateral asset.
///
/// This is deliberately typed as a `String` rather than an `enum` to maximise
/// flexibility and upgradeability. Using a string allows the protocol to dynamically
/// support new collateral assets via governance runtime configurations without
/// requiring downtime or a canister upgrade.
///
/// For robust frontend integrations, consumer applications should enforce safety
/// locally by defining strict string union types (e.g., in TypeScript:
/// `type SupportedAssetId = "ICP" | "ckBTC" | "vUSD";`).
pub type AssetId = String;

/// Configuration for a supported collateral asset.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CollateralAssetConfig {
    /// The canonical identifier for this asset within the clearing protocol.
    pub asset_id: AssetId,
    /// The underlying configuration required to interact with the asset's ledger.
    pub asset: Asset,
    /// The ticker symbol used for display purposes.
    pub symbol: String,
    /// The number of decimal places defined by the underlying ledger.
    pub decimals: u8,
    /// Whether the asset is currently enabled for deposits.
    pub is_enabled: bool,
    /// Identifier of the oracle responsible for updating this asset's metrics.
    pub oracle_id: Option<String>,
    /// Balance domains where this asset may be deposited or withdrawn.
    ///
    /// Defaults to both domains when deserializing legacy state.
    #[serde(default = "default_allowed_balance_domains")]
    pub allowed_balance_domains: Vec<BalanceDomain>,
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

/// Combined structure for public consumption of asset information.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CollateralAssetInfo {
    pub config: CollateralAssetConfig,
    pub metrics: Option<AssetMetrics>,
}
