use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::{PayoffType, SettlementAsset};

/// Input parameters for registering a new derivative series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AddSeriesParams {
    /// The underlying asset ticker (case-insensitive, e.g., "ICP").
    pub underlying: String,
    /// Expiry timestamp in nanoseconds since UNIX epoch.
    pub expiry: u64,
    /// The payoff model for the series.
    pub payoff_type: PayoffType,
    /// The option strike price, if applicable.
    pub strike: Option<u64>,
    /// The asset in which the contract is settled.
    pub settlement_asset: SettlementAsset,
    /// The price oracle identifier (case-insensitive, e.g., "Coingecko").
    pub oracle_source: String,
    /// A short, descriptive title for the series.
    pub title: String,
    /// A detailed description of the series.
    pub description: String,
}
