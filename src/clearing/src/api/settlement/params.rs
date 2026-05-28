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

/// Input parameters for the one-shot settlement-event backfill. Synthesizes
/// `EventType::Settled` events for finalized plans that resolved before the
/// per-user event emission was added to settlement.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default)]
pub struct BackfillSettlementEventsParams {
    /// Resume after this series id (exclusive). `None` starts from the lowest
    /// series id. Callers re-invoke with the previous response's `next_cursor`
    /// until it comes back as `None`.
    pub start_after: Option<SeriesId>,
}
