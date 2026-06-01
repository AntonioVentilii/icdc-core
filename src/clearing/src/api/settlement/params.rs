use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::{BalanceDomain, SeriesId, SettlementInput};

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

/// Input parameters for [`list_settled_series`](super::list_settled_series).
///
/// A series is considered settled the moment a settlement plan is opened for it
/// (any [`PlanStatus`](crate::types::plans::PlanStatus)), which is also when it
/// stops being tradeable. Front ends use this to subtract the resolved set from
/// the registry's open/unexpired catalog page.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default)]
pub struct ListSettledSeriesParams {
    /// When set, only return settled series whose plan recorded this balance
    /// domain. Lets a caller scoped to one domain (e.g. a flow deck) shrink the
    /// response to the relevant subset.
    pub balance_domain: Option<BalanceDomain>,
    /// Resume after this series id (exclusive). `None` starts from the lowest
    /// series id. Settled series ids are returned in ascending order, so paging
    /// with the previous response's `next_cursor` is stable.
    pub start_after: Option<SeriesId>,
    /// Maximum number of ids to return. `None` returns all remaining ids.
    pub limit: Option<u64>,
}
