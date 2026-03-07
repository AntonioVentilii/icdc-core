use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::{Price, SeriesId};

/// Input parameters for initiating a series settlement.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettleSeriesParams {
    /// The derivative series identifier.
    pub series_id: SeriesId,
    /// The final settlement price from the oracle.
    pub settlement_price: Price,
}
