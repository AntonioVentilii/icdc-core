use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::{SeriesId, SettlementInput};

/// Input parameters for initiating a series settlement.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettleSeriesParams {
    /// The derivative series identifier.
    pub series_id: SeriesId,
    /// The final settlement data from the oracle.
    pub settlement: SettlementInput,
}
