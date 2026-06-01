use candid::{CandidType, Deserialize};
use serde::Serialize;

use crate::{api::trade::errors::TradeError, types::event::Event};

/// Outcome of a matched trade submission.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SubmitMatchedTradeResult {
    /// Returns `true` if the trade was successfully processed.
    Ok(bool),
    /// Failed to submit or match the trade.
    Err(TradeError),
}
impl From<Result<bool, TradeError>> for SubmitMatchedTradeResult {
    fn from(value: Result<bool, TradeError>) -> Self {
        match value {
            Ok(v) => SubmitMatchedTradeResult::Ok(v),
            Err(e) => SubmitMatchedTradeResult::Err(e),
        }
    }
}

/// Outcome of a position transfer acceptance.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum AcceptPositionTransferResult {
    /// Returns `true` if the transfer was successfully accepted and processed.
    Ok(bool),
    /// Failed to accept the position transfer.
    Err(TradeError),
}
impl From<Result<bool, TradeError>> for AcceptPositionTransferResult {
    fn from(value: Result<bool, TradeError>) -> Self {
        match value {
            Ok(v) => AcceptPositionTransferResult::Ok(v),
            Err(e) => AcceptPositionTransferResult::Err(e),
        }
    }
}

/// Stable, exclusive cursor for paging a series' executed-trade history.
///
/// Events are ordered by `(timestamp, event_id)`, so the cursor carries both:
/// `event_id` alone is not monotonic in timestamp order because backfilled rows
/// can have a newer id but an older timestamp.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TradeHistoryCursor {
    /// Timestamp (ns) of the last event returned in the previous page.
    pub timestamp: u64,
    /// Event id of the last event returned in the previous page.
    pub event_id: u64,
}

/// A page of executed-trade events scoped to a single series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default)]
pub struct SeriesTradeHistoryPage {
    /// Executed events for the series in this page, ordered by
    /// `(timestamp, event_id)` ascending.
    pub items: Vec<Event>,
    /// When `Some`, pass back as `start_after` to fetch the next page. `None`
    /// means the last page has been returned.
    pub next_cursor: Option<TradeHistoryCursor>,
}
