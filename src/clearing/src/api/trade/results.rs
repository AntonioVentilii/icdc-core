use candid::{CandidType, Deserialize};
use serde::Serialize;

use crate::{api::trade::errors::TradeError, types::event::SeriesTradePoint};

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

/// A page of executed trades scoped to a single series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default)]
pub struct SeriesTradeHistoryPage {
    /// Executed trades for the series in this page, one point per trade, ordered
    /// by `event_id` ascending (i.e. execution order).
    pub items: Vec<SeriesTradePoint>,
    /// When `Some`, pass back as `start_after` to fetch the next page (it is the
    /// `event_id` of the last trade returned). `None` means the last page has
    /// been returned.
    pub next_cursor: Option<u64>,
}
