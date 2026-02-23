use candid::{CandidType, Deserialize};
use serde::Serialize;

use crate::types::{
    errors::{
        DepositCollateralError, MarginAccountError, SettlementError, TradeError,
        WithdrawCollateralError,
    },
    margin::MarginAccount,
};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum DepositCollateralResult {
    Ok,
    Err(DepositCollateralError),
}
impl From<Result<(), DepositCollateralError>> for DepositCollateralResult {
    fn from(value: Result<(), DepositCollateralError>) -> Self {
        match value {
            Ok(_) => DepositCollateralResult::Ok,
            Err(e) => DepositCollateralResult::Err(e),
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum WithdrawCollateralResult {
    Ok,
    Err(WithdrawCollateralError),
}
impl From<Result<(), WithdrawCollateralError>> for WithdrawCollateralResult {
    fn from(value: Result<(), WithdrawCollateralError>) -> Self {
        match value {
            Ok(_) => WithdrawCollateralResult::Ok,
            Err(e) => WithdrawCollateralResult::Err(e),
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SubmitMatchedTradeResult {
    Ok(bool),
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

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SettleSeriesResult {
    Ok,
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

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum AcceptPositionTransferResult {
    Ok(bool),
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

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum GetMarginAccountResult {
    Ok(MarginAccount),
    Err(MarginAccountError),
}
impl From<Result<MarginAccount, MarginAccountError>> for GetMarginAccountResult {
    fn from(value: Result<MarginAccount, MarginAccountError>) -> Self {
        match value {
            Ok(v) => GetMarginAccountResult::Ok(v),
            Err(e) => GetMarginAccountResult::Err(e),
        }
    }
}
