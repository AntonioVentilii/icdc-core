use candid::{CandidType, Deserialize};
use serde::Serialize;

use super::errors::SettlementError;

/// Result of a derivative series settlement request.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SettleSeriesResult {
    /// Settlement plan was successfully created or is already executing.
    Ok,
    /// Failed to initiate settlement.
    Err(SettlementError),
}
impl From<Result<(), SettlementError>> for SettleSeriesResult {
    fn from(value: Result<(), SettlementError>) -> Self {
        match value {
            Ok(_) => SettleSeriesResult::Ok,
            Err(e) => SettleSeriesResult::Err(e),
        }
    }
}
