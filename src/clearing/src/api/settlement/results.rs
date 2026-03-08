use candid::{CandidType, Deserialize};
use serde::Serialize;

use super::errors::SettlementError;

/// Result of a derivative series settlement request.
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
    pub fn ok() -> Self {
        SettleSeriesResult::Ok
    }

    pub fn processing() -> Self {
        SettleSeriesResult::Processing
    }
}

impl From<Result<(), SettlementError>> for SettleSeriesResult {
    fn from(value: Result<(), SettlementError>) -> Self {
        match value {
            Ok(_) => SettleSeriesResult::Ok,
            Err(e) => SettleSeriesResult::Err(e),
        }
    }
}
