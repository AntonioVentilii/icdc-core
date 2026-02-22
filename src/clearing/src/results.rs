use candid::{CandidType, Deserialize};
use serde::Serialize;

use crate::error::ClearingError;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum WithdrawCollateralResult {
    Ok,
    Err(ClearingError),
}
impl From<Result<(), ClearingError>> for WithdrawCollateralResult {
    fn from(value: Result<(), ClearingError>) -> Self {
        match value {
            Ok(_) => WithdrawCollateralResult::Ok,
            Err(e) => WithdrawCollateralResult::Err(e),
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SubmitMatchedTradeResult {
    Ok(bool),
    Err(ClearingError),
}
impl From<Result<bool, ClearingError>> for SubmitMatchedTradeResult {
    fn from(value: Result<bool, ClearingError>) -> Self {
        match value {
            Ok(v) => SubmitMatchedTradeResult::Ok(v),
            Err(e) => SubmitMatchedTradeResult::Err(e),
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SettleSeriesResult {
    Ok,
    Err(ClearingError),
}
impl From<Result<(), ClearingError>> for SettleSeriesResult {
    fn from(value: Result<(), ClearingError>) -> Self {
        match value {
            Ok(_) => SettleSeriesResult::Ok,
            Err(e) => SettleSeriesResult::Err(e),
        }
    }
}
