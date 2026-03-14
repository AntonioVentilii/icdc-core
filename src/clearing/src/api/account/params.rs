use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::{BalanceDomain, OutcomeId, SeriesId};

/// Input parameters for retrieving an account state.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetAccountStateParams {
    /// Whether to force a recalculation of the margin status (e.g. refresh from ledgers).
    pub refresh: Option<bool>,
    /// The specific balance domain to query (defaults to Settlement if not provided).
    pub domain: Option<BalanceDomain>,
}

/// Input parameters for retrieving a user's position in a series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetPositionParams {
    /// The derivative series identifier.
    pub series_id: SeriesId,
    /// Optional outcome identifier for categorical markets.
    pub outcome_id: Option<OutcomeId>,
}
