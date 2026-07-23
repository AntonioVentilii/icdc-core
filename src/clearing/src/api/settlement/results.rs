use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::SeriesId;

use super::errors::SettlementError;
use crate::types::plans::SettlementStatusView;

/// Outcome of a derivative series settlement request.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SettleSeriesResult {
    /// Settlement plan was successfully created and all processing is complete.
    Ok,
    /// The settlement is in progress but incomplete due to processing limits.
    /// The caller should call `settle_series` again to continue.
    Processing,
    /// Failed to initiate settlement.
    Err(SettlementError),
}

impl SettleSeriesResult {
    #[must_use]
    pub fn ok() -> Self {
        SettleSeriesResult::Ok
    }

    #[must_use]
    pub fn processing() -> Self {
        SettleSeriesResult::Processing
    }
}

impl From<Result<(), SettlementError>> for SettleSeriesResult {
    fn from(value: Result<(), SettlementError>) -> Self {
        match value {
            Ok(()) => SettleSeriesResult::Ok,
            Err(e) => SettleSeriesResult::Err(e),
        }
    }
}

/// Outcome of one chunk of the settlement-event backfill.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default)]
pub struct BackfillSettlementEventsResult {
    /// Number of finalized plans visited in this chunk.
    pub plans_scanned: u64,
    /// Number of `Settled` events newly written in this chunk.
    pub events_emitted: u64,
    /// Number of `SettlementPosition`s skipped because their series already has
    /// at least one pre-existing `Settled` event (idempotent skip). Counted
    /// per-position to mirror the live emission contract, which writes one
    /// event per position.
    pub events_skipped: u64,
    /// When `Some`, pass back as `start_after` to continue. `None` means the
    /// backfill is complete.
    pub next_cursor: Option<SeriesId>,
}

/// One series' settlement status, paired with the id it was requested for.
///
/// `status` is `Some` when a settlement plan exists for the series (in any
/// [`PlanStatus`](crate::types::plans::PlanStatus) — a plan is opened the moment
/// settlement begins) and `None` otherwise. A `None` covers both a still-open
/// series and an unknown id; the two are not distinguished here. The `series_id`
/// is echoed on each entry so callers can align results with their requested ids
/// and attribute a `None`, which carries no id of its own.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SeriesSettlementStatus {
    /// The series this status is for (echoes the requested id).
    pub series_id: SeriesId,
    /// The settlement progress, or `None` if the series has no settlement plan
    /// yet (still open / not being settled).
    pub status: Option<SettlementStatusView>,
}

/// A page of settled (resolved) series ids.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default)]
pub struct SettledSeriesPage {
    /// Settled series ids in this page, ascending.
    pub items: Vec<SeriesId>,
    /// When `Some`, pass back as `start_after` to fetch the next page. `None`
    /// means the last page has been returned.
    pub next_cursor: Option<SeriesId>,
}
