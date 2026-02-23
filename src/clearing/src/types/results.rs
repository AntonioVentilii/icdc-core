use candid::{CandidType, Deserialize};
use serde::Serialize;

use crate::{types::margin::MarginAccount, ClearingError};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum DepositCollateralResult {
    Ok,
    Err(ClearingError),
}
impl From<Result<(), ClearingError>> for DepositCollateralResult {
    fn from(value: Result<(), ClearingError>) -> Self {
        match value {
            Ok(_) => DepositCollateralResult::Ok,
            Err(e) => DepositCollateralResult::Err(e),
        }
    }
}

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

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum AcceptPositionTransferResult {
    Ok(bool),
    Err(ClearingError),
}
impl From<Result<bool, ClearingError>> for AcceptPositionTransferResult {
    fn from(value: Result<bool, ClearingError>) -> Self {
        match value {
            Ok(v) => AcceptPositionTransferResult::Ok(v),
            Err(e) => AcceptPositionTransferResult::Err(e),
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum GetMarginAccountResult {
    Ok(MarginAccount),
    Err(ClearingError),
}
impl From<Result<MarginAccount, ClearingError>> for GetMarginAccountResult {
    fn from(value: Result<MarginAccount, ClearingError>) -> Self {
        match value {
            Ok(v) => GetMarginAccountResult::Ok(v),
            Err(e) => GetMarginAccountResult::Err(e),
        }
    }
}
