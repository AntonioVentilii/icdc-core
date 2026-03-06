use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::SeriesId;

/// Input parameters for retrieving a margin account.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetMarginAccountParams {
    /// Whether to force a recalculation of the margin status.
    pub refresh: Option<bool>,
}

/// Input parameters for retrieving a user's position in a series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetPositionParams {
    /// The derivative series identifier.
    pub series_id: SeriesId,
}
