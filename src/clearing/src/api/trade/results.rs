use candid::{CandidType, Deserialize};
use serde::Serialize;

use crate::api::trade::errors::TradeError;

/// Result of a matched trade submission.
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

/// Result of a position transfer acceptance.
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
