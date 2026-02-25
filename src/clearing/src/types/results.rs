use candid::{CandidType, Deserialize};
use serde::Serialize;

use crate::types::{
    errors::{
        DepositCollateralError, MarginAccountError, SettlementError, TradeError,
        WithdrawCollateralError,
    },
    margin::MarginAccount,
};

/// Result of a collateral deposit operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum DepositCollateralResult {
    /// Deposit was successfully planned or executed.
    Ok,
    /// Failed to process deposit.
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

/// Result of a collateral withdrawal operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum WithdrawCollateralResult {
    /// Withdrawal was successfully planned or executed.
    Ok,
    /// Failed to process withdrawal.
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

/// Result of a margin account retrieval request.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum GetMarginAccountResult {
    /// Successfully retrieved the margin account details.
    Ok(MarginAccount),
    /// Failed to retrieve the margin account.
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
