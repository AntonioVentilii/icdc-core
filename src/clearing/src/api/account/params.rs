use candid::{CandidType, Deserialize, Principal};
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

/// Input parameters for the `aggregate_lean` query.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AggregateLeanParams {
    /// The derivative series to aggregate the lean over.
    pub series_id: SeriesId,
    /// The set of principals to aggregate over. The set is caller-supplied and
    /// scanned in full, so it is capped — any list longer than the cap is
    /// truncated. Duplicate principals are de-duplicated and never affect the
    /// counts. The clearing layer ascribes no meaning to the set; how it is
    /// chosen is entirely a consumer concern.
    pub principals: Vec<Principal>,
}
