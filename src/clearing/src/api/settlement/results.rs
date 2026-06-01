use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::SeriesId;

use super::errors::SettlementError;

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

/// A page of settled (resolved) series ids.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default)]
pub struct SettledSeriesPage {
    /// Settled series ids in this page, ascending.
    pub items: Vec<SeriesId>,
    /// When `Some`, pass back as `start_after` to fetch the next page. `None`
    /// means the last page has been returned.
    pub next_cursor: Option<SeriesId>,
}
