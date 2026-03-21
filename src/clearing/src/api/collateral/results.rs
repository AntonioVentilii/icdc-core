use candid::{CandidType, Deserialize};
use serde::Serialize;

use super::errors::{BlockingError, DepositCollateralError, WithdrawCollateralError};

/// Outcome of a collateral blocking operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum BlockCollateralResult {
    /// Collateral was successfully blocked.
    Ok,
    /// Failed to block collateral.
    Err(BlockingError),
}
impl From<Result<(), BlockingError>> for BlockCollateralResult {
    fn from(value: Result<(), BlockingError>) -> Self {
        match value {
            Ok(()) => BlockCollateralResult::Ok,
            Err(e) => BlockCollateralResult::Err(e),
        }
    }
}

/// Outcome of a collateral unblocking operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum UnblockCollateralResult {
    /// Collateral was successfully unblocked.
    Ok,
    /// Failed to unblock collateral.
    Err(BlockingError),
}
impl From<Result<(), BlockingError>> for UnblockCollateralResult {
    fn from(value: Result<(), BlockingError>) -> Self {
        match value {
            Ok(()) => UnblockCollateralResult::Ok,
            Err(e) => UnblockCollateralResult::Err(e),
        }
    }
}

/// Outcome of a collateral deposit operation.
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
            Ok(()) => DepositCollateralResult::Ok,
            Err(e) => DepositCollateralResult::Err(e),
        }
    }
}

/// Outcome of a collateral withdrawal operation.
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
            Ok(()) => WithdrawCollateralResult::Ok,
            Err(e) => WithdrawCollateralResult::Err(e),
        }
    }
}
